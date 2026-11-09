"""More of the home, straight from the iPhone over the home's Wi-Fi, each with its own local API and no cloud:

  * Philips Hue: the bridge's REST API (v1). The first time, the user presses the round button on the bridge and AXYON
    gets a key (kept in the iPhone's Keychain). Then its lights, its rooms and zones, and their scenes.
  * Shelly plugs and relays (Gen1 `/relay/0`, Gen2+ RPC `Switch.Set`), and their dimmers and bulbs.
  * Tasmota (Sonoff and the many plugs flashed with it): `/cm?cmnd=Power On`.
  * WLED LED strips: `/json/state`.
  * Roku TVs and sticks (TCL, AOC, Philco… Roku TV): the External Control Protocol on port 8060.
  * Webhooks: an address the user gives (an Alexa routine through a URL, IFTTT, a Home Assistant webhook, a
    service's own), called with GET or POST. The address, which may carry a key, is kept in the Keychain.

Only what is listed here is ever sent: no command typed by anyone, the model included. home.py keeps the devices and
asks these functions; each raises HomeError with words the user can act on."""

from __future__ import annotations

import json
import re
import urllib.parse
from typing import Any, Optional

from .home import HomeError, _json, http, is_local, resolve_local

ROKU_PORT = 8060
HUE_APP = "AXYON#iphone"

# ── Colours ──────────────────────────────────────────────────────────────────

# The colours a person says, in Portuguese and English, as RGB.
COLOURS = {
    "vermelho": (255, 0, 0), "red": (255, 0, 0),
    "verde": (0, 200, 0), "green": (0, 200, 0),
    "azul": (0, 60, 255), "blue": (0, 60, 255),
    "amarelo": (255, 200, 0), "yellow": (255, 200, 0),
    "laranja": (255, 110, 0), "orange": (255, 110, 0),
    "roxo": (140, 0, 255), "purple": (140, 0, 255), "violeta": (140, 0, 255),
    "rosa": (255, 60, 150), "pink": (255, 60, 150),
    "ciano": (0, 220, 255), "cyan": (0, 220, 255),
    "magenta": (255, 0, 200),
    "branco": (255, 255, 255), "white": (255, 255, 255),
    "quente": (255, 170, 80), "warm": (255, 170, 80), "branco quente": (255, 170, 80),
    "frio": (200, 225, 255), "fria": (200, 225, 255), "cool": (200, 225, 255), "branco frio": (200, 225, 255),
}


def parse_colour(value: Any) -> tuple[int, int, int]:
    """A colour's name, "#RRGGBB" or "r,g,b"."""
    text = str(value or "").strip().lower()
    if text in COLOURS:
        return COLOURS[text]
    m = re.fullmatch(r"#?([0-9a-f]{6})", text)
    if m:
        h = m.group(1)
        return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    m = re.fullmatch(r"(\d{1,3})\s*[,; ]\s*(\d{1,3})\s*[,; ]\s*(\d{1,3})", text)
    if m:
        r, g, b = (min(255, int(x)) for x in m.groups())
        return r, g, b
    names = ", ".join(sorted({k for k in COLOURS if " " not in k}))
    raise HomeError(f"Which colour? A name ({names}) or #RRGGBB.")


def rgb_to_xy(r: int, g: int, b: int) -> tuple[float, float]:
    """sRGB to the CIE xy the Hue lights take (Philips' own conversion, wide gamut D65)."""
    def linear(c: float) -> float:
        c /= 255.0
        return ((c + 0.055) / 1.055) ** 2.4 if c > 0.04045 else c / 12.92

    rl, gl, bl = linear(r), linear(g), linear(b)
    x = rl * 0.664511 + gl * 0.154324 + bl * 0.162028
    y = rl * 0.283881 + gl * 0.668433 + bl * 0.047685
    z = rl * 0.000088 + gl * 0.072310 + bl * 0.986039
    total = x + y + z
    if total <= 0:
        return 0.3227, 0.329  # white
    return round(x / total, 4), round(y / total, 4)


def percent(value: Any, default: int = 100) -> int:
    if isinstance(value, bool):
        return default
    if isinstance(value, (int, float)):
        return max(0, min(100, int(value)))
    m = re.search(r"\d+", str(value or ""))
    return max(0, min(100, int(m.group(0)))) if m else default


