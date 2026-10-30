"""The host's own pieces, without Hermes: Google Finance's quote, the portfolio's figures, the web's results, the
iPhone tools' requests, the superpowers' tools and the bridge. `python3 test_units.py`"""
import json
import os
import sys
import tempfile
import threading
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "app"))

from axyon_hermes import bridge, coucou, iphone, powers, web  # noqa: E402

failures = 0


def check(label, ok):
    global failures
    print(("  ✓ " if ok else "  ✗ ") + label)
    if not ok:
        failures += 1


# Pieces of the quote pages as Google Finance sent them on 27 October 2026 (windows/powers/src/sources/gfinance.rs).
GGRC11 = r'''AF_initDataCallback({key: 'ds:2', hash: '9', data:[[[["/g/11c807gj6w",["GGRC11","BVMF"],"GGR Covepi Renda FII - RL",0,"BRL",[8.69,-0.1600008,-1.8079185,2,2,2],null,8.85,null,"BR","/g/11fzc7250v",[1793125002],"America/Sao_Paulo",-10800]]]]'''
TIMS3 = r'''data:[[[["/g/11dx9hs84p",["TIMS3","BVMF"],"Tim SA",0,"BRL",[18.78,0.6800003,3.7569077,2,2,2],null,18.1,"#063987","BR","/m/0bbzkq0",[1793124996],"America/Sao_Paulo",-10800]]]]'''
BOVA11 = r'''data:[[[["/g/12hdlpl2f",["BOVA11","BVMF"],"Ishares Ibovespa Fundo de Indice",5,"BRL",[172.95,0.5299988,0.30738822,2,2,2],null,172.42,null,"BR","/g/11byd1bjwj",[1793125045],"America/Sao_Paulo",-10800]]]]
data:[[[[null,["BOVA11","BVMF"]],null,174.5,"/g/12hdlpl2f",171.85,175.66,172.94,2,0.5200043,2,0.30159163,2,"BRL","BOVA11:BVMF","Ishares Ibovespa Fundo de Indice",172.42,0,7886171,"Capital market",["/g/12hdlpl2f",["BOVA11","BVMF"],"Ishares Ibovespa Fundo de Indice",5,"BRL",[172.94,0.5200043,0.30159163,2,2,2],null,172.42,null,"BR","/g/11byd1bjwj",[1793125039],"America/Sao_Paulo",-10800]]]]'''

print("Google Finance")
check("price, time and previous close", coucou.parse_quote(GGRC11, "ggrc11") == {"price": 8.69, "at": 1793125002, "previousClose": 8.85})
check("a color between the close and the time", coucou.parse_quote(TIMS3, "TIMS3") == {"price": 18.78, "at": 1793124996, "previousClose": 18.1})
check("the newest entry", coucou.parse_quote(BOVA11, "BOVA11")["at"] == 1793125045)
check("another asset's page has none", coucou.parse_quote(TIMS3, "GGRC11") is None)
check("an empty page has none", coucou.parse_quote("", "GGRC11") is None)
pages = {"GGRC11": GGRC11, "TIMS3": TIMS3}
opened = []
quotes = coucou.fetch_quotes(["ggrc11", "TIMS3", "TIMS3", "bad ticker", "ZZZZ11"],
                             opener=lambda url: (opened.append(url), pages.get(url.split("/quote/")[1].split(":")[0], ""))[1])
check("each ticker read once, the malformed left out", sorted(opened) == sorted(
    [f"https://www.google.com/finance/quote/{t}:BVMF?hl=en" for t in ("GGRC11", "TIMS3", "ZZZZ11")]))
check("the prices found", set(quotes) == {"GGRC11", "TIMS3"})

print("The portfolio")
copy = {"receivedAt": 1793100000, "cash": 100.0, "source": "Google Sheets",
        "positions": [{"ticker": "GGRC11", "class": "fii", "quantity": 100, "averagePrice": 9.0},
                      {"ticker": "TIMS3", "class": "acao", "quantity": 30, "averagePrice": 15.0},
                      {"ticker": "KNSC11", "class": "fii", "quantity": 10, "averagePrice": 9.5},
                      {"ticker": "RDB Nubank", "class": "renda_fixa", "quantity": 1, "averagePrice": 100, "currentValue": 104.5}],
        "ceilings": {"GGRC11": 8.8, "KNRI11": 150.0},
        "prices": {"KNSC11": {"price": 9.1, "close": 9.1, "closeDay": "2026-10-27"}},
        "notes": [{"ticker": "KNSC11", "text": "sem quantidade"}]}
