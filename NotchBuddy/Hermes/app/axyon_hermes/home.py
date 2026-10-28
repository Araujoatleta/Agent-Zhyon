"""AXYON's home straight from the iPhone, with no computer: the user's devices on the home's Wi-Fi, reached by the
iPhone itself, as the computer reaches them (windows/src-tauri/src/smart_home: samsung.rs, android.rs,
home_assistant.rs), in Python.

  * Samsung TVs since 2016 (Tizen): GET http://<tv>:8001/api/v2/ says what the TV is and whether it is on; the remote
    is a WebSocket, wss://<tv>:8002/api/v2/channels/samsung.remote.control. The first time, the TV asks on its screen
    whether to allow AXYON, then gives a token. Its self-signed certificate is pinned on first contact.
  * Android TVs and boxes over ADB, port 5555: an RSA-2048 key made here (kept in the iPhone's Keychain, never in a
    file), offered once (the device asks "Allow USB debugging?"), then a fixed list of commands: keys, the device's
    own apps, reading its state. Never a command typed by anyone, the model included.
  * Home Assistant: its REST API with a long-lived token the user pasted in the app (Keychain), and only the services
    listed in ACTIONS: no lock, no alarm, nothing outside that list.

Device settings (address, name, room, the TV's certificate pin) are in AXYON's folder; tokens and keys are in the
Keychain, through the app (`bridge`, kind "secret"). Only addresses on the local network (or the user's tailnet) are
contacted, except a Home Assistant reached over https. Plugs and switches ask the owner's Face ID before acting when
Hermes asks; a tap in the app is the owner's own.
"""

from __future__ import annotations

import base64
import hashlib
import ipaddress
import json
import os
import re
import secrets
import socket
import ssl
import struct
import threading
import time
import unicodedata
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from typing import Any, Callable, Optional

TOOLSET = "coucou"
# The name the TVs show when they ask whether to allow this iPhone.
NAME = "AXYON"
TIMEOUT = 5.0
MAX_DEVICES = 60
CGNAT = ipaddress.ip_network("100.64.0.0/10")


class HomeError(Exception):
    """What went wrong, in words the user (and the model) can act on."""


# ── The network ──────────────────────────────────────────────────────────────


def is_local(ip: str) -> bool:
    """A home network's address (private, link-local) or the user's tailnet; never this iPhone itself."""
    try:
        a = ipaddress.ip_address(ip.split("%")[0])
    except ValueError:
        return False
    if a.version == 6 and a.ipv4_mapped:
        a = a.ipv4_mapped
    if a.is_loopback or a.is_unspecified or a.is_multicast:
        return False
    return a.is_private or a.is_link_local or (a.version == 4 and a in CGNAT)


def resolve_local(host: str, port: int) -> str:
    """The address of `host`, which must be on the local network."""
    host = (host or "").strip().strip("[]")
    if not host:
        raise HomeError("No address.")
    try:
        infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except socket.gaierror:
        raise HomeError(f"{host} was not found on the network.") from None
    for info in infos:
        if is_local(str(info[4][0])):
            return str(info[4][0])
    raise HomeError("Only devices on your local network can be used.")


# LAN requests never go through a proxy.
_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def http(method: str, url: str, body: Any = None, headers: Optional[dict[str, str]] = None,
         timeout: float = TIMEOUT, limit: int = 2_000_000) -> tuple[int, bytes]:
    data = None
    headers = dict(headers or {})
    if body is not None:
        data = json.dumps(body).encode()
        headers.setdefault("Content-Type", "application/json")
    request = urllib.request.Request(url, data=data, method=method, headers=headers)
    try:
        with _OPENER.open(request, timeout=timeout) as response:
            return response.status, response.read(limit)
    except urllib.error.HTTPError as e:
        try:
            return e.code, e.read(limit)
        except Exception:
            return e.code, b""
    except (urllib.error.URLError, OSError) as e:
        reason = getattr(e, "reason", e)
        raise HomeError(f"It did not answer ({reason}).") from None


def _json(raw: bytes) -> Any:
    try:
        return json.loads(raw.decode("utf-8", "replace"))
    except ValueError:
        return None


def wake_on_lan(mac: str, host: str) -> bool:
    """The magic packet to the device's own address: a TV in network standby may wake (iOS allows no broadcast)."""
    digits = re.sub(r"[^0-9a-fA-F]", "", mac or "")
    if len(digits) != 12:
        return False
    packet = b"\xff" * 6 + bytes.fromhex(digits) * 16
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
            for port in (9, 7):
                s.sendto(packet, (host, port))
        return True
    except OSError:
        return False


def _fold(text: str) -> str:
    """For matching a name the user said: no case, no accents, single spaces."""
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(c for c in text if not unicodedata.combining(c))
    return re.sub(r"\s+", " ", text).strip().casefold()


# ── Samsung TVs (Tizen) ──────────────────────────────────────────────────────

SAMSUNG_REST = 8001
SAMSUNG_SECURE = 8002
PAIR_WAIT = 31.0
OPEN_WAIT = 8.0
_SAMSUNG_KEYS = {
    "home": "KEY_HOME", "back": "KEY_RETURN", "ok": "KEY_ENTER", "up": "KEY_UP", "down": "KEY_DOWN",
    "left": "KEY_LEFT", "right": "KEY_RIGHT", "play_pause": "KEY_PLAY", "menu": "KEY_MENU", "source": "KEY_SOURCE",
    "exit": "KEY_EXIT", "info": "KEY_INFO", "guide": "KEY_GUIDE",
}


def parse_samsung(v: Any) -> Optional[dict[str, Any]]:
    """What GET /api/v2/ says, or None when it is not a Samsung TV's answer."""
    if not isinstance(v, dict) or not isinstance(v.get("device"), dict):
        return None
    device = v["device"]

    def s(key: str) -> str:
        value = device.get(key)
        return value.strip() if isinstance(value, str) else ""

    kind = s("type") or (v.get("type") if isinstance(v.get("type"), str) else "")
    if "samsung" not in kind.lower():
        return None
    name = (s("name") or (v.get("name") if isinstance(v.get("name"), str) else "")).strip()
    name = re.sub(r"^\[TV\]\s*", "", name).strip()
    power = s("PowerState")
    return {
        "name": name or "Samsung TV",
        "model": s("modelName"),
        "os": s("OS"),
        "version": (v.get("version") or "").strip() if isinstance(v.get("version"), str) else "",
        "power": (power.lower() == "on") if power else None,
        "tokenAuth": s("TokenAuthSupport").lower() == "true",
        "mac": s("wifiMac") or None,
        "frame": s("FrameTVSupport").lower() == "true",
    }


def samsung_info(host: str, port: int = SAMSUNG_REST, timeout: float = 3.0) -> Optional[dict[str, Any]]:
    ip = resolve_local(host, port)
    status, raw = http("GET", f"http://{_in_url(ip)}:{port}/api/v2/", timeout=timeout, limit=256 * 1024)
    return parse_samsung(_json(raw)) if status == 200 else None


def _in_url(ip: str) -> str:
    return f"[{ip}]" if ":" in ip else ip


