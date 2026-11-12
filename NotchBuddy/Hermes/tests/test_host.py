"""The host as the app uses it, end to end, against a fake model (fake_model.py): settings, a run with the
superpowers' portfolio tool (the app's side of the bridge answering for the engine), the session remembered, the
bridge, and the token. Needs Hermes' checkout and a Python 3.14 with its dependencies:
`HERMES_SRC=… PYTHON=… python3 test_host.py`. With APP_PACKAGES=… (the app's own list installed in a folder, as for
test_embedded.py) and a PYTHON without them, only what the iPhone carries is found."""
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
failures = 0


def check(label, ok):
    global failures
    print(("  ✓ " if ok else "  ✗ ") + label)
    if not ok:
        failures += 1


def tiny_png():
    """A real 2×2 PNG, as a photo the user sends."""
    import struct
    import zlib

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    rows = b"".join(b"\x00" + b"\xff\x80\x00" * 2 for _ in range(2))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def main():
    hermes_src = os.environ["HERMES_SRC"]
    python = os.environ.get("PYTHON", sys.executable)
    model_port, host_port = free_port(), free_port()
    token = secrets.token_hex(16)
    work = tempfile.mkdtemp(prefix="axyon-host-")
    model = subprocess.Popen([sys.executable, os.path.join(HERE, "fake_model.py"), str(model_port)],
                             stderr=open(os.path.join(work, "model.log"), "w"))
    portfolio = os.path.join(work, "portfolio.json")
    with open(portfolio, "w") as f:
        json.dump({"type": "portfolio", "receivedAt": time.time(), "cash": 100.0, "source": "Google Sheets",
                   "positions": [{"ticker": "GARE11", "class": "fii", "quantity": 204, "averagePrice": 8.95},
                                 {"ticker": "RDB Nubank", "class": "renda_fixa", "quantity": 1, "averagePrice": 100,
                                  "currentValue": 104.5}],
                   "ceilings": {"GARE11": 8.0, "KNSC11": 9.0},
                   "prices": {"GARE11": {"price": 8.31, "close": 8.31, "closeDay": "2026-10-27"}}}, f)
    packages = [os.environ["APP_PACKAGES"]] if os.environ.get("APP_PACKAGES") else []
    # The stand-in model sees, as the user's brain does (it is in no catalog of models that see: Hermes would turn
    # photos into descriptions for it).
    os.makedirs(os.path.join(work, "hermes"))
    with open(os.path.join(work, "hermes", "config.yaml"), "w") as f:
        f.write("model:\n  provider: openai-codex\n  supports_vision: true\ntools:\n  tool_search:\n    enabled: \"off\"\n")
    env = dict(os.environ, AXYON_HERMES_HOME=os.path.join(work, "hermes"), AXYON_HOST_PORT=str(host_port),
               AXYON_HOST_TOKEN=token, AXYON_PORTFOLIO=portfolio,
               PYTHONPATH=os.pathsep.join([os.path.join(ROOT, "shims"), os.path.join(ROOT, "app"), hermes_src]
                                          + packages))
    host = subprocess.Popen([python, os.path.join(HERE, "ios_like.py")], env=env,
                            stdout=open(os.path.join(work, "host.log"), "w"), stderr=subprocess.STDOUT)
    base = f"http://127.0.0.1:{host_port}"

    def call(method, path, body=None, auth=True, headers=None):
        req = urllib.request.Request(base + path, method=method,
                                     data=None if body is None else json.dumps(body).encode())
        if auth:
            req.add_header("Authorization", "Bearer " + token)
        for k, v in (headers or {}).items():
            req.add_header(k, v)
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return r.status, json.loads(r.read() or b"{}")
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def events(run_id):
        req = urllib.request.Request(f"{base}/v1/runs/{run_id}/events")
        req.add_header("Authorization", "Bearer " + token)
        out = []
        with urllib.request.urlopen(req, timeout=120) as r:
            for raw in r:
                line = raw.decode().strip()
                if line.startswith("data: "):
                    out.append(json.loads(line[6:]))
        return out

    try:
        print("Starting")
        ready = False
        for _ in range(240):
            try:
                code, health = call("GET", "/health")
                if code == 200 and health.get("ready"):
                    ready = True
                    break
                if health.get("problem"):
                    break
            except (urllib.error.URLError, ConnectionError):
                pass
            time.sleep(0.25)
        check("Hermes ready behind the host", ready)
        if not ready:
            print(open(os.path.join(work, "host.log")).read()[-3000:])
            return
        check("python is 3.14", health.get("python", "").startswith("3.14"))
        check("ChatGPT's image maker set with ChatGPT as the brain",
              "image_gen:\n  provider: openai-codex" in open(os.path.join(work, "hermes", "config.yaml")).read())
        check("no token: refused", call("GET", "/health", auth=False)[0] == 401)
        check("a web page: refused", call("GET", "/health", headers={"Origin": "https://evil.example"})[0] == 401)

        print("The brain")
        code, s = call("POST", "/v1/settings", {"provider": "custom", "model": "fake-model",
                                               "baseUrl": f"http://127.0.0.1:{model_port}/v1",
                                               "keys": {"OPENAI_API_KEY": "fake-key"}})
        check("set to an OpenAI-compatible brain", code == 200 and s.get("provider") == "custom")
        check("its key known, not shown", s.get("keysSet") == {"OPENAI_API_KEY": True})
        config = open(os.path.join(work, "hermes", "config.yaml")).read()
        check("no key written to Hermes' config", "fake-key" not in config)
        check("unknown brain refused", call("POST", "/v1/settings", {"provider": "nope"})[0] == 400)

        print("The iPhone's own home")
        code, house = call("GET", "/v1/home")
        check("no device yet", code == 200 and house.get("devices") == [])
        code, refused = call("POST", "/v1/home/devices", {"driver": "samsung", "host": "8.8.8.8"})
        check("a device off the local network is refused, in words", code == 422
              and "local network" in refused.get("error", ""))
        code, scan = call("POST", "/v1/home/scan", {"address": "8.8.8.8"})
        check("no scan off the home's Wi-Fi", code == 200 and "Wi-Fi" in scan.get("error", ""))

        def app_answering(answer):
            """The app's side of the bridge: the first request it gets, answered with `answer`."""
            got = []

            def side():
                req = urllib.request.Request(base + "/v1/bridge")
                req.add_header("Authorization", "Bearer " + token)
                with urllib.request.urlopen(req, timeout=60) as resp:
                    for raw in resp:
                        line = raw.decode().strip()
                        if line.startswith("data: "):
                            ask = json.loads(line[6:])
                            got.append(ask)
                            call("POST", f"/v1/bridge/{ask['id']}", {"answer": answer})
                            return

            t = threading.Thread(target=side, daemon=True)
            t.start()
            time.sleep(0.5)
            return t, got

        def app_answering_each(answers):
            """The app's side of the bridge: each request it gets answered with the next of `answers`."""
            got = []

            def side():
                req = urllib.request.Request(base + "/v1/bridge")
                req.add_header("Authorization", "Bearer " + token)
                with urllib.request.urlopen(req, timeout=120) as resp:
                    for raw in resp:
                        line = raw.decode().strip()
                        if line.startswith("data: "):
                            ask = json.loads(line[6:])
                            got.append(ask)
                            call("POST", f"/v1/bridge/{ask['id']}", {"answer": answers[len(got) - 1]})
                            if len(got) == len(answers):
                                return

            t = threading.Thread(target=side, daemon=True)
            t.start()
            time.sleep(0.5)
            return t, got

        print("The committee and the reader")
        brief = {"ok": True, "data": {"prompt": "Você é o comitê de investimentos do AXYON. Proposta: manter GGRC11",
                                      "ticker": "GGRC11", "documents": ["fnet:904"], "facts": [{"id": "F1", "text": "Preço"}],
                                      "docs": [{"id": "D1", "key": "fnet:904"}]}}
        review = {"ok": True, "data": {"prompt": "Você é o auditor do comitê de investimentos do AXYON. GGRC11", "findings": []}}
        app5, asked5 = app_answering_each([brief, review, {"ok": True, "data": {"score": {"for": 2, "against": 2}, "vetoedBy": ["risk"]}}])
        code, verdict = call("POST", "/v1/invest/committee", {"proposal": "manter GGRC11 depois do relatório", "reason": "document",
                                                               "document": "fnet:904", "language": "en-US"})
        app5.join(timeout=30)
        check("the committee's verdict, as the engine kept it", code == 200 and verdict.get("vetoedBy") == ["risk"])
        requests = [a["request"] for a in asked5]
        check("the engine wrote the brief, checked the minds for the auditor, then kept both answers",
              [r["input"]["do"] for r in requests] == ["brief", "audit", "verdict"]
              and requests[0]["input"]["language"] == "Brazilian Portuguese"
              and requests[2]["input"]["ticker"] == "GGRC11" and requests[2]["input"]["reason"] == "document"
              and requests[2]["input"]["document"] == "fnet:904" and requests[2]["input"]["facts"] == [{"id": "F1", "text": "Preço"}])
        answer = requests[2]["input"]["answer"] if len(requests) > 2 else ""
        audit = requests[2]["input"].get("audit", "") if len(requests) > 2 else ""
        check("the brain's five minds and its audit reached the engine", '"veto": true' in answer and '"chair"' in answer
              and '"revisions"' in audit and "evitar_novo_aporte" in audit)
        log = open(os.path.join(work, "model.log")).read()
        check("both asked with no tools", "invest committee: tools=0" in log and "invest audit: tools=0" in log)
        app6, asked6 = app_answering_each([{"ok": True, "data": {"prompt": "You are AXYON's reader. GGRC11"}},
                                           {"ok": True, "data": {"key": "fnet:904", "headline": "Vacância subiu"}}])
        code, doc = call("POST", "/v1/invest/digest", {"key": "fnet:904"})
        app6.join(timeout=30)
        check("a document's digest, kept by the engine", code == 200 and doc.get("headline") == "Vacância subiu"
              and '"thesisRisk": true' in (asked6[1]["request"]["input"]["answer"] if len(asked6) > 1 else ""))
        app7, _ = app_answering_each([{"ok": False, "text": "No such document."}])
        code, refused = call("POST", "/v1/invest/digest", {"key": "fnet:1"})
        app7.join(timeout=30)
        check("the engine's refusal, in words", code == 422 and refused.get("error") == "No such document.")
        check("no key: refused", call("POST", "/v1/invest/digest", {})[0] == 400)

        print("A run with the superpowers' portfolio")
        app, asked_app = app_answering({"ok": True, "text": "GARE11: 204 shares at R$ 8,31 (B3's close of 27/10/2026)"})
        code, r = call("POST", "/v1/runs", {"session": "axyon-test", "message": "qual é a minha carteira?"})
        check("run started", code == 202 and r.get("run_id", "").startswith("run_"))
        evs = events(r["run_id"])
        app.join(timeout=10)
        kinds = [e["type"] for e in evs]
        check("the tool shows as it runs", "tool" in kinds and "tool_done" in kinds)
        check("its name is the computer's", any(e.get("name") == "wealth_portfolio" for e in evs if e["type"] == "tool"))
        check("the app's engine was asked", bool(asked_app) and asked_app[0]["kind"] == "powers"
              and asked_app[0]["request"] == {"tool": "wealth_portfolio", "input": {}})
        done = [e for e in evs if e["type"] == "done"]
        check("it ends with the answer", bool(done) and "Feito:" in done[0]["text"])
        check("from the engine's figures", bool(done) and "GARE11" in done[0]["text"])
        check("the answer streamed", any(e["type"] == "delta" for e in evs))
        seen = open(os.path.join(work, "model.log")).read()
        check("the model has every superpower but the content kit", "life_gym" in seen and "wealth_committee" in seen
              and "life_content" not in seen and "coucou_portfolio" not in seen)

        print("The session is remembered")
        code, r2 = call("POST", "/v1/runs", {"session": "axyon-test", "message": "obrigado"})
        evs2 = events(r2["run_id"])
        check("second answer", any(e["type"] == "done" for e in evs2))
        log = open(os.path.join(work, "model.log")).read()
        check("the model saw the first exchange", "msgs=6" in log or "msgs=5" in log or "msgs=7" in log)

        print("The bridge")
        t, got = app_answering({"devices": [{"id": "tv1", "name": "TV"}]})
        code, r3 = call("POST", "/v1/runs", {"session": "axyon-test-2", "message": "liga a casa"})
        evs3 = events(r3["run_id"])
        t.join(timeout=10)
        check("Hermes asked the app", bool(got) and got[0]["kind"] == "home")
        check("and the answer reached the model", any("tv1" in (e.get("text") or "") for e in evs3 if e["type"] == "done"))

        print("Files the user sends")
        inbox = os.path.join(work, "hermes", "inbox")
        os.makedirs(inbox, exist_ok=True)
        statement = os.path.join(inbox, "extrato.txt")
        with open(statement, "w") as f:
            f.write("Extrato de outubro: aporte de R$ 500 em BTLG11")
        photo = os.path.join(inbox, "print.png")
        with open(photo, "wb") as f:
            f.write(tiny_png())
        code, r5 = call("POST", "/v1/runs", {"session": "axyon-files", "message": "O que é isso?", "attachments": [
            {"path": statement, "name": "extrato.txt"}, {"path": photo, "name": "print.png", "kind": "image"}]})
        done5 = [e for e in events(r5["run_id"]) if e["type"] == "done"]
        said5 = done5[0]["text"] if done5 else ""
        check("the photo reached the model as an image", "Vi 1 imagem" in said5)
        if "Vi 1 imagem" not in said5:
            print("   ", said5[:300], [l[:160] for l in open(os.path.join(work, "model.log")).read().splitlines()[-3:]])
        check("the document's text in the message", "1 documento(s): Extrato de outubro" in said5)
        check("a file outside Hermes' inbox is refused", call("POST", "/v1/runs", {
            "session": "axyon-files", "message": "x", "attachments": [{"path": os.path.join(work, "hermes", "config.yaml")}]})[0] == 400)
        check("a photo alone is a message", call("POST", "/v1/runs", {
            "session": "axyon-files-2", "message": "", "attachments": [{"path": photo, "kind": "image"}]})[0] == 202)

        print("Google")
        code, account = call("GET", "/v1/google")
        check("not connected yet, with how to", code == 200 and account["connected"] is False
              and "console.cloud.google.com" in account.get("next", ""))
        seen = open(os.path.join(work, "model.log")).read()
        check("the model has Gmail, Sheets, Drive, Calendar and the account", all(
            t in seen for t in ("'gmail'", "'google_sheets'", "'google_drive'", "'google_calendar'", "'google_account'")))
        check("and every tool iOS allows: files sent, alerts, cron, delegation", all(
            t in seen for t in ("'iphone_files'", "'iphone_alerts'", "'cronjob_manage'", "'delegate_task'")))
        check("images made only with a brain that has an image maker", "'image_generate'" not in seen)
        check("but none iOS forbids", not any(t in seen for t in ("'terminal'", "'execute_code'", "'browser_navigate'", "'computer_use'")))
        try:
            urllib.request.urlopen(f"{base}/?state=not-waiting&code=x", timeout=10)
            stranger = 200
        except urllib.error.HTTPError as e:
            stranger = e.code
        check("Google's page coming back with another sign-in's state is refused", stranger == 400)
        code, made = call("POST", "/v1/google/connect", {})
        check("no client yet: Settings is told how to make one", code == 422 and "Desktop app" in made["error"])

        print("Scheduled tasks")
        app4, asked4 = app_answering({"ok": True})
        code, r6 = call("POST", "/v1/runs", {"session": "axyon-cron", "message": "agenda a cotação do BTLG11 a cada 5 minutos"})
        evs6 = events(r6["run_id"])
        check("made with Hermes' own cronjob tool", any(e.get("name") == "cronjob_manage" for e in evs6 if e["type"] == "tool"))
        code, listed = call("GET", "/v1/cron")
        jobs = listed.get("jobs") or []
        check("listed for Settings", code == 200 and jobs and jobs[0]["name"] == "Cotação BTLG11" and jobs[0]["enabled"])
        stored = os.path.join(work, "hermes", "cron", "jobs.json")
        with open(stored) as f:
            data = json.load(f)
        data["jobs"][0]["next_run_at"] = time.strftime("%Y-%m-%dT%H:%M:%S+00:00", time.gmtime(time.time() - 30))
        with open(stored, "w") as f:
            json.dump(data, f)
        code, ticked = call("POST", "/v1/cron/tick", {})
        app4.join(timeout=60)
        check("the due task ran at the app's tick", code == 200)
        check("its answer came to the iPhone as a notification", bool(asked4) and asked4[0]["kind"] == "iphone"
              and asked4[0]["request"]["action"] == "notify" and asked4[0]["request"]["title"] == "Cotação BTLG11")
        check("and Hermes counts it done", (call("GET", "/v1/cron")[1]["jobs"][0].get("status") or "") == "ok")
    finally:
        host.terminate()
        model.terminate()
        if failures:
            print(open(os.path.join(work, "host.log")).read()[-2000:])
    print("All host tests passed." if not failures else f"{failures} failed.")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
