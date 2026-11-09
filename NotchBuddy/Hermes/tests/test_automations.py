"""AXYON's ecosystem on the iPhone, against simulated devices on this machine's network address: a Philips Hue bridge
(a key only after its button is pressed, then its lights, rooms and scenes), Shelly Gen1 and Gen2, Tasmota, WLED, a
Roku TV, and a webhook; then the routines (automations.py): made, checked, run step by step (devices, every light
at once, the iPhone's steps, a pause, a question to AXYON Core), the clock's time routines, and AXYON's own tool that
asks the owner's Face ID before it keeps one. The Keychain, Face ID and the iPhone's actions are the app's: a
stand-in bridge here. `python3 test_automations.py` (standard library only)."""
import json
import os
import re
import shutil
import socket
import sys
import tempfile
import threading
import time
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, unquote, urlparse

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "app"))

from axyon_hermes import home, lan  # noqa: E402
from axyon_hermes.automations import RoutineError, Routines, clean  # noqa: E402

failures = 0


def check(label, ok):
    global failures
    print(("  ✓ " if ok else "  ✗ ") + label)
    if not ok:
        failures += 1


def lan_address():
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        s.connect(("192.0.2.1", 9))
        return s.getsockname()[0]
    finally:
        s.close()


HOST = lan_address()


class Bridge:
    """The app's side: the Keychain, Face ID and the iPhone's own actions."""

    def __init__(self):
        self.keychain = {}
        self.allow = True
        self.phone = []

    def ask(self, kind, request, wait=45):
        if kind == "secret":
            name = request["name"]
            if request["do"] == "get":
                return {"value": self.keychain.get(name, "")}
            if request["value"]:
                self.keychain[name] = request["value"]
            else:
                self.keychain.pop(name, None)
            return {"ok": True}
        if kind == "iphone" and request.get("action") == "confirm":
            self.phone.append(("confirm", request.get("text")))
            return {"ok": self.allow}
        if kind == "iphone":
            self.phone.append((request.get("action"), request))
            return {"ok": True}
        return {"error": "unknown"}


class Device:
    """One simulated device: an HTTP server whose answers `route` gives, every call kept."""

    def __init__(self, route):
        self.calls = []
        device = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _go(self, method):
                length = int(self.headers.get("Content-Length") or 0)
                raw = self.rfile.read(length) if length else b""
                try:
                    body = json.loads(raw) if raw else None
                except ValueError:
                    body = raw.decode()
                device.calls.append((method, unquote(self.path), body))
                status, out, kind = route(method, unquote(self.path), body)
                data = out if isinstance(out, bytes) else json.dumps(out).encode()
                self.send_response(status)
                self.send_header("Content-Type", kind)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def do_GET(self):
                self._go("GET")

            def do_POST(self):
                self._go("POST")

            def do_PUT(self):
                self._go("PUT")

        self.server = ThreadingHTTPServer((HOST, 0), Handler)
        self.address = f"{HOST}:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()


J = "application/json"

# ── Philips Hue ──────────────────────────────────────────────────────────────

hue_state = {"pressed": False, "lights": {"1": {"on": False, "bri": 254, "xy": [0.3, 0.3], "reachable": True},
                                          "2": {"on": True, "bri": 127, "reachable": True}}}


