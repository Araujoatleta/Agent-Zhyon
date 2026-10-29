"""The iPhone's own home (axyon_hermes/home.py) against simulated devices on this machine's network address: a
Samsung TV (its REST API and its remote over TLS, which asks once then knows AXYON by its token), an Android TV over
ADB (which asks once for this iPhone's key, then takes the commands of the fixed list) and a Home Assistant (a token,
and only the services AXYON allows). The Keychain and Face ID are the app's: a stand-in bridge here.
`python3 test_home.py` (needs websockets, and openssl for the TV's certificate)."""
import base64
import hashlib
import json
import logging
import os
import secrets
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "app"))

from axyon_hermes import home  # noqa: E402

# The simulated TV's server says when a client leaves early: not this test's business.
logging.getLogger("websockets").setLevel(logging.CRITICAL)

failures = 0


def check(label, ok):
    global failures
    print(("  ✓ " if ok else "  ✗ ") + label)
    if not ok:
        failures += 1


def lan_address():
    """This machine's own address on its network (a device's address must be local, never 127.0.0.1)."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("192.0.2.1", 9))
        return s.getsockname()[0]
    finally:
        s.close()


class Bridge:
    """The app's side: the Keychain (`secret`) and Face ID (`iphone` confirm)."""

    def __init__(self):
        self.keychain = {}
        self.allow = True
        self.asked = []

    def ask(self, kind, request, wait=45):
        self.asked.append((kind, request))
        if kind == "secret":
            name = request["name"]
            if not name.startswith("home."):
                return {"error": "not a home secret"}
            if request["do"] == "get":
                return {"value": self.keychain.get(name, "")}
            if request["value"]:
                self.keychain[name] = request["value"]
            else:
                self.keychain.pop(name, None)
            return {"ok": True}
        if kind == "iphone" and request.get("action") == "confirm":
            return {"ok": self.allow}
        return {"error": "unknown"}


# ── Pure pieces ──────────────────────────────────────────────────────────────

print("Addresses")
check("a home network's address", home.is_local("192.168.0.12") and home.is_local("10.0.0.5"))
check("the tailnet's", home.is_local("100.101.102.103"))
check("never this iPhone itself, nor the internet", not home.is_local("127.0.0.1") and not home.is_local("8.8.8.8"))
check("names as the user says them", home._fold("  TV  da Sála ") == home._fold("tv da sala"))

print("Samsung")
INFO = {"device": {"type": "Samsung SmartTV", "name": "[TV] Samsung 7 Series (43)", "modelName": "UN43AU7700",
                   "OS": "Tizen", "PowerState": "on", "TokenAuthSupport": "true", "wifiMac": "a0:b1:c2:d3:e4:f5",
                   "FrameTVSupport": "false"}, "version": "2.0.25"}
info = home.parse_samsung(INFO)
check("what the TV says it is", info["name"] == "Samsung 7 Series (43)" and info["tokenAuth"] and info["power"] is True)
check("another device's answer is none", home.parse_samsung({"device": {"type": "Speaker"}}) is None)
check("the remote's address names AXYON and carries only a clean token",
      home.samsung_remote_url("192.168.0.9", 8002, True, "12345678").endswith("?name=QVhZT04=&token=12345678")
      and "token" not in home.samsung_remote_url("192.168.0.9", 8002, True, "12&x=1"))
apps = home.parse_samsung_apps({"data": {"data": [{"appId": "111299001912", "name": "YouTube", "app_type": 2},
                                                  {"appId": "bad id!", "name": "X"}]}})
check("the TV's apps, the malformed left out", apps == [{"id": "111299001912", "name": "YouTube", "kind": 2}])