report = coucou.portfolio_report(copy, quotes, 1793125100)
rows = {r["ticker"]: r for r in report["positions"]}
check("a live price and its value", rows["GGRC11"]["price"] == 8.69 and rows["GGRC11"]["value"] == 869.0)
check("the day in percent and money", rows["TIMS3"]["dayChangePct"] == 3.76 and abs(rows["TIMS3"]["dayChangeMoney"] - 30 * (18.78 - 18.1)) < 0.01)
check("the gain on the average price", rows["TIMS3"]["gainPct"] == 25.2)
check("no live price: the computer's last", rows["KNSC11"]["price"] == 9.1 and "2026-10-27" in rows["KNSC11"]["priceFrom"])
check("fixed income at what the user said it is worth", rows["RDB Nubank"]["value"] == 104.5)
check("how far from the user's ceiling", rows["GGRC11"]["vsCeilingPct"] == round((8.69 / 8.8 - 1) * 100, 2))
check("a watched asset", report["watchedWithCeiling"] == [{"ticker": "KNRI11", "ceiling": 150.0}])
check("the total with the cash", report["totalValue"] == round(869.0 + 30 * 18.78 + 91.0 + 104.5 + 100, 2))
check("the notes of AXYON's tab", report["notesInAxyonTab"][0]["ticker"] == "KNSC11")
check("the copy's age said", report["copyFromComputer"].startswith("2"))
check("the computer's copy said as such", report["keptOnIphone"] is False)

print("The wallet with the superpowers' own record")
work = tempfile.mkdtemp(prefix="axyon-wallet-")
powers_dir = os.path.join(work, "powers")
os.makedirs(powers_dir)
computer_file = os.path.join(work, "portfolio.json")
with open(computer_file, "w") as f:
    json.dump(dict(copy, receivedAt=1793100000), f)
wallet = coucou.Coucou(computer_file, powers_dir=powers_dir)
check("no record of its own yet: the computer's copy", wallet._copy()["receivedAt"] == 1793100000)
with open(os.path.join(powers_dir, "powers.json"), "w") as f:
    json.dump({"portfolio": {"positions": [{"ticker": "GARE11", "class": "fii", "quantity": 210, "averagePrice": 8.9,
                                            "subtype": "", "currentValue": None, "note": ""}],
                             "cash": 40.0, "source": "chat", "updated": "2026-10-28"},
               "ceilings": {"manual": {"GARE11": 8.5}}}, f)
with open(os.path.join(powers_dir, "powers-market.json"), "w") as f:
    json.dump({"quotes": {"GARE11": {"ticker": "GARE11", "date": "2026-10-27", "close": 8.31}}}, f)
check("a copy the engine never took is newer: still the computer's", wallet._copy()["receivedAt"] == 1793100000)
with open(os.path.join(powers_dir, "powers-copy.txt"), "w") as f:
    f.write("1793100000")
mine = wallet._copy()
check("once taken, the iPhone's own record", mine.get("keptOnIphone") and mine["positions"][0]["quantity"] == 210)
check("its ceilings and B3's last close", mine["ceilings"] == {"GARE11": 8.5}
      and mine["prices"]["GARE11"] == {"price": 8.31, "close": 8.31, "closeDay": "2026-10-27"})
own = coucou.portfolio_report(mine, {}, 1793125100)
check("valued at the last close, said kept on the iPhone", own["keptOnIphone"] and own["positions"][0]["value"] == round(210 * 8.31, 2)
      and own["totalValue"] == round(210 * 8.31 + 40, 2))
check("no computer at all: the iPhone's record", coucou.Coucou(os.path.join(work, "none.json"), powers_dir=powers_dir)._copy()["keptOnIphone"])
check("nothing anywhere: the screen says it", json.loads(coucou.Coucou(os.path.join(work, "none.json"),
                                                                       powers_dir=os.path.join(work, "x")).portfolio({}))["empty"])

print("The web")
SAMPLE = '''<div class="result results_links results_links_deep web-result ">
<h2 class="result__title"><a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fstatusinvest.com.br%2Ffundos%2Dimobiliarios%2Fgare11&amp;rut=abc">GARE11 - <b>Guardian</b> Real Estate</a></h2>
<a class="result__snippet" href="//duckduckgo.com/l/?uddg=x">Dividend yield de <b>12,5%</b> nos últimos 12 meses.</a>
</div>
<div class="result"><h2><a rel="nofollow" class="result__a" href="https://www.fundsexplorer.com.br/funds/gare11">Funds Explorer</a></h2></div>
<div class="result"><a class="result__a" href="javascript:alert(1)">bad</a></div>'''
results = web.parse_results(SAMPLE)
check("DuckDuckGo's redirect undone", results[0]["url"] == "https://statusinvest.com.br/fundos-imobiliarios/gare11")
check("the title and snippet as text", results[0]["title"] == "GARE11 - Guardian Real Estate" and "12,5%" in results[0]["snippet"])
check("a direct link kept, a script link left out", [r["url"] for r in results][1:] == ["https://www.fundsexplorer.com.br/funds/gare11"])
title, text = web.page_text("<html><head><title>Notícia &amp; mais</title><script>x()</script></head><body><nav>menu</nav>"
                            "<h1>Título</h1><p>Primeiro parágrafo.</p><p>Segundo.</p></body></html>")