def hue_route(method, path, body):
    if path == "/api/config":
        return 200, {"name": "Hue da casa", "bridgeid": "ECB5FAFFFE000001", "modelid": "BSB002"}, J
    if path == "/api" and method == "POST":
        if not hue_state["pressed"]:
            return 200, [{"error": {"type": 101, "description": "link button not pressed"}}], J
        return 200, [{"success": {"username": "axyonKey1234567890abcdef"}}], J
    m = re.fullmatch(r"/api/([^/]+)(/.*)?", path)
    if not m or m.group(1) != "axyonKey1234567890abcdef":
        return 200, [{"error": {"type": 1, "description": "unauthorized user"}}], J
    rest = m.group(2) or ""
    if rest == "/lights":
        return 200, {lid: {"name": "Abajur" if lid == "1" else "Teto", "state": s}
                     for lid, s in hue_state["lights"].items()}, J
    if rest == "/groups":
        return 200, {"1": {"name": "Sala", "type": "Room", "lights": ["1", "2"],
                           "state": {"any_on": any(s["on"] for s in hue_state["lights"].values())}},
                     "2": {"name": "Entretenimento", "type": "Entertainment", "lights": ["1"]}}, J
    if rest == "/scenes":
        return 200, {"abc": {"name": "Relaxar", "type": "GroupScene", "group": "1"},
                     "old": {"name": "Antiga", "type": "LightScene"}}, J
    if rest.startswith("/lights/") and rest.endswith("/state") and method == "PUT":
        lid = rest.split("/")[2]
        hue_state["lights"][lid].update({k: v for k, v in body.items()})
        return 200, [{"success": {}}], J
    if rest.startswith("/lights/"):
        return 200, {"state": hue_state["lights"][rest.split("/")[2]]}, J
    if rest.startswith("/groups/") and rest.endswith("/action"):
        return 200, [{"success": {}}], J
    return 404, {}, J


# ── Shelly ───────────────────────────────────────────────────────────────────

gen1 = {"relays": [{"ison": False}, {"ison": False}]}


def shelly1_route(method, path, body):
    if path == "/shelly":
        return 200, {"type": "SHSW-25", "auth": False, "num_outputs": 2}, J
    if path == "/status":
        return 200, gen1, J
    m = re.fullmatch(r"/relay/(\d)\?turn=(on|off|toggle)", path)
    if m:
        relay = gen1["relays"][int(m.group(1))]
        relay["ison"] = (not relay["ison"]) if m.group(2) == "toggle" else m.group(2) == "on"
        return 200, relay, J
    return 404, {}, J


gen2 = {"switch:0": {"output": False}}


def shelly2_route(method, path, body):
    if path == "/shelly":
        return 200, {"name": "Tomada da cafeteira", "gen": 2, "model": "S3PL-00112EU", "auth_en": False}, J
    if path == "/rpc/Shelly.GetStatus":
        return 200, {**gen2, "sys": {}}, J
    m = re.fullmatch(r"/rpc/Switch\.Set\?id=0&on=(true|false)", path)
    if m:
        gen2["switch:0"]["output"] = m.group(1) == "true"
        return 200, {"was_on": False}, J
    return 404, {}, J


def shelly_locked_route(method, path, body):
    return 200, {"type": "SHPLG-S", "auth": True}, J


# ── Tasmota, WLED, Roku, a webhook ───────────────────────────────────────────

tasmota_state = {"POWER": "OFF", "Dimmer": 100}


def tasmota_route(method, path, body):
    q = parse_qs(urlparse(path).query).get("cmnd", [""])[0]
    if q == "Status 0":
        return 200, {"Status": {"DeviceName": "Luminária", "FriendlyName": ["Luminária"]},
                     "StatusSTS": {"POWER": tasmota_state["POWER"], "Dimmer": 100, "Color": "FFFFFF"},
                     "StatusFWR": {"Hardware": "ESP8266EX"}}, J
    if q == "State":
        return 200, tasmota_state, J
    if q.startswith("Power "):
        tasmota_state["POWER"] = q.split()[1].upper()
        return 200, {"POWER": tasmota_state["POWER"]}, J
    if q.startswith("Dimmer "):
        tasmota_state["Dimmer"] = int(q.split()[1])
        return 200, {"Dimmer": tasmota_state["Dimmer"]}, J
    if q.startswith("Color "):
        return 200, {"Color": q.split()[1]}, J
    return 200, {"Command": "Unknown"}, J


wled_state = {"on": False, "bri": 128}


def wled_route(method, path, body):
    if path == "/json/info":
        return 200, {"name": "Fita da TV", "ver": "0.15.0", "brand": "WLED"}, J
    if path == "/json/state" and method == "GET":
        return 200, wled_state, J
    if path == "/json/state" and method == "POST":
        if body.get("on") == "t":
            wled_state["on"] = not wled_state["on"]
        elif "on" in body:
            wled_state["on"] = body["on"]
        if "bri" in body:
            wled_state["bri"] = body["bri"]
        return 200, {"success": True}, J
    return 404, {}, J