def _get(url: str, timeout: float = 4.0) -> Any:
    status, raw = http("GET", url, timeout=timeout)
    if status == 401 or status == 403:
        raise HomeError("The device asked for a password: turn its login off in its own app, or reach it through "
                        "Home Assistant.")
    if status >= 400:
        raise HomeError(f"The device answered {status}.")
    return _json(raw)


def _base(host: str, port: int = 80) -> str:
    """The device's address; `host` may carry its own port ("192.168.0.30:8080")."""
    m = re.fullmatch(r"([^:\[\]]+):(\d{2,5})", (host or "").strip())
    if m:
        host, port = m.group(1), int(m.group(2))
    ip = resolve_local(host, port)
    shown = f"[{ip}]" if ":" in ip else ip
    return f"http://{shown}" if port == 80 else f"http://{shown}:{port}"


# ── Philips Hue ──────────────────────────────────────────────────────────────


def hue_config(host: str, timeout: float = 2.0) -> Optional[dict[str, Any]]:
    """What the bridge says of itself before any key: its name, id and model, or None for anything else."""
    try:
        v = _get(_base(host) + "/api/config", timeout)
    except HomeError:
        return None
    if not isinstance(v, dict) or not v.get("bridgeid"):
        return None
    return {"name": str(v.get("name") or "Philips Hue")[:60], "bridgeid": str(v["bridgeid"])[:32],
            "model": str(v.get("modelid") or "")[:32]}


def hue_pair(host: str) -> Optional[str]:
    """A key, once the bridge's button was pressed; None while it was not."""
    status, raw = http("POST", _base(host) + "/api", body={"devicetype": HUE_APP}, timeout=4.0)
    v = _json(raw)
    if isinstance(v, list) and v and isinstance(v[0], dict):
        if "success" in v[0] and isinstance(v[0]["success"], dict) and v[0]["success"].get("username"):
            return str(v[0]["success"]["username"])
        error = v[0].get("error") if isinstance(v[0].get("error"), dict) else {}
        if error.get("type") == 101:
            return None
        raise HomeError(f"The Hue bridge said: {error.get('description') or 'no'}.")
    raise HomeError(f"The Hue bridge answered {status}.")


def _hue_call(host: str, key: str, method: str, path: str, body: Any = None) -> Any:
    if not re.fullmatch(r"[A-Za-z0-9\-]{10,64}", key or ""):
        raise HomeError("The Hue bridge's key is missing: pair it again (press its button).")
    status, raw = http(method, f"{_base(host)}/api/{key}{path}", body=body, timeout=5.0)
    v = _json(raw)
    if isinstance(v, list) and v and isinstance(v[0], dict) and isinstance(v[0].get("error"), dict):
        if v[0]["error"].get("type") == 1:
            raise HomeError("The Hue bridge no longer knows AXYON: pair it again (press its button).")
        raise HomeError(f"The Hue bridge said: {v[0]['error'].get('description') or 'no'}.")
    if status >= 400:
        raise HomeError(f"The Hue bridge answered {status}.")
    return v


HUE_LIGHT_CAN = ["turn_on", "turn_off", "toggle", "set_brightness", "set_color", "state"]