check("a page's title", title == "Notícia & mais")
check("its text, without scripts or menus", text == "Título\nPrimeiro parágrafo.\nSegundo.")

print("The iPhone's tools")


class FakeBridge:
    def __init__(self):
        self.asked = []

    def ask(self, kind, request, wait=None):
        self.asked.append((kind, request))
        return {"ok": True}


fake = FakeBridge()
registered = {}


class FakeRegistry:
    def register(self, name, toolset, schema, handler, **kw):
        registered[name] = (toolset, schema, handler)


iphone.IPhone(fake).register(FakeRegistry())
check("thirteen iPhone tools (the alerts too), in Coucou's toolset", len(registered) == 13 and "iphone_alerts" in registered
      and all(t == "coucou" for t, _, _ in registered.values()))
out = registered["iphone_notify"][2]({"title": "Água", "in_seconds": 60, "body": None})
check("the action and its arguments go to the app", fake.asked[-1] == ("iphone", {"action": "notify", "title": "Água", "inSeconds": 60}))
check("the app's answer to the model", json.loads(out) == {"ok": True})
check("every schema says its name", all(s["name"] == n for n, (_, s, _) in registered.items()))

print("The superpowers")


class PowersBridge:
    def __init__(self, answer):
        self.answer, self.asked = answer, []

    def ask(self, kind, request, wait=None):
        self.asked.append((kind, request, wait))
        return self.answer


tools = powers.load_tools()
names = [t["name"] for t in tools]
check("the computer's tools, wealth_ and life_", len(names) == 19 and "wealth_portfolio" in names and "life_gym" in names
      and "wealth_documents" in names and "wealth_learn" in names)
check("the content kit stays on the computer", "life_content" not in names and not any(n.startswith("home_") for n in names))
check("each with its schema", all(t.get("inputSchema", {}).get("type") == "object" for t in tools))
check("no tool file: no tools", powers.load_tools(["/nonexistent/house_tools.json"]) == [])
registered.clear()
pb = PowersBridge({"ok": True, "text": "GARE11 · 204 cotas"})
powers.Powers(pb).register(FakeRegistry())
check("registered in Coucou's toolset", len(registered) == 19 and all(t == "coucou" for t, _, _ in registered.values()))
check("the schema the model sees", registered["wealth_trade"][1]["parameters"]["type"] == "object")
out = registered["wealth_portfolio"][2]({})
check("the call goes to the app's engine, with time to read the sources",
      pb.asked[-1] == ("powers", {"tool": "wealth_portfolio", "input": {}}, powers.WAIT) and powers.WAIT > 60)
check("the engine's text to the model", out == "GARE11 · 204 cotas")
registered["life_english"][2]({"action": "log", "minutes": 15})
check("the arguments as given", pb.asked[-1][1] == {"tool": "life_english", "input": {"action": "log", "minutes": 15}})
refused = powers.Powers(PowersBridge({"ok": False, "text": "wealth_company needs the share's ticker"})).call("wealth_company", {})
check("a refusal as an error the model reads", json.loads(refused) == {"error": "wealth_company needs the share's ticker"})
away = powers.Powers(PowersBridge({"error": "The AXYON app is not listening right now."})).call("life_day", {})
check("the app away: its error", "not listening" in json.loads(away)["error"])

print("The host's server")
from axyon_hermes import host as host_module  # noqa: E402
import socket  # noqa: E402

looked_up = []
real_getfqdn = socket.getfqdn
socket.getfqdn = lambda *a: looked_up.append(a) or "x"
try:
    server = host_module.serve(host_module.Host(tempfile.mkdtemp(), "t" * 32, ""), 0)
    port = server.server_address[1]
    check("it listens on 127.0.0.1 at once, with no name looked up (that can hang inside iOS)",
          not looked_up and server.server_name == "127.0.0.1" and port > 0)
    with socket.create_connection(("127.0.0.1", port), timeout=2):
        check("and takes a connection", True)
    server.server_close()
finally:
    socket.getfqdn = real_getfqdn

print("The bridge")
b = bridge.Bridge()
t0 = time.time()
check("no app listening: an error, after a moment only", "not listening" in b.ask("iphone", {}, wait=0.1)["error"]
      and time.time() - t0 < 1)