ROKU_INFO = (b"<?xml version=\"1.0\"?><device-info><friendly-device-name>TCL Roku TV</friendly-device-name>"
             b"<user-device-name>TV da sala</user-device-name><friendly-model-name>TCL 43S5400</friendly-model-name>"
             b"<power-mode>PowerOn</power-mode><is-tv>true</is-tv></device-info>")
ROKU_APPS = (b"<apps><app id=\"12\" type=\"appl\" version=\"5\">Netflix</app>"
             b"<app id=\"837\" type=\"appl\" version=\"1\">YouTube</app>"
             b"<app id=\"tvinput.hdmi1\" type=\"tvin\" version=\"1\">HDMI 1</app></apps>")


def roku_route(method, path, body):
    if path == "/query/device-info":
        return 200, ROKU_INFO, "text/xml"
    if path == "/query/apps":
        return 200, ROKU_APPS, "text/xml"
    if method == "POST" and (path.startswith("/keypress/") or path.startswith("/launch/")):
        return 200, b"", "text/plain"
    return 404, b"", "text/plain"


def hook_route(method, path, body):
    return 200, {"ok": True}, J


work = tempfile.mkdtemp()
bridge = Bridge()
h = home.Home(work, bridge=bridge)

print("Colours")
check("a colour by its Portuguese name", lan.parse_colour("Azul") == (0, 60, 255))
check("by #RRGGBB", lan.parse_colour("#ff8000") == (255, 128, 0))
x, y = lan.rgb_to_xy(255, 0, 0)
check("red in the Hue lights' xy", 0.6 < x < 0.75 and 0.25 < y < 0.35)
try:
    lan.parse_colour("cor de burro quando foge")
    check("an unknown colour is told", False)
except home.HomeError as e:
    check("an unknown colour is told", "Which colour" in str(e))

print("Philips Hue")
hue = Device(hue_route)
added = h.add({"driver": "hue", "host": hue.address})
check("the bridge added from what it says of itself", added["name"] == "Hue da casa" and added["source"] == "Philips Hue"
      and not added["paired"])
home.HUE_PAIR_WAIT = 3.0
try:
    h.pair(added["id"])
    check("its button not pressed: said", False)
except home.HomeError as e:
    check("its button not pressed: said", "round button" in str(e))
hue_state["pressed"] = True
said = h.pair(added["id"])
check("paired once pressed, the key in the Keychain", said["ok"] and bridge.keychain[f"home.hue.{hue.address}"]
      == "axyonKey1234567890abcdef" and "4 lights, rooms and scenes" in said["message"])
listed = h.overview(states=False)["devices"]
names = {d["name"]: d for d in listed}
check("its lights, its room and the room's scene (an old one left out)", "Abajur" in names and "Sala (todas)" in names
      and "Relaxar · Sala" in names and not any(n.startswith("Antiga") for n in names))
check("a light knows its room and its colours", names["Abajur"]["room"] == "Sala" and "set_color" in names["Abajur"]["can"])
check("a white-only light has no colour", "set_color" not in names["Teto"]["can"])
done = h.act("Abajur", "set_color", "vermelho")
check("a light by its name, red", done["ok"] and hue_state["lights"]["1"]["on"] is True and hue_state["lights"]["1"]["xy"][0] > 0.6)
h.act("Abajur", "set_brightness", 30)
check("at 30 %", hue_state["lights"]["1"]["bri"] == round(30 * 254 / 100))
h.act(names["Relaxar · Sala"]["id"], "run")
check("a scene runs on its room", hue.calls[-1][1].endswith("/groups/1/action") and hue.calls[-1][2] == {"scene": "abc"})
h.act("Sala (todas)", "turn_off")
check("the whole room off", hue.calls[-1][2] == {"on": False})