def hue_entities(host: str, key: str) -> list[dict[str, Any]]:
    """The bridge's lights, its rooms and zones (each as one light) and their scenes."""
    lights = _hue_call(host, key, "GET", "/lights") or {}
    groups = _hue_call(host, key, "GET", "/groups") or {}
    scenes = _hue_call(host, key, "GET", "/scenes") or {}
    out: list[dict[str, Any]] = []
    rooms: dict[str, str] = {}
    for gid, g in (groups.items() if isinstance(groups, dict) else []):
        if isinstance(g, dict) and g.get("type") in ("Room", "Zone"):
            rooms[str(gid)] = str(g.get("name") or "")
    room_of: dict[str, str] = {}
    for gid, g in (groups.items() if isinstance(groups, dict) else []):
        if isinstance(g, dict) and g.get("type") == "Room":
            for lid in g.get("lights") or []:
                room_of[str(lid)] = str(g.get("name") or "")
    for lid, l in (lights.items() if isinstance(lights, dict) else []):
        if not isinstance(l, dict):
            continue
        s = l.get("state") if isinstance(l.get("state"), dict) else {}
        state: dict[str, Any] = {"online": bool(s.get("reachable", True)), "on": bool(s.get("on"))}
        if isinstance(s.get("bri"), int):
            state["brightness"] = round(s["bri"] * 100 / 254)
        colour = "xy" in s or "hue" in s
        out.append({"id": f"hue:{host}/light/{lid}", "name": str(l.get("name") or f"Hue {lid}")[:80],
                    "room": room_of.get(str(lid), ""), "kind": "light", "source": "Philips Hue",
                    "can": HUE_LIGHT_CAN if colour else [c for c in HUE_LIGHT_CAN if c != "set_color"],
                    "asksFirst": False, "state": state})
    for gid, name in rooms.items():
        g = groups[gid]
        s = g.get("state") if isinstance(g.get("state"), dict) else {}
        out.append({"id": f"hue:{host}/group/{gid}", "name": f"{name} (todas)"[:80], "room": name, "kind": "light",
                    "source": "Philips Hue", "can": HUE_LIGHT_CAN, "asksFirst": False,
                    "state": {"online": True, "on": bool(s.get("any_on"))}})
    kept = 0
    for sid, sc in (scenes.items() if isinstance(scenes, dict) else []):
        if not isinstance(sc, dict) or sc.get("type") != "GroupScene" or str(sc.get("group")) not in rooms:
            continue
        if kept >= 40:
            break
        kept += 1
        room = rooms[str(sc["group"])]
        out.append({"id": f"hue:{host}/scene/{sc['group']}/{sid}", "name": f"{sc.get('name') or sid} · {room}"[:80],
                    "room": room, "kind": "scene", "source": "Philips Hue", "can": ["run"], "asksFirst": False,
                    "state": {"online": True}})
    return out


def hue_act(host: str, key: str, path: str, action: str, value: Any) -> str:
    """`path`: light/<id>, group/<id> or scene/<group>/<id>."""
    parts = path.split("/")
    if parts[0] == "scene" and len(parts) == 3:
        if action != "run":
            raise HomeError("A scene can only run.")
        _hue_call(host, key, "PUT", f"/groups/{parts[1]}/action", {"scene": parts[2]})
        return "Scene on."
    if parts[0] not in ("light", "group") or len(parts) != 2 or not parts[1].isdigit():
        raise HomeError("Not a Hue light.")
    target = f"/lights/{parts[1]}/state" if parts[0] == "light" else f"/groups/{parts[1]}/action"
    if action == "state":
        v = _hue_call(host, key, "GET", f"/lights/{parts[1]}" if parts[0] == "light" else f"/groups/{parts[1]}")
        s = (v or {}).get("state") or (v or {}).get("action") or {}
        return json.dumps({"on": s.get("on"), "brightness": round((s.get("bri") or 0) * 100 / 254)})
    if action == "toggle":
        v = _hue_call(host, key, "GET", f"/lights/{parts[1]}" if parts[0] == "light" else f"/groups/{parts[1]}")
        s = (v or {}).get("state") or {}
        on = s.get("any_on") if parts[0] == "group" else s.get("on")
        action = "turn_off" if on else "turn_on"
    if action == "turn_on":
        body: dict[str, Any] = {"on": True}
    elif action == "turn_off":
        body = {"on": False}
    elif action == "set_brightness":
        p = percent(value)
        body = {"on": p > 0, "bri": max(1, round(p * 254 / 100))} if p > 0 else {"on": False}
    elif action == "set_color":
        body = {"on": True, "xy": list(rgb_to_xy(*parse_colour(value)))}
    else:
        raise HomeError(f"A Hue light cannot do {action}.")
    _hue_call(host, key, "PUT", target, body)
    return {"turn_on": "On.", "turn_off": "Off."}.get(action, "Done.")


# ── Shelly ───────────────────────────────────────────────────────────────────


def shelly_info(host: str, timeout: float = 2.0) -> Optional[dict[str, Any]]:
    """A Shelly's own description: its generation, model and name, and whether it has a login."""
    try:
        v = _get(_base(host) + "/shelly", timeout)
    except HomeError:
        return None
    if not isinstance(v, dict) or not (v.get("type") or v.get("model") or v.get("gen")):
        return None
    gen = int(v.get("gen") or 1)
    return {"gen": gen, "model": str(v.get("model") or v.get("type") or "Shelly")[:40],
            "name": str(v.get("name") or v.get("id") or "Shelly")[:60],
            "auth": bool(v.get("auth") or v.get("auth_en")), "outputs": int(v.get("num_outputs") or 0)}


