"""The host as the app uses it, end to end, against a fake model (fake_model.py): settings, a run with the
superpowers' portfolio tool (the app's side of the bridge answering for the engine), the session remembered, the
bridge, and the token. Needs Hermes' checkout and a Python 3.14 with its dependencies:
`HERMES_SRC=… PYTHON=… python3 test_host.py`."""
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
    env = dict(os.environ, AXYON_HERMES_HOME=os.path.join(work, "hermes"), AXYON_HOST_PORT=str(host_port),
               AXYON_HOST_TOKEN=token, AXYON_PORTFOLIO=portfolio,
               PYTHONPATH=os.pathsep.join([os.path.join(ROOT, "shims"), os.path.join(ROOT, "app"), hermes_src]))
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
    finally:
        host.terminate()
        model.terminate()
        if failures:
            print(open(os.path.join(work, "host.log")).read()[-2000:])
    print("All host tests passed." if not failures else f"{failures} failed.")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