print("Shelly, Tasmota, WLED")
s1 = Device(shelly1_route)
added = h.add({"driver": "shelly", "host": s1.address, "name": "Quarto"})
listed = {d["name"]: d for d in h.overview(states=False)["devices"]}
check("a two-relay Shelly: one plug each", "Quarto 1" in listed and "Quarto 2" in listed and listed["Quarto 1"]["asksFirst"])
h.act("Quarto 2", "turn_on")
check("its second relay on (Gen1)", gen1["relays"][1]["ison"] is True and gen1["relays"][0]["ison"] is False)
s2 = Device(shelly2_route)
h.add({"driver": "shelly", "host": s2.address})
bridge.allow = False
out = json.loads(h.tool_control({"device": "Tomada da cafeteira", "action": "turn_on"}))
check("AXYON on a plug asks Face ID; refused, nothing done", out.get("reason") == "declined" and not gen2["switch:0"]["output"])
bridge.allow = True
out = json.loads(h.tool_control({"device": "Tomada da cafeteira", "action": "turn_on"}))
check("allowed, on (Gen2 RPC)", out["ok"] and gen2["switch:0"]["output"] is True)
check("its state read", h.act("Tomada da cafeteira", "state")["state"]["on"] is True)
locked = Device(shelly_locked_route)
try:
    h.add({"driver": "shelly", "host": locked.address})
    check("a Shelly with a login is told", False)
except home.HomeError as e:
    check("a Shelly with a login is told", "has a login" in str(e))
ta = Device(tasmota_route)
added = h.add({"driver": "tasmota", "host": ta.address, "room": "Escritório"})
check("Tasmota: a dimmable, coloured light", added["kind"] == "light" and "set_color" in added["can"])
h.act("Luminária", "turn_on")
h.act("Luminária", "set_brightness", 55)
check("on, at 55 %", tasmota_state["POWER"] == "ON" and tasmota_state["Dimmer"] == 55)
wl = Device(wled_route)
h.add({"driver": "wled", "host": wl.address})
h.act("Fita da TV", "toggle")
check("WLED toggled", wled_state["on"] is True)
h.act("Fita da TV", "set_color", "roxo")
check("its colour", wl.calls[-1][2] == {"on": True, "seg": [{"col": [[140, 0, 255]]}]})

print("Roku TV")
ro = Device(roku_route)
added = h.add({"driver": "roku", "host": ro.address})
check("the TV by its own name, its apps read", added["name"] == "TV da sala" and "Netflix" in added["apps"])
h.act("TV da sala", "volume_up", 3)
check("three steps of volume", [c[1] for c in ro.calls[-3:]] == ["/keypress/VolumeUp"] * 3)
h.act("TV da sala", "open_app", "netflix")
check("Netflix opened", ro.calls[-1][1] == "/launch/12")
h.act("TV da sala", "key", "ok")
check("OK is Select on a Roku", ro.calls[-1][1] == "/keypress/Select")
h.act("TV da sala", "turn_off")
check("off", ro.calls[-1][1] == "/keypress/PowerOff")
try:
    h.act("TV da sala", "set_color", "azul")
    check("what a TV cannot do is told", False)
except home.HomeError as e:
    check("what a TV cannot do is told", "cannot do set_color" in str(e))

print("Webhooks")
hook = Device(hook_route)
try:
    h.add({"driver": "webhook", "name": "Alexa", "url": "http://8.8.8.8/hook"})
    check("plain http only on the Wi-Fi", False)
except home.HomeError as e:
    check("plain http only on the Wi-Fi", "local network" in str(e))
added = h.add({"driver": "webhook", "name": "Portão da garagem", "url": f"http://{hook.address}/trigger/portao?key=s3cr3t",
               "method": "POST", "body": '{"value1": "abrir"}'})
check("kept with its address in the Keychain, not in AXYON's file",
      bridge.keychain["home.webhook.portao-da-garagem"].endswith("key=s3cr3t")
      and "s3cr3t" not in open(os.path.join(work, "axyon-home.json")).read())