def shelly_channels(host: str, gen: int) -> list[dict[str, Any]]:
    """Its relays and lights: [{"kind": "switch"|"light", "channel": n, "on": bool}]."""
    if gen >= 2:
        v = _get(_base(host) + "/rpc/Shelly.GetStatus") or {}
        out = []
        for key, comp in (v.items() if isinstance(v, dict) else []):
            m = re.fullmatch(r"(switch|light):(\d+)", key)
            if m and isinstance(comp, dict):
                out.append({"kind": m.group(1), "channel": int(m.group(2)), "on": bool(comp.get("output"))})
        return sorted(out, key=lambda c: (c["kind"], c["channel"]))
    v = _get(_base(host) + "/status") or {}
    out = [{"kind": "switch", "channel": i, "on": bool(r.get("ison"))}
           for i, r in enumerate(v.get("relays") or []) if isinstance(r, dict)]
    out += [{"kind": "light", "channel": i, "on": bool(r.get("ison")), "brightness": r.get("brightness")}
            for i, r in enumerate(v.get("lights") or []) if isinstance(r, dict)]
    return out


def shelly_act(host: str, gen: int, kind: str, channel: int, action: str, value: Any) -> str:
    base = _base(host)
    if action == "state":
        for c in shelly_channels(host, gen):
            if c["kind"] == kind and c["channel"] == channel:
                return json.dumps({"on": c["on"]})
        raise HomeError("The Shelly does not have that output.")
    if gen >= 2:
        comp = "Switch" if kind == "switch" else "Light"
        if action == "toggle":
            path = f"/rpc/{comp}.Toggle?id={channel}"
        elif action in ("turn_on", "turn_off"):
            path = f"/rpc/{comp}.Set?id={channel}&on={'true' if action == 'turn_on' else 'false'}"
        elif action == "set_brightness" and kind == "light":
            path = f"/rpc/Light.Set?id={channel}&on=true&brightness={max(1, percent(value))}"
        else:
            raise HomeError(f"The Shelly cannot do {action}.")
    else:
        part = "relay" if kind == "switch" else "light"
        if action in ("turn_on", "turn_off", "toggle"):
            path = f"/{part}/{channel}?turn={action.replace('turn_', '')}"
        elif action == "set_brightness" and kind == "light":
            path = f"/light/{channel}?turn=on&brightness={max(1, percent(value))}"
        else:
            raise HomeError(f"The Shelly cannot do {action}.")
    _get(base + path)
    return {"turn_on": "On.", "turn_off": "Off."}.get(action, "Done.")


# ── Tasmota ──────────────────────────────────────────────────────────────────


def _tasmota(host: str, command: str, timeout: float = 4.0) -> Any:
    return _get(_base(host) + "/cm?cmnd=" + urllib.parse.quote(command), timeout)


def tasmota_info(host: str, timeout: float = 2.0) -> Optional[dict[str, Any]]:
    try:
        v = _tasmota(host, "Status 0", timeout)
    except HomeError:
        return None
    if not isinstance(v, dict) or not isinstance(v.get("Status"), dict):
        return None
    status = v["Status"]
    names = status.get("FriendlyName") if isinstance(status.get("FriendlyName"), list) else []
    sts = v.get("StatusSTS") if isinstance(v.get("StatusSTS"), dict) else {}
    return {"name": str(status.get("DeviceName") or (names[0] if names else "") or "Tasmota")[:60],
            "light": "Dimmer" in sts, "colour": "Color" in sts,
            "model": str((v.get("StatusFWR") or {}).get("Hardware") or "Tasmota")[:40]}


