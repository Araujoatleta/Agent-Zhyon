"""The host's own pieces, without Hermes: Google Finance's quote, the portfolio's figures, the web's results, the
iPhone tools' requests and the bridge. `python3 test_units.py`"""
import json
import os
import sys
import threading
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "app"))

from axyon_hermes import bridge, coucou, iphone, web  # noqa: E402

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
check("twelve iPhone tools, in Coucou's toolset", len(registered) == 12 and all(t == "coucou" for t, _, _ in registered.values()))
out = registered["iphone_notify"][2]({"title": "Água", "in_seconds": 60, "body": None})
check("the action and its arguments go to the app", fake.asked[-1] == ("iphone", {"action": "notify", "title": "Água", "inSeconds": 60}))
check("the app's answer to the model", json.loads(out) == {"ok": True})
check("every schema says its name", all(s["name"] == n for n, (_, s, _) in registered.items()))

print("The bridge")
b = bridge.Bridge()
check("no app listening: an error, at once", "not listening" in b.ask("iphone", {}, wait=0.1)["error"])
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

print("All unit tests passed." if not failures else f"{failures} failed.")
sys.exit(1 if failures else 0)