check("a webhook asks Face ID when AXYON calls it", added["asksFirst"] and added["can"] == ["run"])
h.act("Portão da garagem", "run")
check("called with its body", hook.calls[-1] == ("POST", "/trigger/portao?key=s3cr3t", {"value1": "abrir"}))

print("Looking on the Wi-Fi")
found = [h._identify(d.address, 80) for d in (hue, s2, wl, ta)] + [h._identify(ro.address, 8060)]
check("each told apart by what it answers", [f and f["driver"] for f in found] == ["hue", "shelly", "wled", "tasmota", "roku"])

# ── Routines ─────────────────────────────────────────────────────────────────

print("Routines: what one is")
try:
    clean({"name": "x", "steps": [{"do": "every", "kind": "switch", "action": "turn_on"}]})
    check("every plug on at once is refused", False)
except RoutineError:
    check("every plug on at once is refused", True)
try:
    clean({"name": "x", "trigger": {"kind": "time", "at": "25:00"}, "steps": [{"do": "pause", "seconds": 2}]})
    check("a time that does not exist", False)
except RoutineError as e:
    check("a time that does not exist", "HH:MM" in str(e))
try:
    clean({"name": "x", "steps": [{"do": "iphone", "action": "open", "url": "javascript:alert(1)"}]})
    check("no script as a link", False)
except RoutineError:
    check("no script as a link", True)
r = clean({"name": "Cheguei", "trigger": {"kind": "arrive", "place": {"name": "Casa", "lat": -23.55, "lon": -46.63,
                                                                       "radius": 20}},
           "steps": [{"do": "pause", "seconds": 1}]})
check("a place's radius at least 100 m", r["trigger"]["place"]["radius"] == 100)

asked = []


def ask(prompt, session, timeout):
    asked.append((prompt, session))
    return "Hoje: reunião às 10h. A carteira abriu +0,4%."


routines = Routines(work, h, bridge=bridge, ask=ask)
night = routines.put({"name": "Boa noite", "icon": "moon.stars.fill", "steps": [
    {"do": "every", "kind": "light", "action": "turn_off"},
    {"do": "device", "device": "TV da sala", "action": "turn_off"},
    {"do": "device", "device": "Tomada da cafeteira", "action": "turn_off"},
    {"do": "iphone", "action": "reminder", "title": "Tomar o remédio", "inMinutes": 30},
    {"do": "iphone", "action": "notify", "body": "Casa apagada. Boa noite!"},
    {"do": "device", "device": "geladeira", "action": "turn_on"},
]})
try:
    routines.put({"name": "boa  NOITE", "steps": [{"do": "pause", "seconds": 1}]})
    check("two routines with one name are refused", False)
except RoutineError:
    check("two routines with one name are refused", True)

print("Routines: running one")
for lid in hue_state["lights"]:
    hue_state["lights"][lid]["on"] = True
wled_state["on"] = True
tasmota_state["POWER"] = "ON"
bridge.phone.clear()
out = routines.run("boa noite", origin="siri", wait=20)
check("found by its name as Siri says it, and run to the end", not out["running"] and len(out["steps"]) == 6)
check("every light off: Hue, Tasmota, WLED", not any(s["on"] for s in hue_state["lights"].values())
      and tasmota_state["POWER"] == "OFF" and wled_state["on"] is False)
check("the TV off", ro.calls[-1][1] == "/keypress/PowerOff")
check("a plug in the owner's own routine: no Face ID asked", gen2["switch:0"]["output"] is False
      and not any(k == "confirm" for k, _ in bridge.phone))
reminder = [req for k, req in bridge.phone if k == "reminders"]
check("the reminder for in 30 minutes", reminder and reminder[0]["do"] == "add" and reminder[0]["due"])
check("the notification", any(k == "notify" and req["body"] == "Casa apagada. Boa noite!" for k, req in bridge.phone))
check("a step that fails is said, and the others still run", out["steps"][-1]["ok"] is False and
      "geladeira" in out["steps"][-1]["said"] and all(s["ok"] for s in out["steps"][:-1]) and out["ok"] is False)