def samsung_remote_url(host: str, port: int, secure: bool, token: Optional[str]) -> str:
    scheme = "wss" if secure else "ws"
    name = base64.b64encode(NAME.encode()).decode()
    url = f"{scheme}://{_in_url(host)}:{port}/api/v2/channels/samsung.remote.control?name={name}"
    if secure and token and len(token) <= 64 and token.isascii() and token.isalnum():
        url += f"&token={token}"
    return url


def _samsung_app_ok(app_id: str) -> bool:
    return bool(re.fullmatch(r"[A-Za-z0-9._-]{1,64}", app_id or ""))


def parse_samsung_apps(v: Any) -> list[dict[str, Any]]:
    data = v.get("data") if isinstance(v, dict) else None
    data = data.get("data") if isinstance(data, dict) else None
    apps = []
    for a in data if isinstance(data, list) else []:
        if not isinstance(a, dict):
            continue
        app_id = str(a.get("appId") or "").strip()
        name = str(a.get("name") or "").strip()
        if _samsung_app_ok(app_id) and name and len(name) <= 80:
            kind = a.get("app_type") if isinstance(a.get("app_type"), int) else 2
            apps.append({"id": app_id, "name": name, "kind": kind})
    apps.sort(key=lambda a: _fold(a["name"]))
    return apps


class SamsungRemote:
    """The TV's remote: a WebSocket that stays open for a few keys, then closes."""

    def __init__(self, ws: Any, token: Optional[str], pin: Optional[str]):
        self.ws = ws
        self.token = token
        self.pin = pin

    @classmethod
    def open(cls, host: str, secure: bool, token: Optional[str], pin: Optional[str], pairing: bool,
             port: Optional[int] = None) -> "SamsungRemote":
        from websockets.sync.client import connect

        port = port or (SAMSUNG_SECURE if secure else SAMSUNG_REST)
        ip = resolve_local(host, port)
        context = None
        if secure:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
            context.check_hostname = False
            context.verify_mode = ssl.CERT_NONE  # self-signed: the pin below is the check
        wait = PAIR_WAIT if pairing else OPEN_WAIT
        try:
            ws = connect(samsung_remote_url(ip, port, secure, token), ssl=context, proxy=None, open_timeout=TIMEOUT,
                         close_timeout=1, ping_interval=None, max_size=2 ** 20, compression=None,
                         user_agent_header=None)
        except Exception as e:
            raise HomeError(f"The TV did not answer: is it on and on the same Wi-Fi? ({e})") from None
        seen = None
        if secure:
            try:
                der = ws.socket.getpeercert(binary_form=True)
                seen = hashlib.sha256(der).hexdigest() if der else None
            except Exception:
                seen = None
            if pin and seen != pin:
                ws.close()
                raise HomeError("The TV's certificate changed since it was added: remove it and add it again.")
        deadline = time.monotonic() + wait
        new_token = token
        try:
            while True:
                left = deadline - time.monotonic()
                if left <= 0:
                    raise HomeError(cls._no_answer(pairing))
                try:
                    raw = ws.recv(timeout=left)
                except TimeoutError:
                    raise HomeError(cls._no_answer(pairing)) from None
                v = _json(raw.encode() if isinstance(raw, str) else raw)
                event = v.get("event") if isinstance(v, dict) else None
                if event == "ms.channel.connect":
                    data = v.get("data") if isinstance(v.get("data"), dict) else {}
                    given = data.get("token")
                    if isinstance(given, str) and given:
                        new_token = given
                    return cls(ws, new_token, seen or pin)
                if event == "ms.channel.unauthorized":
                    raise HomeError("The TV did not allow AXYON. On the TV: Settings → General → External Device "
                                    "Manager → Device Connection Manager, allow AXYON, then try again.")
                if event == "ms.channel.timeOut":
                    raise HomeError(cls._no_answer(pairing))
                if event == "ms.error":
                    data = v.get("data") if isinstance(v.get("data"), dict) else {}
                    raise HomeError(f"The TV said: {data.get('message') or 'error'}.")
        except BaseException:
            try:
                ws.close()
            except Exception:
                pass
            raise

    @staticmethod
    def _no_answer(pairing: bool) -> str:
        if pairing:
            return "Nobody answered on the TV: allow AXYON there when it asks, then try again."
        return "The TV did not open its remote."

    def _send(self, v: dict[str, Any]) -> None:
        self.ws.send(json.dumps(v))

    def key(self, key: str, cmd: str = "Click") -> None:
        self._send({"method": "ms.remote.control",
                    "params": {"Cmd": cmd, "DataOfCmd": key, "Option": "false", "TypeOfRemote": "SendRemoteKey"}})

    def _event(self, name: str, wait: float) -> Optional[dict[str, Any]]:
        deadline = time.monotonic() + wait
        while True:
            left = deadline - time.monotonic()
            if left <= 0:
                return None
            try:
                raw = self.ws.recv(timeout=left)
            except TimeoutError:
                return None
            v = _json(raw.encode() if isinstance(raw, str) else raw)
            if isinstance(v, dict) and v.get("event") == name:
                return v

    def apps(self) -> Optional[list[dict[str, Any]]]:
        self._send({"method": "ms.channel.emit", "params": {"event": "ed.installedApp.get", "to": "host"}})
        v = self._event("ed.installedApp.get", 5.0)
        return parse_samsung_apps(v) if v is not None else None

    def launch(self, app: dict[str, Any]) -> bool:
        if not _samsung_app_ok(str(app.get("id") or "")):
            return False
        data = {"action_type": "NATIVE_LAUNCH" if app.get("kind") == 4 else "DEEP_LINK", "appId": app["id"],
                "metaTag": ""}
        self._send({"method": "ms.channel.emit", "params": {"event": "ed.apps.launch", "to": "host", "data": data}})
        v = self._event("ed.apps.launch", 3.0)
        code = v.get("data") if isinstance(v, dict) else None
        return not (isinstance(code, int) and code >= 400)

    def close(self) -> None:
        time.sleep(0.3)  # the keys a moment to be taken
        try:
            self.ws.close()
        except Exception:
            pass


# ── Android over ADB ─────────────────────────────────────────────────────────

ADB_PORT = 5555
_CNXN, _AUTH, _OPEN, _OKAY, _CLSE, _WRTE, _STLS = (struct.unpack("<I", w)[0] for w in
                                                   (b"CNXN", b"AUTH", b"OPEN", b"OKAY", b"CLSE", b"WRTE", b"STLS"))
ADB_VERSION = 0x01000001
ADB_MAX_PAYLOAD = 256 * 1024
_TOKEN, _SIGNATURE, _PUBLIC_KEY = 1, 2, 3
ALLOW_WAIT = 60.0
_SHA1_INFO = bytes([0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2B, 0x0E, 0x03, 0x02, 0x1A, 0x05, 0x00, 0x04, 0x14])
_E = 65537
KEYCODES = {
    "power": 26, "volume_up": 24, "volume_down": 25, "mute": 164, "sleep": 223, "wakeup": 224, "home": 3, "back": 4,
    "up": 19, "down": 20, "left": 21, "right": 22, "ok": 23, "play_pause": 85, "next": 87, "previous": 88,
    "channel_up": 166, "channel_down": 167, "menu": 82,
}


def adb_message(command: int, arg0: int, arg1: int, data: bytes = b"") -> bytes:
    check = sum(data) & 0xFFFFFFFF
    return struct.pack("<6I", command, arg0, arg1, len(data), check, command ^ 0xFFFFFFFF) + data