print("ADB")
t0 = time.time()
key = home.AdbKey.generate()
check(f"a 2048-bit key made here ({time.time() - t0:.1f}s)", key.n.bit_length() == 2048)
token = os.urandom(20)
signature = key.sign(token)
block = pow(int.from_bytes(signature, "big"), 65537, key.n).to_bytes(256, "big")
check("the token signed as adbd checks it", block.startswith(b"\x00\x01\xff") and block.endswith(home._SHA1_INFO + token))
public = key.android_public()
words = struct.unpack("<I", public[:4])[0]
n = int.from_bytes(public[8:8 + 4 * words], "little")
n0inv = struct.unpack("<I", public[4:8])[0]
check("the public key as Android keeps it", words == 64 and n == key.n and (n0inv * key.n) % (1 << 32) == (1 << 32) - 1
      and len(public) == 8 + 8 * words + 4)
check("kept and read back", home.AdbKey.from_secret(key.to_secret()).n == key.n)
check("only package names are opened", home.adb_launch_command("com.netflix.ninja").startswith("monkey -p com.netflix.ninja ")
      and home.adb_launch_command("x; reboot") is None and home.adb_launch_command("com.x/../y") is None)
KITKAT = """@@power
POWER MANAGER (dumpsys power)
  mWakefulness=Asleep
@@audio
- STREAM_MUSIC:
   Mute count: 0
   Current: 2: 5, 400: 7,
   Max: 15
@@top
    mResumedActivity: ActivityRecord{42b1c0f0 u0 com.android.launcher/com.android.launcher2.Launcher t1}
"""
state = home.adb_parse_state(KITKAT)
check("Android 4.4: asleep, not muted, the launcher on screen",
      state == {"on": False, "muted": False, "app": "com.android.launcher"})
TEN = """@@power
  mWakefulness=Awake
@@audio
- STREAM_MUSIC:
   Muted: false
   Max: 15
   streamVolume:6
@@top
  topResumedActivity=ActivityRecord{1234567 u0 com.google.android.youtube.tv/com.google.Shell t12}
"""
check("Android 10: awake, its volume and app", home.adb_parse_state(TEN) == {
    "on": True, "muted": False, "volume": 40, "app": "com.google.android.youtube.tv"})
check("the apps of `pm list packages`", [a["name"] for a in home.adb_parse_apps(
    "package:com.netflix.ninja\npackage:org.xbmc.kodi\npackage:bad name\n")] == ["Netflix", "Kodi"])

print("Home Assistant")
STATES = [
    {"entity_id": "light.sala", "state": "on", "attributes": {"friendly_name": "Luz da sala", "brightness": 128}},
    {"entity_id": "switch.tomada", "state": "off", "attributes": {"friendly_name": "Tomada do quarto"}},
    {"entity_id": "lock.porta", "state": "locked", "attributes": {"friendly_name": "Porta"}},
    {"entity_id": "media_player.tv", "state": "playing", "attributes": {"friendly_name": "TV", "volume_level": 0.3}},
    {"entity_id": "sensor.temp", "state": "21", "attributes": {}},
]
entities = home.ha_entities(STATES)
check("lights, switches and players; never a lock or a sensor",
      [e["id"] for e in entities] == ["ha:light.sala", "ha:switch.tomada", "ha:media_player.tv"])
check("their state", entities[0]["state"] == {"online": True, "on": True, "brightness": 50}
      and entities[2]["state"]["volume"] == 30)
check("a switch asks first", entities[1]["asksFirst"] and not entities[0]["asksFirst"])
check("a brightness in percent", home.ha_service_data("light", "set_brightness", "light.sala", "40%")
      == {"entity_id": "light.sala", "brightness_pct": 40})


# ── Simulated devices ────────────────────────────────────────────────────────

HOST = lan_address()
if not home.is_local(HOST):
    print(f"This machine's address {HOST} is not a local one: the device tests need one.")
    sys.exit(1)
work = tempfile.mkdtemp(prefix="axyon-home-")