late = threading.Thread(target=lambda: (time.sleep(0.3), b.listening(+1)))
late.start()
asked = {}
asking = threading.Thread(target=lambda: asked.update(answer=b.ask("iphone", {"action": "status"}, wait=3)))
asking.start()
request = b.next_request(2)
check("an app that comes back a moment later still gets the request", request is not None
      and b.answer(request["id"], {"ok": True}))
asking.join()
late.join()
check("and its answer reaches the tool", asked.get("answer") == {"ok": True})
b.listening(-1)
b.listening(+1)
got = {}


def app():
    req = b.next_request(2)
    got["req"] = req
    b.answer(req["id"], {"done": req["request"]["x"]})


threading.Thread(target=app).start()
check("asked and answered", b.ask("iphone", {"x": 7}, wait=2) == {"done": 7})
check("an answer to nothing is refused", b.answer("b999", {}) is False)
t0 = time.time()
check("no answer in time: an error", "in time" in b.ask("iphone", {"x": 1}, wait=0.2)["error"] and time.time() - t0 < 1)
check("and its request is withdrawn", b.next_request(0.05) is None)
old_stream = {}


def closed_stream():
    try:
        b.next_request(2, gone=lambda: True)
    except ConnectionResetError:
        old_stream["ended"] = True


def new_stream():
    time.sleep(0.2)
    req = b.next_request(2, gone=lambda: False)
    b.answer(req["id"], {"by": "the new stream"})


threads = [threading.Thread(target=closed_stream), threading.Thread(target=new_stream)]
for t in threads:
    t.start()
check("a stream the app closed leaves the request to its new one",
      b.ask("iphone", {"x": 2}, wait=3) == {"by": "the new stream"} and old_stream.get("ended"))
for t in threads:
    t.join()

print("The files the user sends")
from axyon_hermes import attachments  # noqa: E402

home = tempfile.mkdtemp(prefix="axyon-inbox-")
box = attachments.inbox(home)
os.makedirs(box)
outside = os.path.join(home, "config.yaml")
with open(outside, "w") as f:
    f.write("model: secret")
receipt = os.path.join(box, "nota.pdf")
with open(receipt, "wb") as f:
    f.write(b"%PDF-1.7 not read here")
with open(receipt + ".txt.extracted", "w") as f:
    f.write("Nota fiscal 123\nTotal: R$ 89,90")
sheet = os.path.join(box, "gastos.csv")
with open(sheet, "w") as f:
    f.write("data,valor\n2026-10-01,12.5\n" + "x" * 70_000)
photo = os.path.join(box, "foto.jpg")
with open(photo, "wb") as f:
    f.write(b"\xff\xd8\xff\xe0 jpeg")
song = os.path.join(box, "musica.mp3")
with open(song, "wb") as f:
    f.write(b"ID3")
items = attachments.accept(home, [
    {"path": receipt, "name": "nota.pdf", "kind": "file", "text": receipt + ".txt.extracted"},
    {"path": sheet, "name": "gastos.csv"}, {"path": photo, "name": "foto.jpg", "kind": "image"},
    {"path": song, "name": "musica.mp3"}])
check("the app's files in the inbox are taken, a photo as one", [i["kind"] for i in items] == ["file", "file", "image", "file"])


def refused(raw):
    try:
        attachments.accept(home, raw)
    except ValueError:
        return True
    return False


check("a file outside the inbox is refused", refused([{"path": outside}]))
check("a path climbing out of it too", refused([{"path": os.path.join(box, "..", "config.yaml")}]))
check("a missing file too", refused([{"path": os.path.join(box, "nada.pdf")}]))
turn = attachments.compose("Quanto gastei?", items, native=False, describe=lambda p: f"[described {os.path.basename(p)}]")
check("a PDF's text, as the app read it, in the turn", "Nota fiscal 123" in turn and "nota.pdf" in turn)
check("a long document cut, with how to read on", "read the rest with iphone_files" in turn and len(turn) < 75_000)
check("a file with no text said so, with its path", "musica.mp3" in turn and "cannot be read on the iPhone" in turn)
check("a photo described for a brain that does not see, first", turn.startswith("[described foto.jpg]"))
check("the user's words last", turn.rstrip().endswith("Quanto gastei?"))
check("only documents: no description asked", "described" not in attachments.compose("", items[:1], native=False, describe=lambda p: "[described]"))
files = attachments.Files(home)
listed = files.call({"do": "list"})["files"]
check("the files listed, without the PDF's text beside it", {f["name"] for f in listed} == {"nota.pdf", "gastos.csv", "foto.jpg", "musica.mp3"})
check("a PDF read again through the app's text", "Total: R$ 89,90" in files.call({"do": "read", "path": receipt})["text"])
first = files.call({"do": "read", "path": sheet})
check("a long file read in parts", first["next_offset"] == 20_000 and "next_offset" not in files.call({"do": "read", "path": sheet, "offset": 60_000}))
check("a photo is for vision_analyze", "vision_analyze" in files.call({"do": "read", "path": photo})["error"])
check("Hermes cannot read outside the inbox", "No such file" in files.call({"do": "read", "path": outside})["error"])