def tasmota_act(host: str, action: str, value: Any) -> str:
    if action == "state":
        v = _tasmota(host, "State") or {}
        out: dict[str, Any] = {"on": str(v.get("POWER") or v.get("POWER1") or "").upper() == "ON"}
        if isinstance(v.get("Dimmer"), int):
            out["brightness"] = v["Dimmer"]
        return json.dumps(out)
    if action in ("turn_on", "turn_off", "toggle"):
        v = _tasmota(host, "Power " + {"turn_on": "On", "turn_off": "Off", "toggle": "Toggle"}[action])
    elif action == "set_brightness":
        v = _tasmota(host, f"Dimmer {percent(value)}")
    elif action == "set_color":
        r, g, b = parse_colour(value)
        v = _tasmota(host, f"Color {r:02X}{g:02X}{b:02X}")
    else:
        raise HomeError(f"Tasmota cannot do {action}.")
    if isinstance(v, dict) and v.get("Command") == "Unknown":
        raise HomeError("Tasmota did not know that command.")
    return {"turn_on": "On.", "turn_off": "Off."}.get(action, "Done.")


# ── WLED ─────────────────────────────────────────────────────────────────────


def wled_info(host: str, timeout: float = 2.0) -> Optional[dict[str, Any]]:
    try:
        v = _get(_base(host) + "/json/info", timeout)
    except HomeError:
        return None
    if not isinstance(v, dict) or str(v.get("brand") or "").upper() != "WLED":
        return None
    return {"name": str(v.get("name") or "WLED")[:60], "model": f"WLED {v.get('ver') or ''}".strip()[:40]}


def wled_act(host: str, action: str, value: Any) -> str:
    url = _base(host) + "/json/state"
    if action == "state":
        v = _get(url) or {}
        return json.dumps({"on": bool(v.get("on")), "brightness": round((v.get("bri") or 0) * 100 / 255)})
    if action == "turn_on":
        body: dict[str, Any] = {"on": True}
    elif action == "turn_off":
        body = {"on": False}
    elif action == "toggle":
        body = {"on": "t"}
    elif action == "set_brightness":
        p = percent(value)
        body = {"on": p > 0, "bri": max(1, round(p * 255 / 100))}
    elif action == "set_color":
        body = {"on": True, "seg": [{"col": [list(parse_colour(value))]}]}
    elif action == "preset":
        body = {"ps": max(1, min(250, percent(value, 1)))}
    else:
        raise HomeError(f"WLED cannot do {action}.")
    status, _raw = http("POST", url, body=body, timeout=4.0)
    if status >= 400:
        raise HomeError(f"WLED answered {status}.")
    return {"turn_on": "On.", "turn_off": "Off."}.get(action, "Done.")


# ── Roku ─────────────────────────────────────────────────────────────────────

# The keys a Roku remote has, under the names the other TVs use.
ROKU_KEYS = {"home": "Home", "back": "Back", "ok": "Select", "enter": "Select", "up": "Up", "down": "Down",
             "left": "Left", "right": "Right", "play_pause": "Play", "play": "Play", "pause": "Play",
             "menu": "Info", "info": "Info", "rewind": "Rev", "forward": "Fwd", "replay": "InstantReplay",
             "search": "Search", "source": "InputHDMI1", "hdmi1": "InputHDMI1", "hdmi2": "InputHDMI2",
             "hdmi3": "InputHDMI3", "tv": "InputTuner", "volume_up": "VolumeUp", "volume_down": "VolumeDown",
             "mute": "VolumeMute", "channel_up": "ChannelUp", "channel_down": "ChannelDown",
             "power": "Power", "turn_on": "PowerOn", "turn_off": "PowerOff"}
ROKU_CAN = ["turn_on", "turn_off", "volume_up", "volume_down", "mute", "channel_up", "channel_down", "open_app",
            "key", "play_pause", "state"]


def _xml_field(xml: str, name: str) -> str:
    m = re.search(rf"<{name}>([^<]{{0,200}})</{name}>", xml)
    return _unescape(m.group(1).strip()) if m else ""


def _unescape(text: str) -> str:
    return (text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", '"')
            .replace("&apos;", "'"))


def roku_info(host: str, port: int = ROKU_PORT, timeout: float = 2.0) -> Optional[dict[str, Any]]:
    try:
        status, raw = http("GET", _base(host, port) + "/query/device-info", timeout=timeout, limit=64_000)
    except HomeError:
        return None
    xml = raw.decode("utf-8", "replace")
    if status >= 400 or "<device-info>" not in xml:
        return None
    name = _xml_field(xml, "user-device-name") or _xml_field(xml, "friendly-device-name") or "Roku"
    return {"name": name[:60], "model": (_xml_field(xml, "friendly-model-name") or _xml_field(xml, "model-name"))[:40],
            "power": _xml_field(xml, "power-mode"), "tv": _xml_field(xml, "is-tv") == "true"}