def _read_exact(sock: socket.socket, n: int) -> bytes:
    out = bytearray()
    while len(out) < n:
        chunk = sock.recv(n - len(out))
        if not chunk:
            raise ConnectionError("closed")
        out += chunk
    return bytes(out)


def adb_read(sock: socket.socket) -> tuple[int, int, int, bytes]:
    command, arg0, arg1, length, _check, magic = struct.unpack("<6I", _read_exact(sock, 24))
    if magic != command ^ 0xFFFFFFFF or length > 1024 * 1024:
        raise ConnectionError("not an ADB message")
    return command, arg0, arg1, _read_exact(sock, length) if length else b""


def _probably_prime(n: int, rounds: int = 40) -> bool:
    if n < 4:
        return n in (2, 3)
    d, s = n - 1, 0
    while d % 2 == 0:
        d //= 2
        s += 1
    for _ in range(rounds):
        a = secrets.randbelow(n - 3) + 2
        x = pow(a, d, n)
        if x in (1, n - 1):
            continue
        for _ in range(s - 1):
            x = x * x % n
            if x == n - 1:
                break
        else:
            return False
    return True


_SMALL_PRIMES = [p for p in range(3, 2000, 2) if all(p % q for q in range(3, int(p ** 0.5) + 1, 2))]


def _random_prime(bits: int) -> int:
    while True:
        candidate = secrets.randbits(bits) | (3 << (bits - 2)) | 1
        for _ in range(4096):
            if candidate.bit_length() != bits:
                break
            if (all(candidate % p for p in _SMALL_PRIMES) and candidate % _E != 1
                    and _probably_prime(candidate)):
                return candidate
            candidate += 2