class Tv:
    """A Samsung TV: REST on one port, its remote over TLS on another."""

    def __init__(self):
        self.power = "on"
        self.received = []
        self.uris = []
        self.allowed_token = "58034573"
        cert, keyfile = os.path.join(work, "tv.pem"), os.path.join(work, "tv.key")
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", keyfile, "-out", cert,
                        "-days", "2", "-subj", "/CN=SmartViewSDK"], check=True, capture_output=True)
        self.cert_der = ssl.PEM_cert_to_DER_cert(open(cert).read())
        tv = self

        class Rest(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def do_GET(self):
                body = dict(INFO)
                body["device"] = dict(INFO["device"], PowerState=tv.power)
                raw = json.dumps(body).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

        self.rest = ThreadingHTTPServer((HOST, 0), Rest)
        threading.Thread(target=self.rest.serve_forever, daemon=True).start()
        from websockets.sync.server import serve

        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(cert, keyfile)

        def remote(ws):
            uri = ws.request.path
            tv.uris.append(uri)
            try:
                if "token=" + tv.allowed_token in uri:
                    ws.send(json.dumps({"event": "ms.channel.connect", "data": {"id": "x"}}))
                else:
                    ws.send(json.dumps({"event": "ed.edenTV.update"}))
                    ws.send(json.dumps({"event": "ms.channel.connect", "data": {"id": "x", "token": tv.allowed_token}}))
                for raw in ws:
                    v = json.loads(raw)
                    tv.received.append(v)
                    event = v.get("params", {}).get("event")
                    if event == "ed.installedApp.get":
                        ws.send(json.dumps({"event": "ed.installedApp.get", "data": {"data": [
                            {"appId": "111299001912", "app_type": 2, "name": "YouTube"},
                            {"appId": "3201907018807", "app_type": 2, "name": "Netflix"}]}}))
                    elif event == "ed.apps.launch":
                        ws.send(json.dumps({"event": "ed.apps.launch", "data": 200}))
            except Exception:
                pass

        self.ws = serve(remote, HOST, 0, ssl=context, compression=None)
        threading.Thread(target=self.ws.serve_forever, daemon=True).start()
        self.rest_port = self.rest.server_address[1]
        self.ws_port = self.ws.socket.getsockname()[1]

    def keys(self):
        return [m["params"]["DataOfCmd"] for m in self.received if m.get("method") == "ms.remote.control"]


class AndroidTv:
    """An Android 4.4 box over ADB: it asks once for this iPhone's key, then runs the commands it is sent."""

    def __init__(self):
        self.allowed = []
        self.commands = []
        self.awake = False
        self.server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.server.bind((HOST, 0))
        self.server.listen(4)
        self.port = self.server.getsockname()[1]
        threading.Thread(target=self._serve, daemon=True).start()

    def _serve(self):
        while True:
            conn, _ = self.server.accept()
            threading.Thread(target=self._client, args=(conn,), daemon=True).start()

    def _verify(self, token, signature):
        for n in self.allowed:
            size = (n.bit_length() + 7) // 8
            block = pow(int.from_bytes(signature, "big"), 65537, n).to_bytes(size, "big")
            if block.endswith(home._SHA1_INFO + token) and block.startswith(b"\x00\x01\xff"):
                return True
        return False

    def _answer(self, command):
        self.commands.append(command)
        if command.startswith("getprop"):
            return "FakeBox\n4.4.2\n19\nTV do quarto\n"
        if command.startswith("echo @@power"):
            return KITKAT.replace("Asleep", "Awake" if self.awake else "Asleep")
        if command == "pm list packages -3":
            return "package:com.netflix.ninja\npackage:org.xbmc.kodi\n"
        if command.startswith("input keyevent"):
            if "26" in command.split():
                self.awake = not self.awake
            return ""
        if command.startswith("monkey"):
            return "Events injected: 1\n"
        return ""

    def _client(self, conn):
        send = lambda *m: conn.sendall(home.adb_message(*m))  # noqa: E731
        token = os.urandom(20)
        try:
            while True:
                cmd, arg0, arg1, data = home.adb_read(conn)
                if cmd == home._CNXN:
                    send(home._AUTH, home._TOKEN, 0, token)
                elif cmd == home._AUTH and arg0 == home._SIGNATURE:
                    if self._verify(token, data):
                        send(home._CNXN, 0x01000000, 4096, b"device::ro.product.model=FakeBox;")
                    else:
                        token = os.urandom(20)
                        send(home._AUTH, home._TOKEN, 0, token)
                elif cmd == home._AUTH and arg0 == home._PUBLIC_KEY:
                    raw = base64.b64decode(data.split(b" ")[0])
                    words = struct.unpack("<I", raw[:4])[0]
                    self.allowed.append(int.from_bytes(raw[8:8 + 4 * words], "little"))  # the user said yes
                    send(home._CNXN, 0x01000000, 4096, b"device::ro.product.model=FakeBox;")
                elif cmd == home._OPEN:
                    command = data.rstrip(b"\x00").decode()[len("shell:"):]
                    out = self._answer(command).encode()
                    remote = 100 + arg0
                    send(home._OKAY, remote, arg0)
                    if out:
                        send(home._WRTE, remote, arg0, out)
                        home.adb_read(conn)  # its OKAY
                    send(home._CLSE, remote, arg0)
                    home.adb_read(conn)  # its CLSE
        except (ConnectionError, OSError):
            conn.close()


class Assistant:
    def __init__(self):
        self.calls = []
        ha = self

        class Api(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _send(self, code, obj):
                raw = json.dumps(obj).encode()
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            def _ok(self):
                return self.headers.get("Authorization") == "Bearer good-token"

            def do_GET(self):
                if not self._ok():
                    return self._send(401, {"message": "401: Unauthorized"})
                if self.path == "/api/states":
                    return self._send(200, STATES)
                return self._send(404, {})

            def do_POST(self):
                if not self._ok():
                    return self._send(401, {"message": "401: Unauthorized"})
                body = json.loads(self.rfile.read(int(self.headers.get("Content-Length") or 0)) or b"{}")
                ha.calls.append((self.path, body))
                return self._send(200, [])

        self.server = ThreadingHTTPServer((HOST, 0), Api)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = f"http://{HOST}:{self.server.server_address[1]}"


bridge = Bridge()
h = home.Home(work, bridge)

print("A Samsung TV from the iPhone")
tv = Tv()
added = h.add({"driver": "samsung", "host": HOST, "room": "Sala", "restPort": tv.rest_port, "remotePort": tv.ws_port})
check("added: named by the TV, in its room", added["name"] == "Samsung 7 Series (43)" and added["room"] == "Sala")
paired = h.pair(added["id"])
check("paired: the TV gave its token, kept in the Keychain", paired["ok"]
      and bridge.keychain.get(f"home.samsung.{HOST}") == "58034573")
kept = h.load()["devices"][0]
check("its certificate pinned and its apps kept", kept["pin"] == hashlib.sha256(tv.cert_der).hexdigest()
      and [a["name"] for a in kept["apps"]] == ["Netflix", "YouTube"])
done = h.act("Samsung 7 Series (43)", "volume_up", 2)
check("two steps of volume, by its name", done["ok"] and tv.keys() == ["KEY_VOLUP", "KEY_VOLUP"])
check("the second time it knows AXYON by its token", tv.uris[-1].endswith("&token=58034573"))
h.act(added["id"], "open_app", "youtube")
launch = [m for m in tv.received if m.get("params", {}).get("event") == "ed.apps.launch"]
check("an app of its own list, by name", launch and launch[-1]["params"]["data"]["appId"] == "111299001912")
check("on already: turning it on presses nothing", "already on" in h.act(added["id"], "turn_on")["did"]
      and "KEY_POWER" not in tv.keys())
h.act(added["id"], "turn_off")
check("turned off with its power key", tv.keys()[-1] == "KEY_POWER")
tv.power = "standby"
check("in standby: off already", "already off" in h.act(added["id"], "turn_off")["did"])
tv.power = "on"
try:
    h.act(added["id"], "reboot")
    check("an action it does not have is refused", False)
except home.HomeError as e:
    check("an action it does not have is refused", "cannot do reboot" in str(e))
data = h.load()
data["devices"][0]["pin"] = "0" * 64
h.save(data)
try:
    h.act(added["id"], "mute")
    check("a changed certificate is told", False)
except home.HomeError as e:
    check("a changed certificate is told", "certificate changed" in str(e))

print("An Android box over ADB from the iPhone")
box = AndroidTv()
added = h.add({"driver": "android", "host": HOST, "port": box.port})
try:
    h.act(added["id"], "volume_up")
    check("not paired yet: it says so", False)
except home.HomeError as e:
    check("not paired yet: it says so", "pair it" in str(e))
t0 = time.time()
paired = h.pair(added["id"])
check(f"paired: the key made, offered, allowed ({time.time() - t0:.1f}s)", paired["ok"] and len(box.allowed) == 1
      and "home.adb-key" in bridge.keychain)
kept = [d for d in h.load()["devices"] if d["driver"] == "android"][0]
check("named by the device, its Android and its apps known", kept["name"] == "TV do quarto" and kept["sdk"] == 19
      and [a["name"] for a in kept["apps"]] == ["Netflix", "Kodi"])
h.act("tv do quarto", "volume_up", 3)
check("three steps of volume in one command", box.commands[-1] == "input keyevent 24 24 24")
h.act(added["id"], "turn_on")
check("Android 4.4, asleep: its power key wakes it", box.commands[-1] == "input keyevent 26" and box.awake)
check("awake: turning it on presses nothing", "already on" in h.act(added["id"], "turn_on")["did"])
h.act(added["id"], "open_app", "netflix")
check("an app of its own list", box.commands[-1].startswith("monkey -p com.netflix.ninja "))
check("its state", h.act(added["id"], "state")["state"]["on"] is True)
bridge2 = Bridge()
bridge2.keychain = dict(bridge.keychain)
again = home.Home(work, bridge2)
again.act(added["id"], "volume_down")
check("the key read back from the Keychain: the box knows this iPhone", box.commands[-1] == "input keyevent 25"
      and len(box.allowed) == 1)

print("Home Assistant from the iPhone")
ha = Assistant()
bridge.keychain["home.ha.token"] = "bad-token"
try:
    h.set_assistant(ha.url)
    check("a wrong token is told", False)
except home.HomeError as e:
    check("a wrong token is told", "refused the token" in str(e))
bridge.keychain["home.ha.token"] = "good-token"
check("connected, its entities counted", h.set_assistant(ha.url)["entities"] == 3)
done = h.act("Luz da sala", "set_brightness", 40)
check("a light by its name, at 40 %", done["ok"] and ha.calls[-1] == ("/api/services/light/turn_on",
                                                                    {"entity_id": "light.sala", "brightness_pct": 40}))
bridge.allow = False
calls = len(ha.calls)
out = json.loads(h.tool_control({"device": "Tomada do quarto", "action": "turn_on"}))
check("a switch asks Face ID first; refused, nothing done", out.get("reason") == "declined" and len(ha.calls) == calls)
bridge.allow = True
out = json.loads(h.tool_control({"device": "ha:switch.tomada", "action": "turn_on"}))
check("allowed, done", out["ok"] and ha.calls[-1][0] == "/api/services/switch/turn_on")
out = json.loads(h.tool_control({"device": "ha:lock.porta", "action": "turn_on"}))
check("never a lock", not out["ok"] and "does not act" in out["error"])

print("Hermes' tools")
listed = json.loads(h.tool_devices({}))
names = [d["name"] for d in listed["devices"]]
check("every device, the iPhone's own and Home Assistant's", "TV do quarto" in names and "Luz da sala" in names
      and "Samsung 7 Series (43)" in names)
check("the TV's state read", [d for d in listed["devices"] if d["source"] == "Samsung TV"][0]["state"]["on"] is True)
out = json.loads(h.tool_control({"device": "geladeira", "action": "turn_on"}))
check("a device it does not know: said, as JSON", out["ok"] is False and "No device called geladeira" in out["error"])
check("removed, its token forgotten", h.remove(f"samsung:{HOST}") and f"home.samsung.{HOST}" not in bridge.keychain)

shutil.rmtree(work, ignore_errors=True)
print("All home tests passed." if failures == 0 else f"{failures} failed.")
sys.exit(1 if failures else 0)