kept = routines.find(night["id"])
check("its last run kept, with where it came from", kept["lastRun"]["origin"] == "siri" and kept["lastRun"]["ok"] is False)

morning = routines.put({"name": "Bom dia", "trigger": {"kind": "time", "at": "07:30", "days": [1, 2, 3, 4, 5]},
                        "steps": [{"do": "ask", "prompt": "Meu resumo do dia"},
                                  {"do": "device", "device": "Abajur", "action": "set_brightness", "value": 60}]})
monday = datetime(2026, 11, 9, 7, 31)
check("not due before its minute", routines.slot(morning, datetime(2026, 11, 9, 7, 29)) is None)
check("not on a Saturday", routines.slot(morning, datetime(2026, 11, 14, 7, 31)) is None)
check("not hours late", routines.slot(morning, datetime(2026, 11, 9, 10, 0)) is None)
bridge.phone.clear()
ran = routines.tick(monday, wait=10)
check("due at 07:31 on a Monday: run by the clock", ran == [morning["id"]])
check("AXYON Core asked, in a session of its own", asked and asked[-1][0] == "Meu resumo do dia"
      and asked[-1][1].startswith(f"axyon-routine-{morning['id']}-"))
check("its answer as a notification titled with the routine", any(
    k == "notify" and req["title"] == "Bom dia" and "reunião às 10h" in req["body"] for k, req in bridge.phone))
check("once per day: the next tick runs nothing", routines.tick(datetime(2026, 11, 9, 7, 45)) == [])
time.sleep(0.3)
check("the Hue light at 60 % after the question", hue_state["lights"]["1"]["bri"] == round(60 * 254 / 100))

print("AXYON's own tool")
bridge.allow = False
out = json.loads(routines.tool({"do": "save", "routine": {
    "name": "Modo cinema", "icon": "film.fill",
    "steps": [{"do": "device", "device": "Sala (todas)", "action": "set_brightness", "value": 15},
              {"do": "device", "device": "TV da sala", "action": "open_app", "value": "Netflix"}]}}))
check("a routine AXYON makes asks the owner's Face ID; refused, not kept",
      out.get("reason") == "declined" and routines.find("Modo cinema") is None)
check("the owner was shown what it does", any(k == "confirm" and "Modo cinema" in t and "Netflix" in t
                                              for k, t in bridge.phone))
bridge.allow = True
out = json.loads(routines.tool({"do": "save", "routine": {
    "name": "Modo cinema", "steps": [{"do": "device", "device": "TV da sala", "action": "open_app", "value": "Netflix"}]}}))
check("allowed, kept as AXYON's", out["ok"] and routines.find("modo cinema")["by"] == "axyon")
out = json.loads(routines.tool({"do": "save", "routine": {"name": "Modo cinema", "steps": [
    {"do": "pause", "seconds": 2}, {"do": "device", "device": "TV da sala", "action": "open_app", "value": "YouTube"}]}}))
check("saved again by its name: changed, not doubled", out["ok"] and len([r for r in routines.load()
                                                                          if r["name"] == "Modo cinema"]) == 1)
listed = json.loads(routines.tool({"do": "list"}))
check("listed for AXYON with when and what", any(r["name"] == "Bom dia" and "07:30" in r["when"]
                                                 for r in listed["routines"]))
out = json.loads(routines.tool({"do": "run", "routine": "modo cinema"}))
check("run by AXYON", out["ok"] and ro.calls[-1][1] == "/launch/837")
out = json.loads(routines.tool({"do": "save", "routine": {"name": "Ruim", "steps": [{"do": "teleport"}]}}))
check("a step that does not exist: said, as JSON", out["ok"] is False and "A step is" in out["error"])
check("removed with Face ID", json.loads(routines.tool({"do": "remove", "routine": "Modo cinema"}))["ok"]
      and routines.find("Modo cinema") is None)

shutil.rmtree(work, ignore_errors=True)
print("All ecosystem tests passed." if failures == 0 else f"{failures} failed.")
sys.exit(1 if failures else 0)