class AdbKey:
    """This iPhone's ADB key: RSA-2048, e = 65537, as adb makes it."""

    BITS = 2048

    def __init__(self, p: int, q: int):
        self.p, self.q = p, q
        self.n = p * q
        self.d = pow(_E, -1, (p - 1) * (q - 1))

    @classmethod
    def generate(cls) -> "AdbKey":
        while True:
            p, q = _random_prime(cls.BITS // 2), _random_prime(cls.BITS // 2)
            if p != q and (p * q).bit_length() == cls.BITS:
                return cls(p, q)

    def to_secret(self) -> str:
        return json.dumps({"p": format(self.p, "x"), "q": format(self.q, "x")})

    @classmethod
    def from_secret(cls, text: Optional[str]) -> Optional["AdbKey"]:
        try:
            v = json.loads(text or "")
            key = cls(int(v["p"], 16), int(v["q"], 16))
            return key if key.n.bit_length() == cls.BITS else None
        except (ValueError, KeyError, TypeError):
            return None

    @property
    def size(self) -> int:
        return (self.n.bit_length() + 7) // 8

    def sign(self, token: bytes) -> bytes:
        """PKCS#1 v1.5 over SHA-1's DigestInfo, the token standing for the digest: what adbd checks."""
        tail = _SHA1_INFO + token
        if len(tail) + 11 > self.size:
            raise HomeError("The device sent a token too long to sign.")
        block = b"\x00\x01" + b"\xff" * (self.size - len(tail) - 3) + b"\x00" + tail
        return pow(int.from_bytes(block, "big"), self.d, self.n).to_bytes(self.size, "big")

    def android_public(self) -> bytes:
        """The public key as Android keeps it (android_pubkey.cpp): words, -1/n mod 2^32, n, R^2 mod n, e."""
        words = self.size // 4
        n0 = self.n & 0xFFFFFFFF
        inverse = pow(n0, -1, 1 << 32)
        rr = pow(2, 2 * self.n.bit_length(), self.n)
        return (struct.pack("<II", words, (-inverse) & 0xFFFFFFFF) + self.n.to_bytes(words * 4, "little")
                + rr.to_bytes(words * 4, "little") + struct.pack("<I", _E))

    def public_line(self) -> bytes:
        return base64.b64encode(self.android_public()) + b" axyon@iphone\x00"


class Adb:
    """A device that let this iPhone in, ready for the commands of the fixed list."""

    def __init__(self, sock: socket.socket, banner: str):
        self.sock = sock
        self.banner = banner
        self.next_id = 1

    @classmethod
    def connect(cls, host: str, port: int, key: Optional[AdbKey], ask: bool) -> "Adb":
        ip = resolve_local(host, port)
        try:
            sock = socket.create_connection((ip, port), timeout=3.0)
        except OSError as e:
            raise HomeError(f"The device did not answer on port {port}: is network debugging on? ({e})") from None
        sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
        try:
            sock.sendall(adb_message(_CNXN, ADB_VERSION, ADB_MAX_PAYLOAD, b"host::\x00"))
            signed = offered = False
            while True:
                sock.settimeout(ALLOW_WAIT if offered else 8.0)
                try:
                    command, arg0, _arg1, data = adb_read(sock)
                except (OSError, ConnectionError):
                    if offered:
                        raise HomeError("The device did not allow this iPhone: accept \"Allow USB debugging?\" on "
                                        "its screen (with \"Always allow\"), then try again.") from None
                    raise HomeError("The device closed the connection.") from None
                if command == _CNXN:
                    sock.settimeout(15.0)
                    return cls(sock, data.decode("utf-8", "replace").rstrip("\x00"))
                if command == _STLS:
                    raise HomeError("This device only offers Android's wireless debugging with a pairing code. Turn "
                                    "on network debugging on port 5555 in its Developer options instead.")
                if command == _AUTH and arg0 == _TOKEN:
                    if key is not None and not signed:
                        signed = True
                        sock.sendall(adb_message(_AUTH, _SIGNATURE, 0, key.sign(data)))
                    elif key is not None and ask and not offered:
                        offered = True
                        sock.sendall(adb_message(_AUTH, _PUBLIC_KEY, 0, key.public_line()))
                    else:
                        raise HomeError("The device does not know this iPhone yet: add it again from the Home tab.")
        except BaseException:
            sock.close()
            raise

    def shell(self, command: str, limit: int = 4 * 1024 * 1024) -> str:
        local = self.next_id
        self.next_id += 1
        self.sock.sendall(adb_message(_OPEN, local, 0, b"shell:" + command.encode() + b"\x00"))
        out = bytearray()
        remote = None
        while True:
            cmd, arg0, arg1, data = adb_read(self.sock)
            if arg1 != local:
                continue
            if cmd == _OKAY:
                remote = arg0
            elif cmd == _WRTE:
                if len(out) < limit:
                    out += data[: limit - len(out)]
                self.sock.sendall(adb_message(_OKAY, local, arg0))
            elif cmd == _CLSE:
                if remote is None:
                    raise HomeError("The device refused the command.")
                self.sock.sendall(adb_message(_CLSE, local, arg0))
                return out.decode("utf-8", "replace")

    def close(self) -> None:
        try:
            self.sock.close()
        except OSError:
            pass


def adb_banner_value(banner: str, key: str) -> str:
    props = banner.split("::", 1)[1] if "::" in banner else ""
    for kv in props.split(";"):
        if kv.startswith(key + "="):
            return kv[len(key) + 1:]
    return ""


ADB_ABOUT = ("getprop ro.product.model; getprop ro.build.version.release; getprop ro.build.version.sdk; "
             "settings get global device_name")


def adb_parse_about(out: str) -> dict[str, Any]:
    lines = [line.strip() for line in out.splitlines()] + ["", "", "", ""]
    try:
        sdk = int(lines[2])
    except ValueError:
        sdk = 0
    name = lines[3] if lines[3] != "null" else ""
    return {"model": lines[0], "release": lines[1], "sdk": sdk, "name": name[:60]}


def adb_state_command(sdk: int) -> str:
    if sdk >= 23:
        return ("echo @@power; dumpsys power | grep -E 'mWakefulness=|Display Power: state=|mScreenOn='; echo @@audio; "
                "dumpsys audio | grep -E -A 12 '^- STREAM_MUSIC:'; echo @@top; dumpsys activity activities | "
                "grep -E 'mResumedActivity|topResumedActivity'")
    return "echo @@power; dumpsys power; echo @@audio; dumpsys audio; echo @@top; dumpsys activity activities"


def _section(out: str, name: str) -> str:
    lines = out.splitlines()
    try:
        start = [line.strip() for line in lines].index(f"@@{name}") + 1
    except ValueError:
        return ""
    taken = []
    for line in lines[start:]:
        if line.strip().startswith("@@"):
            break
        taken.append(line)
    return "\n".join(taken)


def _number_after(text: str, label: str) -> Optional[int]:
    at = text.find(label)
    if at < 0:
        return None
    m = re.match(r"\s*(\d+)", text[at + len(label):])
    return int(m.group(1)) if m else None


def adb_parse_state(out: str) -> dict[str, Any]:
    power = _section(out, "power")
    state: dict[str, Any] = {}
    m = re.search(r"mWakefulness=(\w+)", power)
    if m:
        state["on"] = m.group(1) == "Awake"
    elif "Display Power: state=" in power:
        state["on"] = "Display Power: state=ON" in power
    elif "mScreenOn=" in power:
        state["on"] = "mScreenOn=true" in power
    audio = _section(out, "audio")
    start = audio.find("- STREAM_MUSIC:")
    if start >= 0:
        block_lines = []
        for line in audio[start:].splitlines()[1:]:
            if line.lstrip().startswith("- "):
                break
            block_lines.append(line)
        block = "\n".join(block_lines)
        if "Muted:" in block:
            state["muted"] = block[block.find("Muted:") + 6:].lstrip().startswith("true")
        elif _number_after(block, "Mute count:") is not None:
            state["muted"] = (_number_after(block, "Mute count:") or 0) > 0
        top = _number_after(block, "Max:")
        current = _number_after(block, "streamVolume:")
        if current is None:
            devices = re.search(r"Devices:\s*([\w-]+)", block)
            line = re.search(r"Current:(.*)", block)
            if devices and line:
                current = _number_after(line.group(1), f"({devices.group(1)}):")
        if top and current is not None:
            state["volume"] = round(min(current, top) * 100 / top)
    top_app = re.search(r"(?:mResumedActivity|topResumedActivity)[^ ]* ?[:=]? ?ActivityRecord\{[^ ]+ [^ ]+ "
                        r"([A-Za-z0-9_.]+)/", _section(out, "top"))
    if top_app:
        state["app"] = top_app.group(1)
    return state


def adb_valid_package(package: str) -> bool:
    parts = package.split(".")
    return (len(package) <= 200 and len(parts) >= 2 and parts[0][:1].isalpha()
            and all(p and re.fullmatch(r"[A-Za-z0-9_]+", p) for p in parts))


def adb_apps_command(sdk: int) -> str:
    if sdk >= 24:
        return ("cmd package query-activities --brief -a android.intent.action.MAIN -c "
                "android.intent.category.LEANBACK_LAUNCHER; cmd package query-activities --brief -a "
                "android.intent.action.MAIN -c android.intent.category.LAUNCHER")
    return "pm list packages -3"


def adb_parse_apps(out: str) -> list[dict[str, Any]]:
    seen, apps = set(), []
    for line in out.splitlines():
        line = line.strip()
        if line.startswith("package:"):
            line = line[8:]
        package = line.split("/")[0].strip()
        if adb_valid_package(package) and package not in seen:
            seen.add(package)
            apps.append({"id": package, "name": _package_name(package)})
    return apps


def _package_name(package: str) -> str:
    known = {"com.google.android.youtube.tv": "YouTube", "com.google.android.youtube": "YouTube",
             "com.netflix.ninja": "Netflix", "com.netflix.mediaclient": "Netflix",
             "com.amazon.amazonvideo.livingroom": "Prime Video", "com.disney.disneyplus": "Disney+",
             "com.globo.globotv": "Globoplay", "com.spotify.tv.android": "Spotify", "com.spotify.music": "Spotify",
             "org.xbmc.kodi": "Kodi", "com.hbo.hbonow": "Max", "com.wbd.stream": "Max"}
    if package in known:
        return known[package]
    last = [p for p in package.split(".") if p not in ("com", "br", "org", "tv", "android", "app", "net")]
    return (last[-1] if last else package).capitalize()


def adb_launch_command(package: str) -> Optional[str]:
    if not adb_valid_package(package):
        return None
    return (f"monkey -p {package} -c android.intent.category.LEANBACK_LAUNCHER "
            f"-c android.intent.category.LAUNCHER 1")


# ── Home Assistant ───────────────────────────────────────────────────────────

# What can be done to each kind of entity, and the service that does it: nothing else is ever called.
ACTIONS: dict[str, dict[str, str]] = {
    "light": {"turn_on": "turn_on", "turn_off": "turn_off", "toggle": "toggle", "set_brightness": "turn_on"},
    "switch": {"turn_on": "turn_on", "turn_off": "turn_off", "toggle": "toggle"},
    "input_boolean": {"turn_on": "turn_on", "turn_off": "turn_off", "toggle": "toggle"},
    "fan": {"turn_on": "turn_on", "turn_off": "turn_off", "toggle": "toggle"},
    "media_player": {"turn_on": "turn_on", "turn_off": "turn_off", "volume_up": "volume_up",
                     "volume_down": "volume_down", "set_volume": "volume_set", "mute": "volume_mute",
                     "unmute": "volume_mute", "play_pause": "media_play_pause", "next": "media_next_track",
                     "previous": "media_previous_track"},
    "cover": {"open": "open_cover", "close": "close_cover", "stop": "stop_cover"},
    "scene": {"run": "turn_on"},
    "script": {"run": "turn_on"},
    "climate": {"turn_on": "turn_on", "turn_off": "turn_off", "set_temperature": "set_temperature"},
}
# Kinds whose actions ask the owner's Face ID first when Hermes asks (what they power is unknown).
ASKS_FIRST = {"switch", "input_boolean"}
_KIND_OF = {"light": "light", "switch": "switch", "input_boolean": "switch", "fan": "fan", "media_player": "tv",
            "cover": "cover", "scene": "scene", "script": "routine", "climate": "climate"}


def ha_base(url: str) -> str:
    """A Home Assistant's address, checked: https anywhere, http on the local network only."""
    url = (url or "").strip().rstrip("/")
    if not re.match(r"^https?://", url, re.I):
        url = "http://" + url
    parsed = urllib.parse.urlparse(url)
    if not parsed.hostname or parsed.username or parsed.password:
        raise HomeError("Not a Home Assistant address (http://homeassistant.local:8123).")
    if parsed.scheme.lower() == "http":
        resolve_local(parsed.hostname, parsed.port or 8123)
    return f"{parsed.scheme.lower()}://{parsed.netloc}"


def ha_entities(states: Any) -> list[dict[str, Any]]:
    out = []
    for s in states if isinstance(states, list) else []:
        if not isinstance(s, dict):
            continue
        entity = str(s.get("entity_id") or "")
        domain = entity.split(".")[0]
        if domain not in ACTIONS or not re.fullmatch(r"[a-z_]+\.[a-z0-9_]+", entity):
            continue
        attributes = s.get("attributes") if isinstance(s.get("attributes"), dict) else {}
        state: dict[str, Any] = {"online": s.get("state") not in ("unavailable", "unknown")}
        value = s.get("state")
        if domain in ("light", "switch", "input_boolean", "fan", "media_player", "climate"):
            state["on"] = value not in ("off", "unavailable", "unknown", "standby")
        if domain == "cover":
            state["position"] = value
        brightness = attributes.get("brightness")
        if isinstance(brightness, (int, float)):
            state["brightness"] = round(brightness * 100 / 255)
        volume = attributes.get("volume_level")
        if isinstance(volume, (int, float)):
            state["volume"] = round(volume * 100)
        if isinstance(attributes.get("is_volume_muted"), bool):
            state["muted"] = attributes["is_volume_muted"]
        if isinstance(attributes.get("current_temperature"), (int, float)):
            state["temperature"] = attributes["current_temperature"]
        out.append({
            "id": "ha:" + entity,
            "name": str(attributes.get("friendly_name") or entity)[:80],
            "room": "",
            "kind": _KIND_OF.get(domain, domain),
            "source": "Home Assistant",
            "can": sorted(ACTIONS[domain]),
            "asksFirst": domain in ASKS_FIRST,
            "state": state,
        })
    out.sort(key=lambda d: (list(ACTIONS).index(d["id"][3:].split(".")[0]), _fold(d["name"])))
    return out


def ha_service_data(domain: str, action: str, entity: str, value: Any) -> dict[str, Any]:
    data: dict[str, Any] = {"entity_id": entity}
    if action == "set_brightness":
        data["brightness_pct"] = max(1, min(100, int(_number(value, 100))))
    elif action == "set_volume":
        data["volume_level"] = max(0, min(100, int(_number(value, 20)))) / 100
    elif action in ("mute", "unmute"):
        data["is_volume_muted"] = action == "mute"
    elif action == "set_temperature":
        data["temperature"] = max(10.0, min(32.0, float(_number(value, 22))))
    return data


def _number(value: Any, default: float) -> float:
    if isinstance(value, bool):
        return default
    if isinstance(value, (int, float)):
        return float(value)
    m = re.search(r"-?\d+(?:[.,]\d+)?", str(value or ""))
    return float(m.group(0).replace(",", ".")) if m else default


# ── The home ─────────────────────────────────────────────────────────────────

# What each kind of device the iPhone drives itself can do.
_SAMSUNG_CAN = ["turn_on", "turn_off", "volume_up", "volume_down", "mute", "channel_up", "channel_down", "open_app",
                "key"]
_ANDROID_CAN = ["turn_on", "turn_off", "volume_up", "volume_down", "mute", "channel_up", "channel_down", "open_app",
                "key", "play_pause"]


class Home:
    def __init__(self, folder: str, bridge: Any = None):
        self.path = os.path.join(folder, "axyon-home.json")
        self.bridge = bridge
        self.lock = threading.RLock()
        self._secrets: dict[str, Optional[str]] = {}
        # One command at a time per device: a TV's remote takes one connection.
        self._busy: dict[str, threading.Lock] = {}

    # ── What is kept ─────────────────────────────────────────────────────────

    def load(self) -> dict[str, Any]:
        try:
            with open(self.path, encoding="utf-8") as f:
                data = json.load(f)
            if isinstance(data, dict) and isinstance(data.get("devices"), list):
                return data
        except (OSError, ValueError):
            pass
        return {"version": 1, "devices": [], "assistant": None}

    def save(self, data: dict[str, Any]) -> None:
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        os.replace(tmp, self.path)

    def secret(self, name: str) -> Optional[str]:
        """A token or key from the iPhone's Keychain, through the app (kept in memory once read)."""
        if name in self._secrets:
            return self._secrets[name]
        if self.bridge is None:
            return None
        answer = self.bridge.ask("secret", {"do": "get", "name": name}, wait=10)
        if isinstance(answer, dict) and "error" in answer:
            raise HomeError(str(answer["error"]))
        value = answer.get("value") if isinstance(answer, dict) else None
        value = value if isinstance(value, str) and value else None
        self._secrets[name] = value
        return value

    def keep_secret(self, name: str, value: Optional[str]) -> None:
        if self.bridge is None:
            raise HomeError("The AXYON app is not listening right now.")
        answer = self.bridge.ask("secret", {"do": "set", "name": name, "value": value or ""}, wait=10)
        if not (isinstance(answer, dict) and answer.get("ok")):
            raise HomeError(str((answer or {}).get("error") if isinstance(answer, dict) else "")
                            or "The Keychain did not keep it.")
        self._secrets[name] = value or None

    def forget_secret(self, name: str) -> None:
        self._secrets.pop(name, None)

    # ── The devices ──────────────────────────────────────────────────────────

    def _find(self, data: dict[str, Any], wanted: str) -> Optional[dict[str, Any]]:
        wanted = (wanted or "").strip()
        devices = data["devices"]
        for d in devices:
            if d["id"] == wanted:
                return d
        folded = _fold(wanted)
        for d in devices:
            if _fold(d.get("name", "")) == folded or _fold(f"{d.get('name', '')} {d.get('room', '')}") == folded:
                return d
        matches = [d for d in devices if folded and folded in _fold(f"{d.get('name', '')} {d.get('room', '')}")]
        return matches[0] if len(matches) == 1 else None

    def add(self, body: dict[str, Any]) -> dict[str, Any]:
        """A device the user adds from the app (or a scan found): it is asked what it is, then kept."""
        driver = str(body.get("driver") or "").strip()
        host = str(body.get("host") or "").strip().strip("[]")
        name = str(body.get("name") or "").strip()[:60]
        room = str(body.get("room") or "").strip()[:40]
        if driver not in ("samsung", "android"):
            raise HomeError("Which kind of device? A Samsung TV or an Android TV / box.")
        if driver == "samsung":
            rest = int(body.get("restPort") or SAMSUNG_REST)
            info = samsung_info(host, rest)
            if info is None:
                raise HomeError(f"No Samsung TV answered at {host}: is it on, on the same Wi-Fi?")
            device = {"id": f"samsung:{host}", "driver": "samsung", "host": host, "kind": "tv",
                      "name": name or info["name"], "room": room, "model": info["model"],
                      "secure": info["tokenAuth"], "mac": info["mac"], "frame": info["frame"], "pin": None,
                      "apps": [], "paired": False}
            # Other ports than the TVs' own: a simulated TV in the tests.
            if rest != SAMSUNG_REST:
                device["restPort"] = rest
            if body.get("remotePort"):
                device["remotePort"] = int(body["remotePort"])
        else:
            port = int(body.get("port") or ADB_PORT)
            resolve_local(host, port)
            device = {"id": f"android:{host}:{port}", "driver": "android", "host": host, "port": port, "kind": "tv",
                      "name": name or "Android TV", "room": room, "model": "", "sdk": 0, "apps": [],
                      "paired": False}
        with self.lock:
            data = self.load()
            data["devices"] = [d for d in data["devices"] if d["id"] != device["id"]]
            if len(data["devices"]) >= MAX_DEVICES:
                raise HomeError("Too many devices.")
            data["devices"].append(device)
            self.save(data)
        return self.describe(device)

    def remove(self, device_id: str) -> bool:
        with self.lock:
            data = self.load()
            kept = [d for d in data["devices"] if d["id"] != device_id]
            if len(kept) == len(data["devices"]):
                return False
            data["devices"] = kept
            self.save(data)
        if device_id.startswith("samsung:"):
            try:
                self.keep_secret("home.samsung." + device_id.split(":", 1)[1], None)
            except HomeError:
                pass
        return True

    def _update(self, device_id: str, **changes: Any) -> None:
        with self.lock:
            data = self.load()
            for d in data["devices"]:
                if d["id"] == device_id:
                    d.update(changes)
            self.save(data)

    def describe(self, d: dict[str, Any]) -> dict[str, Any]:
        out = {"id": d["id"], "name": d.get("name", ""), "room": d.get("room", ""), "kind": d.get("kind", "tv"),
               "source": {"samsung": "Samsung TV", "android": "Android"}.get(d.get("driver", ""), ""),
               "paired": bool(d.get("paired")), "asksFirst": False,
               "can": _SAMSUNG_CAN if d.get("driver") == "samsung" else _ANDROID_CAN,
               "apps": [a["name"] for a in d.get("apps", [])][:30]}
        if d.get("model"):
            out["model"] = d["model"]
        return out

    def overview(self, states: bool = True) -> dict[str, Any]:
        """Every device the iPhone drives itself, with what each says now when it answers quickly."""
        data = self.load()
        devices = [self.describe(d) for d in data["devices"]]
        if states:
            with ThreadPoolExecutor(max_workers=6) as pool:
                for device, state in zip(devices, pool.map(self._quick_state, data["devices"])):
                    if state is not None:
                        device["state"] = state
        out: dict[str, Any] = {"devices": devices}
        assistant = data.get("assistant")
        if assistant:
            out["homeAssistant"] = {"url": assistant.get("url", "")}
            try:
                entities = self._ha_states(assistant)
                devices.extend(entities)
                out["homeAssistant"]["connected"] = True
            except HomeError as e:
                out["homeAssistant"]["connected"] = False
                out["homeAssistant"]["problem"] = str(e)
        return out

    def _quick_state(self, d: dict[str, Any]) -> Optional[dict[str, Any]]:
        if d.get("driver") == "samsung":
            try:
                info = samsung_info(d["host"], d.get("restPort", SAMSUNG_REST), timeout=1.5)
            except HomeError:
                info = None
            if info is None:
                return {"online": False}
            state: dict[str, Any] = {"online": True}
            if info["power"] is not None:
                state["on"] = info["power"]
            return state
        return None  # an Android device's state takes a few seconds: asked with the action "state"

    # ── Pairing ──────────────────────────────────────────────────────────────

    def pair(self, device_id: str) -> dict[str, Any]:
        """The first contact: the TV asks on its screen whether to allow AXYON, the Android device whether to allow
        this iPhone's key. Up to a minute."""
        data = self.load()
        d = self._find(data, device_id)
        if d is None:
            raise HomeError("No such device.")
        with self._device_lock(d["id"]):
            if d["driver"] == "samsung":
                token = self.secret("home.samsung." + d["host"]) if d.get("secure") else None
                remote = SamsungRemote.open(d["host"], bool(d.get("secure")), token, d.get("pin"), pairing=True,
                                            port=d.get("remotePort"))
                try:
                    if remote.token and remote.token != token:
                        self.keep_secret("home.samsung." + d["host"], remote.token)
                    apps = remote.apps() or d.get("apps") or []
                finally:
                    remote.close()
                self._update(d["id"], pin=remote.pin, apps=apps, paired=True)
                return {"ok": True, "message": "The TV allowed AXYON.", "apps": len(apps)}
            key = self._adb_key(make=True)
            adb = Adb.connect(d["host"], int(d.get("port") or ADB_PORT), key, ask=True)
            try:
                about = adb_parse_about(adb.shell(ADB_ABOUT))
                apps = adb_parse_apps(adb.shell(adb_apps_command(about["sdk"])))
            finally:
                adb.close()
            name = d.get("name") if d.get("name") not in ("", "Android TV") else (about["name"] or about["model"])
            self._update(d["id"], paired=True, model=about["model"], sdk=about["sdk"], apps=apps,
                         name=name or "Android TV")
            return {"ok": True, "message": "The device allowed this iPhone.", "apps": len(apps)}

    def _adb_key(self, make: bool) -> Optional[AdbKey]:
        key = AdbKey.from_secret(self.secret("home.adb-key"))
        if key is None and make:
            key = AdbKey.generate()
            self.keep_secret("home.adb-key", key.to_secret())
        return key

    def _device_lock(self, device_id: str) -> threading.Lock:
        with self.lock:
            return self._busy.setdefault(device_id, threading.Lock())

    # ── Acting ───────────────────────────────────────────────────────────────

    def act(self, wanted: str, action: str, value: Any = None, confirm: Optional[Callable[[str], bool]] = None
            ) -> dict[str, Any]:
        """Does `action` to the device `wanted` (its id, or its name as the user says it). `confirm`, when given, is
        asked before a device that asks first is acted on."""
        action = (action or "").strip().lower()
        data = self.load()
        if wanted.startswith("ha:"):
            return self._ha_act(data, wanted[3:], action, value, confirm)
        d = self._find(data, wanted)
        if d is None:
            entity = self._ha_find(data, wanted)
            if entity:
                return self._ha_act(data, entity, action, value, confirm)
            raise HomeError(f"No device called {wanted} on this iPhone (home_devices lists them).")
        if action == "state":
            return {"ok": True, "device": d["name"], "state": self._state(d)}
        can = _SAMSUNG_CAN if d["driver"] == "samsung" else _ANDROID_CAN
        if action not in can:
            raise HomeError(f"{d['name']} cannot do {action} (it can: {', '.join(can)}).")
        with self._device_lock(d["id"]):
            did = self._samsung_act(d, action, value) if d["driver"] == "samsung" else self._android_act(d, action, value)
        return {"ok": True, "device": d["name"], "did": did}

    def _steps(self, value: Any) -> int:
        return max(1, min(15, int(_number(value, 1))))

    def _app(self, d: dict[str, Any], value: Any) -> dict[str, Any]:
        asked = _fold(str(value or ""))
        apps = d.get("apps") or []
        for app in apps:
            if _fold(app["name"]) == asked or app["id"] == str(value):
                return app
        close = [a for a in apps if asked and asked in _fold(a["name"])]
        if len(close) == 1:
            return close[0]
        names = ", ".join(a["name"] for a in apps[:25]) or "none known yet (pair it again)"
        raise HomeError(f"No app {value} on {d['name']}. Its apps: {names}.")

    def _samsung_act(self, d: dict[str, Any], action: str, value: Any) -> str:
        host = d["host"]
        rest = d.get("restPort", SAMSUNG_REST)
        if action in ("turn_on", "turn_off"):
            info = samsung_info(host, rest, timeout=1.5) if self._answers(host, rest) else None
            if action == "turn_on" and info is None:
                woke = wake_on_lan(d.get("mac") or "", resolve_local(host, rest)) if d.get("mac") else False
                if woke:
                    return "Sent the TV the wake-up signal: it turns on if its network standby is on."
                raise HomeError("The TV is off and does not answer on the network: turn it on with its remote once "
                                "(or turn on Settings → General → Network → Expert → Power On with Mobile).")
            # A TV that answers is on, unless it says standby (the models since 2018 say which).
            if action == "turn_on" and info["power"] is not False:
                return "It was already on."
            if action == "turn_off" and (info is None or info["power"] is False):
                return "It was already off."
        token = self.secret("home.samsung." + host) if d.get("secure") else None
        if d.get("secure") and not token:
            raise HomeError(f"{d['name']} has not allowed AXYON yet: pair it from the Home tab.")
        remote = SamsungRemote.open(host, bool(d.get("secure")), token, d.get("pin"), pairing=False,
                                    port=d.get("remotePort"))
        try:
            if remote.token and remote.token != token:
                self.keep_secret("home.samsung." + host, remote.token)
            if action in ("turn_on", "turn_off"):
                if d.get("frame") and action == "turn_off":
                    remote.key("KEY_POWER", "Press")
                    time.sleep(3.0)
                    remote.key("KEY_POWER", "Release")
                else:
                    remote.key("KEY_POWER")
                return "Turned on." if action == "turn_on" else "Turned off."
            if action in ("volume_up", "volume_down", "channel_up", "channel_down"):
                key = {"volume_up": "KEY_VOLUP", "volume_down": "KEY_VOLDOWN", "channel_up": "KEY_CHUP",
                       "channel_down": "KEY_CHDOWN"}[action]
                steps = self._steps(value)
                for _ in range(steps):
                    remote.key(key)
                    time.sleep(0.12)
                return f"{action.replace('_', ' ').capitalize()} ×{steps}."
            if action == "mute":
                remote.key("KEY_MUTE")
                return "Mute pressed (it toggles)."
            if action == "key":
                name = _fold(str(value or "")).replace(" ", "_")
                if name not in _SAMSUNG_KEYS:
                    raise HomeError(f"No key {value} (keys: {', '.join(_SAMSUNG_KEYS)}).")
                remote.key(_SAMSUNG_KEYS[name])
                return f"Pressed {name}."
            if action == "open_app":
                apps = d.get("apps") or []
                if not apps:
                    apps = remote.apps() or []
                    if apps:
                        self._update(d["id"], apps=apps)
                        d["apps"] = apps
                app = self._app(d, value)
                if not remote.launch(app):
                    raise HomeError(f"The TV did not open {app['name']}.")
                return f"Opened {app['name']}."
        finally:
            remote.close()
        raise HomeError("Not done.")

    def _answers(self, host: str, port: int) -> bool:
        try:
            with socket.create_connection((resolve_local(host, port), port), timeout=1.2):
                return True
        except (OSError, HomeError):
            return False

    def _android(self, d: dict[str, Any]) -> Adb:
        key = self._adb_key(make=False)
        if key is None or not d.get("paired"):
            raise HomeError(f"{d['name']} has not allowed this iPhone yet: pair it from the Home tab.")
        return Adb.connect(d["host"], int(d.get("port") or ADB_PORT), key, ask=False)

    def _android_act(self, d: dict[str, Any], action: str, value: Any) -> str:
        adb = self._android(d)
        try:
            sdk = int(d.get("sdk") or 0)
            if action in ("turn_on", "turn_off"):
                state = adb_parse_state(adb.shell(adb_state_command(sdk)))
                if state.get("on") is (action == "turn_on"):
                    return "It was already on." if action == "turn_on" else "It was already off."
                if sdk >= 20:
                    code = KEYCODES["wakeup" if action == "turn_on" else "sleep"]
                else:
                    code = KEYCODES["power"]
                adb.shell(f"input keyevent {code}")
                return "Turned on." if action == "turn_on" else "Turned off."
            if action in ("volume_up", "volume_down", "channel_up", "channel_down"):
                steps = self._steps(value)
                adb.shell("input keyevent " + " ".join([str(KEYCODES[action])] * steps))
                return f"{action.replace('_', ' ').capitalize()} ×{steps}."
            if action in ("mute", "play_pause"):
                adb.shell(f"input keyevent {KEYCODES[action]}")
                return "Done."
            if action == "key":
                name = _fold(str(value or "")).replace(" ", "_")
                if name not in KEYCODES or name in ("power", "sleep", "wakeup"):
                    raise HomeError(f"No key {value}.")
                adb.shell(f"input keyevent {KEYCODES[name]}")
                return f"Pressed {name}."
            if action == "open_app":
                if not d.get("apps"):
                    apps = adb_parse_apps(adb.shell(adb_apps_command(sdk)))
                    self._update(d["id"], apps=apps)
                    d["apps"] = apps
                app = self._app(d, value)
                command = adb_launch_command(app["id"])
                if command is None:
                    raise HomeError("Not an app of the device.")
                out = adb.shell(command).lower()
                if "error" in out or "no activities found" in out or "aborted" in out:
                    raise HomeError(f"{d['name']} did not open {app['name']}.")
                return f"Opened {app['name']}."
        finally:
            adb.close()
        raise HomeError("Not done.")

    def _state(self, d: dict[str, Any]) -> dict[str, Any]:
        if d["driver"] == "samsung":
            return self._quick_state(d) or {}
        with self._device_lock(d["id"]):
            adb = self._android(d)
            try:
                return adb_parse_state(adb.shell(adb_state_command(int(d.get("sdk") or 0))))
            finally:
                adb.close()

    # ── Home Assistant ───────────────────────────────────────────────────────

    def set_assistant(self, url: str) -> dict[str, Any]:
        """The user's Home Assistant: its address here, its token already in the Keychain (the app put it there)."""
        base = ha_base(url)
        self.forget_secret("home.ha.token")
        entities = self._ha_states({"url": base})
        with self.lock:
            data = self.load()
            data["assistant"] = {"url": base}
            self.save(data)
        return {"ok": True, "url": base, "entities": len(entities)}

    def remove_assistant(self) -> None:
        with self.lock:
            data = self.load()
            data["assistant"] = None
            self.save(data)
        try:
            self.keep_secret("home.ha.token", None)
        except HomeError:
            pass

    def _ha_call(self, assistant: dict[str, Any], method: str, path: str, body: Any = None) -> Any:
        token = self.secret("home.ha.token")
        if not token:
            raise HomeError("Home Assistant's token is missing: add it again in the Home tab.")
        status, raw = http(method, assistant["url"] + path, body=body, timeout=8.0,
                           headers={"Authorization": f"Bearer {token}"})
        if status == 401:
            raise HomeError("Home Assistant refused the token: make a new long-lived token (your profile → "
                            "Security) and paste it again.")
        if status >= 400:
            raise HomeError(f"Home Assistant answered {status}.")
        return _json(raw)

    def _ha_states(self, assistant: dict[str, Any]) -> list[dict[str, Any]]:
        return ha_entities(self._ha_call(assistant, "GET", "/api/states"))

    def _ha_find(self, data: dict[str, Any], wanted: str) -> Optional[str]:
        assistant = data.get("assistant")
        if not assistant:
            return None
        try:
            entities = self._ha_states(assistant)
        except HomeError:
            return None
        folded = _fold(wanted)
        exact = [e for e in entities if _fold(e["name"]) == folded or e["id"][3:] == wanted]
        if exact:
            return exact[0]["id"][3:]
        close = [e for e in entities if folded and folded in _fold(e["name"])]
        return close[0]["id"][3:] if len(close) == 1 else None

    def _ha_act(self, data: dict[str, Any], entity: str, action: str, value: Any,
                confirm: Optional[Callable[[str], bool]]) -> dict[str, Any]:
        assistant = data.get("assistant")
        if not assistant:
            raise HomeError("No Home Assistant on this iPhone yet (Home tab → Add → Home Assistant).")
        domain = entity.split(".")[0]
        if domain not in ACTIONS or not re.fullmatch(r"[a-z_]+\.[a-z0-9_]+", entity):
            raise HomeError(f"AXYON does not act on {entity}.")
        if action not in ACTIONS[domain]:
            raise HomeError(f"{entity} cannot do {action} (it can: {', '.join(sorted(ACTIONS[domain]))}).")
        if domain in ASKS_FIRST and confirm is not None and not confirm(f"{action.replace('_', ' ')}: {entity}"):
            return {"ok": False, "reason": "declined", "message": "The owner did not confirm on the iPhone."}
        service = ACTIONS[domain][action]
        self._ha_call(assistant, "POST", f"/api/services/{domain}/{service}",
                      ha_service_data(domain, action, entity, value))
        return {"ok": True, "device": entity, "did": f"{domain}.{service}"}

    # ── Looking for devices ──────────────────────────────────────────────────

    def scan(self, address: str, prefix: int = 24) -> dict[str, Any]:
        """The Samsung TVs, ADB devices and Home Assistants on the iPhone's Wi-Fi (`address`/`prefix`, which the app
        gives: the Wi-Fi's own address, so a cellular one is never scanned)."""
        if not is_local(address):
            return {"error": "Connect the iPhone to your home's Wi-Fi to look for devices.", "found": []}
        network = ipaddress.ip_network(f"{address}/{max(22, min(30, int(prefix)))}", strict=False)
        hosts = [str(h) for h in network.hosts() if str(h) != address][:1022]
        ports = (SAMSUNG_REST, ADB_PORT, 8123)
        refused_by_ios = [0]

        def probe(target: tuple[str, int]) -> Optional[tuple[str, int]]:
            try:
                with socket.create_connection(target, timeout=0.7):
                    return target
            except OSError as e:
                if getattr(e, "errno", None) == 65:  # EHOSTUNREACH on iOS: local network not allowed (yet)
                    refused_by_ios[0] += 1
                return None

        with ThreadPoolExecutor(max_workers=64) as pool:
            open_ports = [t for t in pool.map(probe, [(h, p) for h in hosts for p in ports]) if t]
        known = {d["id"] for d in self.load()["devices"]}
        found = []
        for host, port in sorted(open_ports, key=lambda t: (ipaddress.ip_address(t[0]), t[1])):
            if port == SAMSUNG_REST:
                try:
                    info = samsung_info(host, timeout=2.0)
                except HomeError:
                    info = None
                if info:
                    found.append({"driver": "samsung", "host": host, "name": info["name"], "model": info["model"],
                                  "added": f"samsung:{host}" in known})
            elif port == ADB_PORT:
                found.append({"driver": "android", "host": host, "port": port, "name": "Android (ADB)",
                              "added": f"android:{host}:{port}" in known})
            elif port == 8123:
                found.append({"driver": "homeassistant", "host": host, "url": f"http://{host}:8123",
                              "name": "Home Assistant"})
        out: dict[str, Any] = {"network": str(network), "found": found}
        if not open_ports and refused_by_ios[0] > len(hosts):
            out["error"] = ("AXYON may not look on your local network: iPhone Settings → Privacy & Security → "
                            "Local Network → AXYON.")
        return out

    # ── Hermes' tools ────────────────────────────────────────────────────────

    def tool_devices(self, args: dict[str, Any], **_kw) -> str:
        try:
            out = self.overview(states=True)
        except HomeError as e:
            return json.dumps({"error": str(e)}, ensure_ascii=False)
        if not out["devices"]:
            out["note"] = ("No device on this iPhone yet: the user adds them in AXYON's Home tab (Samsung TV, Android "
                           "TV, Home Assistant). A paired computer's home is in coucou_home.")
        return json.dumps(out, ensure_ascii=False)

    def tool_control(self, args: dict[str, Any], **_kw) -> str:
        def confirm(what: str) -> bool:
            if self.bridge is None:
                return False
            answer = self.bridge.ask("iphone", {"action": "confirm", "text": what}, wait=60)
            return isinstance(answer, dict) and answer.get("ok") is True

        try:
            result = self.act(str(args.get("device") or ""), str(args.get("action") or ""), args.get("value"),
                              confirm=confirm)
        except HomeError as e:
            result = {"ok": False, "error": str(e)}
        except Exception as e:  # a device that answers oddly: said, never a crash of the turn
            result = {"ok": False, "error": f"The device answered something unexpected ({e})."}
        return json.dumps(result, ensure_ascii=False)

    def register(self, registry) -> None:
        registry.register(
            name="home_devices", toolset=TOOLSET, handler=self.tool_devices, emoji="🏠",
            schema={"name": "home_devices",
                    "description": "The user's devices this iPhone drives itself on the home's Wi-Fi, with no computer: "
                                   "Samsung TVs, Android TVs and boxes, and the entities of their Home Assistant "
                                   "(lights, switches, media players, covers, scenes, scripts), each with its id, "
                                   "room, what it can do (`can`), its apps and its state when it answers.",
                    "parameters": {"type": "object", "properties": {}}})
        registry.register(
            name="home_control", toolset=TOOLSET, handler=self.tool_control, emoji="🏠",
            schema={"name": "home_control",
                    "description": "Acts on one of the devices of home_devices, from this iPhone: turn_on, turn_off, "
                                   "volume_up / volume_down / channel_up / channel_down (value: how many steps), mute, "
                                   "open_app (value: the app's name), key (value: home, back, ok, up, down, left, "
                                   "right, play_pause, menu, source), state; for Home Assistant also toggle, "
                                   "set_brightness (0-100), set_volume (0-100), unmute, play_pause, next, previous, "
                                   "open, close, stop, run (a scene or script), set_temperature. A switch asks the "
                                   "owner's Face ID first.",
                    "parameters": {"type": "object", "properties": {
                        "device": {"type": "string", "description": "The device's id from home_devices, or its name"},
                        "action": {"type": "string"},
                        "value": {"description": "Steps, a percent, an app's name or a key, when the action takes one"}},
                        "required": ["device", "action"]}})