print("Google, signed in on the iPhone")
import base64  # noqa: E402
import hashlib  # noqa: E402
import urllib.parse  # noqa: E402
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer  # noqa: E402

from axyon_hermes import google  # noqa: E402

seen = {"challenge": "", "sent": [], "drafts": [], "revoked": [], "auth": [], "writes": [], "events": []}
ACCESS = {"live": "at-1"}


class FakeGoogle(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _out(self, code, obj=None, raw=None):
        body = raw if raw is not None else json.dumps(obj or {}).encode()
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _in(self):
        data = self.rfile.read(int(self.headers.get("Content-Length") or 0))
        if self.headers.get("Content-Type", "").startswith("application/x-www-form-urlencoded"):
            return {k: v[0] for k, v in urllib.parse.parse_qs(data.decode()).items()}
        return json.loads(data or b"{}")

    def _authed(self):
        seen["auth"].append(self.headers.get("Authorization"))
        if self.headers.get("Authorization") != "Bearer " + ACCESS["live"]:
            self._out(401, {"error": {"message": "Invalid Credentials"}})
            return False
        return True

    def do_POST(self):
        path = urllib.parse.urlparse(self.path).path
        body = self._in()
        if path == "/token":
            if body["grant_type"] == "authorization_code":
                challenge = base64.urlsafe_b64encode(hashlib.sha256(body["code_verifier"].encode()).digest()).rstrip(b"=").decode()
                ok = (body["code"] == "good-code" and challenge == seen["challenge"] and body["client_secret"] == "s3cret"
                      and body["redirect_uri"].startswith("http://127.0.0.1:"))
                return self._out(200, {"access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600,
                                       "scope": " ".join(google.SERVICES["email"] + google.SERVICES["sheets"])}) if ok else \
                    self._out(400, {"error": "invalid_grant", "error_description": "Bad Request"})
            if body["grant_type"] == "refresh_token" and body["refresh_token"] == "rt-1":
                ACCESS["live"] = "at-2"
                return self._out(200, {"access_token": "at-2", "expires_in": 3600})
            return self._out(400, {"error": "invalid_grant"})
        if path == "/revoke":
            seen["revoked"].append(body.get("token"))
            return self._out(200, {})
        if not self._authed():
            return
        if path == "/gmail/messages/send":
            seen["sent"].append(body)
            return self._out(200, {"id": "sent-1"})
        if path == "/gmail/drafts":
            seen["drafts"].append(body)
            return self._out(200, {"id": "draft-1"})
        if path.startswith("/sheets/") and path.endswith(":append"):
            seen["writes"].append(("append", path, body))
            return self._out(200, {"updates": {"updatedRange": "Aportes!A8:C8", "updatedRows": 1}})
        if path == "/calendar/calendars/primary/events":
            seen["events"].append(body)
            return self._out(200, {"id": "ev-1", "htmlLink": "https://calendar.google.com/ev-1"})
        self._out(404, {})

    def do_PUT(self):
        body = self._in()
        if not self._authed():
            return
        seen["writes"].append(("write", urllib.parse.unquote(urllib.parse.urlparse(self.path).path), body))
        self._out(200, {"updatedRange": body["range"], "updatedCells": 2})

    def do_GET(self):
        url = urllib.parse.urlparse(self.path)
        path, query = url.path, urllib.parse.parse_qs(url.query)
        if not self._authed():
            return
        if path == "/gmail/profile":
            return self._out(200, {"emailAddress": "dono@example.com"})
        if path == "/gmail/messages":
            return self._out(200, {"messages": [{"id": "m1", "threadId": "t1"}]})
        if path == "/gmail/messages/m1":
            headers = [{"name": "From", "value": "Gestora <ri@fii.com.br>"}, {"name": "Subject", "value": "Relatório BTLG11"},
                       {"name": "Date", "value": "Tue, 28 Oct 2026 10:00:00 -0300"},
                       {"name": "Message-ID", "value": "<abc@fii.com.br>"}]
            if query.get("format") == ["full"]:
                text = base64.urlsafe_b64encode("Rendimento de R$ 0,78 por cota.".encode()).decode()
                return self._out(200, {"id": "m1", "threadId": "t1", "payload": {"headers": headers, "mimeType": "multipart/mixed", "parts": [
                    {"mimeType": "text/plain", "body": {"data": text}},
                    {"mimeType": "application/pdf", "filename": "relatorio.pdf", "body": {"size": 1200}}]}})
            return self._out(200, {"id": "m1", "threadId": "t1", "snippet": "Rendimento &amp; mais", "labelIds": ["UNREAD"],
                                   "payload": {"headers": headers}})
        if path.startswith("/sheets/") and "/values/" in path:
            return self._out(200, {"range": urllib.parse.unquote(path.split("/values/")[1]), "values": [["BTLG11", "101,50"]]})
        if path.startswith("/sheets/"):
            return self._out(200, {"properties": {"title": "Carteira"},
                                   "sheets": [{"properties": {"title": "AXYON", "gridProperties": {"rowCount": 100, "columnCount": 8}}}]})
        if path == "/drive/files":
            seen["drive_q"] = query.get("q", [""])[0]
            return self._out(200, {"files": [{"id": "doc1234567890", "name": "Plano 2027"}]})
        if path == "/drive/files/doc1234567890":
            return self._out(200, {"id": "doc1234567890", "name": "Plano 2027", "mimeType": "application/vnd.google-apps.document"})
        if path == "/drive/files/doc1234567890/export":
            return self._out(200, raw="Meta: 1000 cotas de BTLG11".encode())
        if path == "/drive/files/pdf1234567890" and query.get("alt") == ["media"]:
            return self._out(200, raw=b"%PDF-1.7 binary")
        if path == "/drive/files/pdf1234567890":
            return self._out(200, {"id": "pdf1234567890", "name": "informe.pdf", "mimeType": "application/pdf", "size": "15"})
        if path == "/calendar/calendars/primary/events":
            return self._out(200, {"items": [{"id": "e1", "summary": "Assembleia BTLG11", "start": {"dateTime": "2026-11-03T10:00:00-03:00"},
                                              "end": {"dateTime": "2026-11-03T11:00:00-03:00"}}]})
        self._out(404, {})


fake = ThreadingHTTPServer(("127.0.0.1", 0), FakeGoogle)
threading.Thread(target=fake.serve_forever, daemon=True).start()
root = f"http://127.0.0.1:{fake.server_address[1]}"


class Asked:
    """The app's side of the bridge, for the e-mail card and the PDF's text."""
    def __init__(self, send):
        self.send, self.asked = send, []

    def ask(self, kind, request, wait=None):
        self.asked.append((kind, request))
        if request.get("action") == "pdf_text":
            return {"text": "Informe de rendimentos 2026"}
        return {"send": self.send}


ghome = tempfile.mkdtemp(prefix="axyon-google-")
os.makedirs(os.path.join(ghome, "inbox"))
g = google.Google(ghome, bridge=Asked(False), port=lambda: 47830,
                  endpoints={k: root + "/" + k for k in ("auth", "token", "revoke", "gmail", "sheets", "drive", "calendar")})
first = g.status()
check("no client yet: the steps to make one, with Google Cloud's links", not first["connected"] and "console.cloud.google.com" in first["next"])
check("connecting without a client says the same", "Desktop app" in json.loads(g.account_tool({"do": "connect"}))["error"])
client_json = os.path.join(ghome, "inbox", "client_secret.json")
with open(client_json, "w") as f:
    json.dump({"installed": {"client_id": "123-abc.apps.googleusercontent.com", "client_secret": "s3cret"}}, f)
check("a JSON outside the chat's files is refused", "error" in json.loads(g.account_tool({"do": "client", "path": "/etc/passwd"})))
check("a client ID that is not Google's is refused", "error" in json.loads(g.account_tool({"do": "client", "client_id": "x", "client_secret": "y"})))
check("the client from the JSON the user sent", json.loads(g.account_tool({"do": "client", "path": client_json}))["ok"] and g.client()["client_secret"] == "s3cret")
link = json.loads(g.account_tool({"do": "connect", "services": ["email", "sheets"]}))
params = urllib.parse.parse_qs(urllib.parse.urlparse(link["url"]).query)
seen["challenge"] = params["code_challenge"][0]
check("a link to Google's page, PKCE, offline, back to this host on 127.0.0.1",
      link["url"].startswith(root + "/auth?") and params["redirect_uri"] == ["http://127.0.0.1:47830/"]
      and params["code_challenge_method"] == ["S256"] and params["access_type"] == ["offline"]
      and "gmail.send" in params["scope"][0] and "drive" not in params["scope"][0])
code, page = g.callback("state=someone-else&code=good-code")
check("a browser coming back with another state is refused", code == 400 and not g.status()["connected"])
code, page = g.callback(urllib.parse.urlencode({"state": params["state"][0], "code": "good-code", "scope": "x"}))
done = g.status()
check("Google's browser coming back finishes it alone", code == 200 and "connected to Google" in page and done["connected"])
check("the account and services known", done["account"] == "dono@example.com" and set(done["services"]) == {"email", "sheets"})
check("the token kept where Hermes' skill keeps it, for the user only",
      json.load(open(os.path.join(ghome, "google_token.json")))["refresh_token"] == "rt-1"
      and oct(os.stat(os.path.join(ghome, "google_token.json")).st_mode & 0o777) == "0o600")
again = json.loads(g.account_tool({"do": "connect"}))
again_params = urllib.parse.parse_qs(urllib.parse.urlparse(again["url"]).query)
seen["challenge"] = again_params["code_challenge"][0]
check("an address from an older link is refused",
      "older link" in json.loads(g.account_tool({"do": "finish", "pasted": f"http://127.0.0.1:47830/?state={params['state'][0]}&code=good-code"}))["error"])
pasted = json.loads(g.account_tool({"do": "finish", "pasted": f"http://127.0.0.1:47830/?state={again_params['state'][0]}&code=good-code&scope=x"}))
check("the address pasted in the chat finishes it too", pasted.get("ok") and pasted["connected"])
found = json.loads(g.gmail_tool({"do": "search", "query": "from:ri@fii.com.br newer_than:7d"}))["messages"]
check("Gmail searched, with who, what and the unread", found[0]["subject"] == "Relatório BTLG11" and found[0]["unread"] and found[0]["snippet"] == "Rendimento & mais")
mail = json.loads(g.gmail_tool({"do": "read", "id": "m1"}))
check("an e-mail read with its text and attachments", "R$ 0,78" in mail["body"] and mail["attachments"][0]["name"] == "relatorio.pdf")
check("a draft saved in Gmail", json.loads(g.gmail_tool({"do": "draft", "to": "a@b.com", "subject": "Oi", "body": "Teste"}))["draft"] == "draft-1")
refused = json.loads(g.gmail_tool({"do": "send", "to": "a@b.com", "subject": "Oi", "body": "Teste"}))
check("no tap on Send in AXYON: nothing sent", refused["sent"] is False and not seen["sent"] and g.bridge.asked[-1][1]["action"] == "mail")
g.bridge = Asked(True)
sent = json.loads(g.gmail_tool({"do": "send", "to": "a@b.com", "subject": "Oi", "body": "Teste"}))
raw = base64.urlsafe_b64decode(seen["sent"][-1]["raw"] + "===").decode()
check("the user's tap sends it, as shown", sent["sent"] and "To: a@b.com" in raw and "Subject: Oi" in raw
      and g.bridge.asked[-1][1]["subject"] == "Oi")
json.loads(g.gmail_tool({"do": "reply", "id": "m1", "body": "Obrigado"}))
import email  # noqa: E402
import email.policy  # noqa: E402

reply = email.message_from_bytes(base64.urlsafe_b64decode(seen["sent"][-1]["raw"] + "==="), policy=email.policy.default)
check("a reply in its thread, to who wrote", seen["sent"][-1]["threadId"] == "t1" and reply["In-Reply-To"] == "<abc@fii.com.br>"
      and reply["Subject"] == "Re: Relatório BTLG11" and "ri@fii.com.br" in reply["To"])
sheet_link = "https://docs.google.com/spreadsheets/d/1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz/edit#gid=0"
check("a spreadsheet's id from its link", google.spreadsheet_id(sheet_link) == "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz")
check("its tabs", json.loads(g.sheets_tool({"do": "tabs", "spreadsheet": sheet_link}))["tabs"][0]["title"] == "AXYON")
check("cells read", json.loads(g.sheets_tool({"do": "read", "spreadsheet": sheet_link, "range": "AXYON!A1:B2"}))["values"] == [["BTLG11", "101,50"]])
json.loads(g.sheets_tool({"do": "write", "spreadsheet": sheet_link, "range": "AXYON!B2", "values": [["102,10"]]}))
check("cells written as the user would type them", seen["writes"][-1][0] == "write" and "AXYON!B2" in seen["writes"][-1][1]
      and seen["writes"][-1][2]["values"] == [["102,10"]])
check("a row appended", json.loads(g.sheets_tool({"do": "append", "spreadsheet": sheet_link, "range": "Aportes!A:C",
                                                  "values": [["2026-10-29", "BTLG11", 10]]}))["rows"] == 1)
check("Drive searched by a file's name", json.loads(g.drive_tool({"do": "search", "query": "Plano"}))["files"][0]["name"] == "Plano 2027"
      and seen["drive_q"] == "name contains 'Plano' and trashed = false")
check("a Doc read as text", "1000 cotas" in json.loads(g.drive_tool({"do": "read", "id": "https://docs.google.com/document/d/doc1234567890/edit"}))["text"])
pdf = json.loads(g.drive_tool({"do": "read", "id": "pdf1234567890"}))
check("a PDF from Drive read by the iPhone, kept for iphone_files", pdf["text"] == "Informe de rendimentos 2026"
      and os.path.exists(pdf["path"] + ".txt.extracted") and pdf["path"].startswith(os.path.join(ghome, "inbox")))
check("the calendar's events", json.loads(g.calendar_tool({"do": "list"}))["events"][0]["title"] == "Assembleia BTLG11")
json.loads(g.calendar_tool({"do": "add", "title": "Aporte", "start": "2026-11-05T09:00"}))
check("an event added, an hour long by default, in the iPhone's time zone",
      seen["events"][-1]["summary"] == "Aporte" and seen["events"][-1]["start"]["dateTime"].startswith("2026-11-05T09:00:00")
      and seen["events"][-1]["end"]["dateTime"].startswith("2026-11-05T10:00:00"))
ACCESS["live"] = "at-rotated"
token = json.load(open(os.path.join(ghome, "google_token.json")))
token["token"], token["expiry"] = "at-1", "2020-01-01T00:00:00Z"
json.dump(token, open(os.path.join(ghome, "google_token.json"), "w"))
check("an expired sign-in refreshed by itself", json.loads(g.gmail_tool({"do": "search", "query": ""}))["messages"] and seen["auth"][-1] == "Bearer at-2")
check("disconnected: revoked at Google and forgotten", json.loads(g.account_tool({"do": "disconnect"}))["ok"]
      and seen["revoked"] == ["rt-1"] and not os.path.exists(os.path.join(ghome, "google_token.json")))
check("then every tool says to connect", "not connected" in json.loads(g.gmail_tool({"do": "search", "query": ""}))["error"])
fake.shutdown()

print("The investing brain")
from axyon_hermes import invest  # noqa: E402


class Engine:
    """The app's engine on the other side of the bridge: the brief, then what the brain said, kept."""

    def __init__(self, fail=None):
        self.asked = []
        self.fail = fail

    def ask(self, kind, request, wait=None):
        self.asked.append((kind, request))
        tool, data = request["tool"], request["input"]
        if self.fail == tool + ":" + data["do"]:
            return {"ok": False, "text": "the committee needs the asset's ticker"}
        if data["do"] == "brief":
            prompt = "Run AXYON's investment committee on GGRC11" if tool == "invest_committee" else "You are AXYON's reader"
            return {"ok": True, "data": {"prompt": prompt, "ticker": "GGRC11", "documents": ["fnet:904"]}}
        return {"ok": True, "data": {"kept": data["answer"], "ticker": data.get("ticker"), "reason": data.get("reason")}}


prompts = []
engine = Engine()
brain = invest.Invest(engine, ask=lambda prompt: (prompts.append(prompt), '{"minds": []}')[1])
verdict = brain.committee({"proposal": "comprar mais 100 cotas de GGRC11", "reason": "asked", "language": "Portuguese"})
check("the committee's brief asked of the engine, then the brain, then kept by the engine",
      [r[1]["input"]["do"] for r in engine.asked] == ["brief", "verdict"] and prompts == ["Run AXYON's investment committee on GGRC11"])
check("the verdict is the engine's, for the ticker the engine found", verdict == {"kept": '{"minds": []}', "ticker": "GGRC11", "reason": "asked"})
check("each through the superpowers' bridge", all(k == "powers" and r["tool"] == "invest_committee" for k, r in engine.asked))
digest = brain.digest({"key": "fnet:904", "language": "Portuguese"})
check("a document's digest the same way", engine.asked[-1][1] == {"tool": "invest_docs", "input": {"do": "digest", "key": "fnet:904", "answer": '{"minds": []}'}}
      and digest["kept"] == '{"minds": []}' and prompts[-1] == "You are AXYON's reader")
try:
    invest.Invest(Engine(fail="invest_committee:brief"), ask=lambda p: "{}").committee({"proposal": "manter"})
    said = ""
except invest.InvestError as exc:
    said = str(exc)
check("the engine's refusal comes back in words", said == "the committee needs the asset's ticker")
for bad in ({}, {"proposal": "  "}):
    try:
        brain.committee(bad)
        check("no proposal: refused", False)
    except ValueError:
        check("no proposal: refused", True)

print("All unit tests passed." if not failures else f"{failures} failed.")
sys.exit(1 if failures else 0)