def roku_apps(host: str, port: int = ROKU_PORT) -> list[dict[str, str]]:
    status, raw = http("GET", _base(host, port) + "/query/apps", timeout=4.0, limit=256_000)
    if status >= 400:
        return []
    found = re.findall(r'<app id="([\w.\-]{1,40})"[^>]*>([^<]{1,80})</app>', raw.decode("utf-8", "replace"))
    return [{"id": i, "name": _unescape(n.strip())} for i, n in found][:80]


def roku_key(host: str, key: str, port: int = ROKU_PORT, times: int = 1) -> None:
    if key not in set(ROKU_KEYS.values()):
        raise HomeError(f"No key {key} on a Roku.")
    for _ in range(max(1, min(15, times))):
        status, _raw = http("POST", _base(host, port) + "/keypress/" + key, timeout=4.0)
        if status == 403:
            raise HomeError("The Roku refused: on the TV, Settings → System → Advanced system settings → Control by "
                            "mobile apps → Network access: Default (or Permissive).")
        if status >= 400:
            raise HomeError(f"The Roku answered {status}.")


def roku_launch(host: str, app_id: str, port: int = ROKU_PORT) -> None:
    if not re.fullmatch(r"[\w.\-]{1,40}", app_id):
        raise HomeError("Not a Roku app.")
    status, _raw = http("POST", _base(host, port) + "/launch/" + app_id, timeout=6.0)
    if status >= 400:
        raise HomeError(f"The Roku answered {status}.")


# ── Webhooks ─────────────────────────────────────────────────────────────────


def webhook_url(url: str) -> str:
    """A webhook's address, checked: https anywhere, http on the local network only, no login in it."""
    url = (url or "").strip()
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme.lower() not in ("http", "https") or not parsed.hostname:
        raise HomeError("A webhook is an address that starts with https:// (or http:// on your Wi-Fi).")
    if parsed.username or parsed.password:
        raise HomeError("Put no login in the address: a key in the address itself is fine.")
    if parsed.scheme.lower() == "http":
        resolve_local(parsed.hostname, parsed.port or 80)
    if len(url) > 2000:
        raise HomeError("The address is too long.")
    return url


def webhook_call(url: str, method: str, body: str) -> str:
    url = webhook_url(url)
    method = "POST" if (method or "").upper() == "POST" else "GET"
    data: Any = None
    headers: dict[str, str] = {"User-Agent": "AXYON"}
    if method == "POST" and body:
        try:
            data = json.loads(body)
        except ValueError:
            data = None
    if method == "POST" and data is None and body:
        # Plain text: sent as it is.
        status, _raw = _http_text(url, body, headers)
    else:
        status, _raw = http(method, url, body=data if method == "POST" else None, headers=headers, timeout=10.0,
                            limit=64_000)
    if status >= 400:
        raise HomeError(f"The webhook answered {status}.")
    return f"Called ({status})."


def _http_text(url: str, body: str, headers: dict[str, str]) -> tuple[int, bytes]:
    import urllib.error
    import urllib.request

    from .home import _OPENER

    request = urllib.request.Request(url, data=body.encode("utf-8"), method="POST",
                                     headers={**headers, "Content-Type": "text/plain; charset=utf-8"})
    try:
        with _OPENER.open(request, timeout=10.0) as response:
            return response.status, response.read(64_000)
    except urllib.error.HTTPError as e:
        return e.code, b""
    except (urllib.error.URLError, OSError) as e:
        raise HomeError(f"It did not answer ({getattr(e, 'reason', e)}).") from None


__all__ = ["COLOURS", "HUE_LIGHT_CAN", "ROKU_CAN", "ROKU_KEYS", "ROKU_PORT", "hue_act", "hue_config", "hue_entities",
           "hue_pair", "is_local", "parse_colour", "percent", "rgb_to_xy", "roku_apps", "roku_info", "roku_key",
           "roku_launch", "shelly_act", "shelly_channels", "shelly_info", "tasmota_act", "tasmota_info", "webhook_call",
           "webhook_url", "wled_act", "wled_info"]
