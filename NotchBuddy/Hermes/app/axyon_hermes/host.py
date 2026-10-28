"""Hermes inside AXYON on the iPhone: its API for the app, on 127.0.0.1 only.

The app starts this in its embedded Python with, in the environment:
  AXYON_HERMES_HOME   the folder Hermes keeps everything in (config.yaml, auth.json, memories, skills, sessions)
  AXYON_HOST_PORT     the port to listen on (127.0.0.1 only)
  AXYON_HOST_TOKEN    the bearer token every request must carry (made by the app for this launch)
  AXYON_PORTFOLIO     the file where the app keeps the portfolio the computer gave it
  AXYON_POWERS        the folder of the superpowers' own files (the app's engine writes them; the wallet reads them)

and talks to it with:
  GET  /health                       ready or not, the brain, and why not when it is not
  POST /v1/runs {session, message}   answers one message in a session of the app's (Hermes remembers it)
  GET  /v1/runs/<id>/events          the answer as it is written: `delta`, `tool`, `tool_done`, then `done` or
                                     `error` (server-sent events, a keepalive every 10 s)
  POST /v1/runs/<id>/stop            stops it
  GET  /v1/settings                  the brain: provider, model, and whether it can sign in
  POST /v1/settings {provider, model, baseUrl, keys}
                                     the brain; keys are kept in memory only (the app keeps them in the Keychain)
  POST /v1/auth/chatgpt/start        ChatGPT sign-in, as `hermes auth add openai-codex` does: a code for the user
  GET  /v1/auth/chatgpt              where that sign-in is
  GET  /v1/bridge                    what Hermes asks of the app (server-sent), answered with
  POST /v1/bridge/<id>               the app's answer

Nothing else is served, no request without the token or from a web page (an Origin header) is answered.
"""

from __future__ import annotations

import hmac
import json
import logging
import os
import select
import socket
import sys
import threading
import time
import traceback
import urllib.parse
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Optional

from . import PROTOCOL, ios

log = logging.getLogger("axyon.host")

KEEPALIVE = 10.0
MAX_BODY = 256 * 1024
MAX_MESSAGE = 32_000
RUNS_KEPT = 20

# The brains the app offers, with the variable each one's key goes in. ChatGPT (the user's subscription) has no
# key: Hermes signs in to it itself and keeps the sign-in in its own auth.json.
PROVIDERS = {
    "openai-codex": {"name": "ChatGPT", "keys": []},
    "openrouter": {"name": "OpenRouter", "keys": ["OPENROUTER_API_KEY"]},
    "gemini": {"name": "Google Gemini", "keys": ["GEMINI_API_KEY"]},
    "anthropic": {"name": "Anthropic", "keys": ["ANTHROPIC_API_KEY"]},
    "custom": {"name": "OpenAI-compatible", "keys": ["OPENAI_API_KEY"]},
}
KEY_NAMES = {k for p in PROVIDERS.values() for k in p["keys"]}


SOUL_MARK = "<!-- AXYON on the iPhone wrote this file -->"
SOUL = """# Personality
You are AXYON, the user's personal assistant. You live inside their iPhone: Hermes is your engine and the iPhone is
your body. You act through it: notifications and reminders, the calendar, contacts, apps and links, their Shortcuts,
the devices of their home, the web, and your superpowers: their investments (the portfolio against B3's and the CVM's
official data, funds, ceilings, taxes, the decision journal) and their studies, English and training.

## Style
- Direct and warm. Short paragraphs, lists when they help, no filler.
- Answer in the language the user writes in (usually Brazilian Portuguese).
- Do what you can with your tools before answering; say what you did, briefly.
- A message, an e-mail or a call is opened ready for the user, who sends it.

## What to avoid
- Restating the request, and claiming you did something you did not do.

""" + SOUL_MARK + "\n"


class Run:
    def __init__(self, session: str, message: str):
        self.id = "run_" + uuid.uuid4().hex[:16]
        self.session = session
        self.message = message
        self.events: list[dict[str, Any]] = []
        self.cond = threading.Condition()
        self.finished = False
        self.agent = None
        self.started = time.time()

    def put(self, kind: str, **fields: Any) -> None:
        with self.cond:
            if self.finished:
                return
            event = {"seq": len(self.events), "type": kind, **fields}
            self.events.append(event)
            if kind in ("done", "error"):
                self.finished = True
            self.cond.notify_all()

    def after(self, seq: int, timeout: float) -> tuple[list[dict[str, Any]], bool]:
        with self.cond:
            self.cond.wait_for(lambda: len(self.events) > seq or self.finished, timeout=timeout)
            return self.events[seq:], self.finished


class Host:
    def __init__(self, home: str, token: str, portfolio_file: str):
        self.home = home
        self.token = token
        self.portfolio_file = portfolio_file
        self.runs: dict[str, Run] = {}
        self.lock = threading.Lock()
        self.ready = False
        self.problem: Optional[str] = None
        self.version = ""
        self.db = None
        self.auth: dict[str, Any] = {"state": "idle"}
        self.coucou = None
        self.house = None  # the iPhone's own home (home.py)
        from .bridge import Bridge

        self.bridge = Bridge()

    # ── Start ────────────────────────────────────────────────────────────────

    def start(self) -> None:
        """Imports Hermes (a few seconds on an iPhone) and registers Coucou's tools; the server answers meanwhile."""
        import faulthandler

        # An import that hangs says where, on the app's own output.
        faulthandler.dump_traceback_later(150, file=2)
        try:
            self._seed_home()
            t0 = time.time()
            import run_agent  # noqa: F401  (Hermes' agent, and with it its tools)
            from tools.registry import registry

            from .coucou import Coucou
            from .home import Home
            from .iphone import IPhone
            from .powers import Powers
            from .web import Web

            self.coucou = Coucou(self.portfolio_file, bridge=self.bridge, powers_dir=os.environ.get("AXYON_POWERS", ""))
            self.coucou.register(registry)
            self.house = Home(self.home, bridge=self.bridge)
            self.house.register(registry)
            IPhone(self.bridge).register(registry)
            Powers(self.bridge).register(registry)
            Web().register(registry)
            model = self._model_config()
            if model.get("provider") == "custom" and model.get("base_url"):
                # The address the user typed for their OpenAI-compatible brain: its key may go there.
                os.environ.setdefault("OPENAI_BASE_URL", str(model["base_url"]))
            try:
                from hermes_state_registry import acquire

                self.db = acquire()
            except Exception:
                log.exception("Hermes' session store did not open: conversations are not kept")
            try:
                from hermes_cli import __version__ as version
            except Exception:
                version = ""
            self.version = str(version or "")
            self.ready = True
            log.info("Hermes ready in %.1fs", time.time() - t0)
            os.write(2, f"AXYON: Hermes ready in {time.time() - t0:.1f}s\n".encode())
        except BaseException as exc:  # a module's SystemExit too: the app must hear why
            self.problem = f"Hermes did not start: {exc!r}"
            log.error("%s\n%s", self.problem, traceback.format_exc())
            os.write(2, f"AXYON: {self.problem}\n{traceback.format_exc()}".encode("utf-8", "replace"))
        finally:
            faulthandler.cancel_dump_traceback_later()

    def _seed_home(self) -> None:
        """A first launch: the config (tools straight in front of the model, no tool search) and AXYON's persona."""
        path = os.path.join(self.home, "config.yaml")
        if not os.path.exists(path):
            with open(path, "w", encoding="utf-8") as f:
                f.write(
                    "# Hermes inside AXYON on the iPhone. The app writes the model section from its settings.\n"
                    "model:\n  provider: openai-codex\n"
                    "tools:\n  tool_search:\n    enabled: \"off\"\n")
        soul = os.path.join(self.home, "SOUL.md")
        if not os.path.exists(soul) or SOUL_MARK in open(soul, encoding="utf-8").read():
            with open(soul, "w", encoding="utf-8") as f:
                f.write(SOUL)

    # ── The brain ────────────────────────────────────────────────────────────

    def settings(self) -> dict[str, Any]:
        model = self._model_config()
        provider = model.get("provider") or "openai-codex"
        signed_in = self._chatgpt_signed_in() if provider == "openai-codex" else None
        keys = {k: bool(os.environ.get(k)) for k in PROVIDERS.get(provider, {}).get("keys", [])}
        return {"provider": provider, "model": model.get("default") or model.get("model") or "",
                "baseUrl": model.get("base_url") or "", "providers": PROVIDERS, "keysSet": keys,
                "chatgptSignedIn": signed_in}

    def _model_config(self) -> dict[str, Any]:
        try:
            from hermes_cli.config import load_config

            return dict(load_config().get("model") or {})
        except Exception:
            return {}

    def set_settings(self, body: dict[str, Any]) -> dict[str, Any]:
        address = str(body.get("openaiBaseUrl") or "").strip()
        if address.startswith(("https://", "http://")):
            os.environ["OPENAI_BASE_URL"] = address
        keys = body.get("keys") or {}
        for name, value in keys.items():
            if name in KEY_NAMES and isinstance(value, str):
                if value.strip():
                    os.environ[name] = value.strip()
                else:
                    os.environ.pop(name, None)
        provider = body.get("provider")
        if provider is not None:
            if provider not in PROVIDERS:
                raise ValueError(f"unknown provider {provider!r}")
            from hermes_cli.config import load_config, save_config

            config = load_config()
            model = dict(config.get("model") or {})
            model["provider"] = provider
            if body.get("model") is not None:
                model["default"] = str(body["model"]).strip()
                model.pop("model", None)
            base = str(body.get("baseUrl") or "").strip()
            if provider == "custom":
                if not base.startswith(("https://", "http://")):
                    raise ValueError("an OpenAI-compatible brain needs its address (https://…/v1)")
                model["base_url"] = base
                # The key in OPENAI_API_KEY is sent only to this address (Hermes checks it).
                os.environ["OPENAI_BASE_URL"] = base
            else:
                model.pop("base_url", None)
            config["model"] = model
            save_config(config)
        return self.settings()

    def _chatgpt_signed_in(self) -> bool:
        try:
            from hermes_cli.auth import resolve_codex_runtime_credentials

            creds = resolve_codex_runtime_credentials(read_only=True)
            return bool(creds and (creds.get("api_key") or creds.get("access_token")))
        except Exception:
            return False

    def chatgpt_start(self) -> dict[str, Any]:
        """The device sign-in of `hermes auth add openai-codex`: a code the user types on OpenAI's page, then Hermes
        keeps its own sign-in (never the computer's: OpenAI's refresh tokens are single-use)."""
        with self.lock:
            if self.auth.get("state") == "waiting":
                return dict(self.auth)
            from hermes_cli import auth_codex as ac

            issuer = "https://auth.openai.com"
            device = ac._codex_request_device_code(issuer, ac.CODEX_OAUTH_CLIENT_ID)
            self.auth = {"state": "waiting", "code": device["user_code"], "url": f"{issuer}/codex/device",
                         "expiresIn": 15 * 60}

        def finish() -> None:
            try:
                code = ac._codex_poll_authorization_code(
                    issuer, device_auth_id=device["device_auth_id"], user_code=device["user_code"],
                    poll_interval=device["interval"])
                tokens = ac._codex_exchange_authorization_code(issuer, ac.CODEX_OAUTH_CLIENT_ID, code)
                from hermes_cli.auth import _save_codex_tokens, _update_config_for_provider

                _save_codex_tokens({"access_token": tokens.get("access_token", ""),
                                    "refresh_token": tokens.get("refresh_token", "")})
                _update_config_for_provider("openai-codex", ac._codex_base_url())
                self.auth = {"state": "done"}
            except BaseException as exc:  # a SystemExit from a cancelled poll included
                self.auth = {"state": "failed", "message": str(exc) or exc.__class__.__name__}

        threading.Thread(target=finish, name="chatgpt-sign-in", daemon=True).start()
        return dict(self.auth)

    # ── Runs ─────────────────────────────────────────────────────────────────

    def start_run(self, session: str, message: str) -> Run:
        run = Run(session, message)
        with self.lock:
            self.runs[run.id] = run
            for old in sorted(self.runs.values(), key=lambda r: r.started)[:-RUNS_KEPT]:
                if old.finished:
                    self.runs.pop(old.id, None)
        threading.Thread(target=self._run, args=(run,), name=f"hermes-{run.id}", daemon=True).start()
        return run

    def _run(self, run: Run) -> None:
        try:
            from hermes_cli.runtime_provider import resolve_runtime_provider
            from run_agent import AIAgent

            runtime = resolve_runtime_provider()
            model = runtime.pop("model", None) or self._model_config().get("default") or ""
            if not model and runtime.get("provider"):
                # A brain with no model chosen: the provider's own cost-safe default, as Hermes' gateway does.
                try:
                    from hermes_cli.models import get_default_model_for_provider

                    model = get_default_model_for_provider(runtime["provider"]) or ""
                except Exception:
                    model = ""
            history = []
            if self.db is not None:
                try:
                    history = self.db.get_messages_as_conversation(run.session) or []
                except Exception:
                    history = []

            def on_delta(delta: Optional[str]) -> None:
                if delta:
                    run.put("delta", text=delta)

            def on_tool(event: str, tool_name: Optional[str] = None, preview: Optional[str] = None, args=None, **kw):
                if event == "tool.started":
                    run.put("tool", name=tool_name or "", preview=(preview or "")[:300])
                elif event == "tool.completed":
                    run.put("tool_done", name=tool_name or "", error=bool(kw.get("is_error")))

            agent = AIAgent(
                model=model, api_key=runtime.get("api_key"), base_url=runtime.get("base_url"),
                provider=runtime.get("provider"), requested_provider=runtime.get("requested_provider"),
                api_mode=runtime.get("api_mode"), credential_pool=runtime.get("credential_pool"),
                request_overrides=runtime.get("request_overrides"),
                enabled_toolsets=ios.TOOLSETS, quiet_mode=True, platform="api_server",
                session_id=run.session, session_db=self.db,
                stream_delta_callback=on_delta, tool_progress_callback=on_tool)
            run.agent = agent
            result = agent.run_conversation(user_message=run.message, conversation_history=history,
                                            task_id=run.session)
            if not isinstance(result, dict):
                result = {}
            if result.get("failed"):
                run.put("error", message=str(result.get("error") or "Hermes could not answer."))
            elif result.get("interrupted"):
                run.put("done", text=result.get("final_response") or "", stopped=True)
            else:
                run.put("done", text=result.get("final_response") or "")
        except Exception as exc:
            log.error("run %s failed:\n%s", run.id, traceback.format_exc())
            run.put("error", message=_said(exc))

    def stop_run(self, run_id: str) -> bool:
        run = self.runs.get(run_id)
        if run is None:
            return False
        agent = run.agent
        if agent is not None:
            try:
                agent.interrupt("Stopped from the app.")
            except Exception:
                pass
        return True


def _said(exc: BaseException) -> str:
    """An error as the user may read it: Hermes' own words when it gave some."""
    text = str(exc).strip() or exc.__class__.__name__
    return text[:600]


class Handler(BaseHTTPRequestHandler):
    server_version = "AXYON-Hermes/1"
    protocol_version = "HTTP/1.1"
    host: Host  # set on the class by serve()

    def log_message(self, fmt, *args):  # the requests are the user's: none goes to the log
        pass

    # ── Plumbing ─────────────────────────────────────────────────────────────

    def _allowed(self) -> bool:
        if self.headers.get("Origin"):
            return False  # a web page, never the app
        given = self.headers.get("Authorization", "")
        expected = "Bearer " + self.host.token
        return hmac.compare_digest(given.encode(), expected.encode())

    def _send(self, code: int, obj: Any) -> None:
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _body(self) -> dict[str, Any]:
        length = int(self.headers.get("Content-Length") or 0)
        if length > MAX_BODY:
            raise ValueError("request too large")
        raw = self.rfile.read(length) if length else b"{}"
        data = json.loads(raw or b"{}")
        if not isinstance(data, dict):
            raise ValueError("a JSON object is expected")
        return data

    def _events_start(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        self.close_connection = True

    def _event(self, obj: Optional[dict[str, Any]]) -> None:
        line = ": keepalive\n\n" if obj is None else "data: " + json.dumps(obj, ensure_ascii=False) + "\n\n"
        self.wfile.write(line.encode("utf-8"))
        self.wfile.flush()

    def _peer_gone(self) -> bool:
        """Whether the app closed this stream: it sends nothing on it, so anything to read is its end."""
        try:
            readable, _, _ = select.select([self.connection], [], [], 0)
            return bool(readable) and self.connection.recv(1, socket.MSG_PEEK) == b""
        except OSError:
            return True

    # ── The iPhone's own home ────────────────────────────────────────────────

    def _home(self, work):
        from .home import HomeError

        try:
            return self._send(200, work())
        except HomeError as exc:
            return self._send(422, {"error": str(exc)})

    def _home_post(self, path, body):
        house = self.host.house
        parts = [urllib.parse.unquote(p) for p in path.split("/")[3:]]
        if parts == ["scan"]:
            return self._home(lambda: house.scan(str(body.get("address") or ""), int(body.get("prefix") or 24)))
        if parts == ["devices"]:
            return self._home(lambda: house.add(body))
        if parts == ["assistant"]:
            return self._home(lambda: house.set_assistant(str(body.get("url") or "")))
        if parts == ["assistant", "remove"]:
            return self._home(lambda: (house.remove_assistant(), {"ok": True})[1])
        if len(parts) == 3 and parts[0] == "devices":
            device, what = parts[1], parts[2]
            if what == "pair":
                return self._home(lambda: house.pair(device))
            if what == "remove":
                return self._home(lambda: {"ok": house.remove(device)})
            if what == "action":
                # A tap in the app is the owner's own: nothing more to confirm.
                return self._home(lambda: house.act(device, str(body.get("action") or ""), body.get("value")))
        return self._send(404, {"error": "not found"})

    # ── Routes ───────────────────────────────────────────────────────────────

    def do_GET(self):
        if not self._allowed():
            return self._send(401, {"error": "unauthorized"})
        h = self.host
        path = self.path.split("?")[0]
        if path == "/health":
            return self._send(200, {"ok": True, "protocol": PROTOCOL, "ready": h.ready, "problem": h.problem,
                                    "hermes": h.version, "python": sys.version.split()[0]})
        if path == "/v1/settings":
            return self._send(200, h.settings() if h.ready else {"error": "starting"})
        if path == "/v1/auth/chatgpt":
            return self._send(200, h.auth)
        if path == "/v1/portfolio":
            if not h.ready or h.coucou is None:
                return self._send(503, {"error": h.problem or "Hermes is starting"})
            return self._send(200, json.loads(h.coucou.portfolio({})))
        if path == "/v1/home":
            if not h.ready or h.house is None:
                return self._send(503, {"error": h.problem or "Hermes is starting"})
            return self._home(lambda: h.house.overview(states="quick=1" not in self.path))
        if path.startswith("/v1/runs/") and path.endswith("/events"):
            run = h.runs.get(path.split("/")[3])
            if run is None:
                return self._send(404, {"error": "no such run"})
            after = 0
            if "after=" in self.path:
                try:
                    after = max(0, int(self.path.split("after=")[1].split("&")[0]))
                except ValueError:
                    after = 0
            self._events_start()
            try:
                while True:
                    events, finished = run.after(after, KEEPALIVE)
                    for e in events:
                        self._event(e)
                    after += len(events)
                    if finished and after >= len(run.events):
                        break
                    if not events:
                        self._event(None)
            except (BrokenPipeError, ConnectionResetError):
                pass
            return
        if path == "/v1/bridge":
            self._events_start()
            h.bridge.listening(+1)
            try:
                while True:
                    request = h.bridge.next_request(KEEPALIVE, gone=self._peer_gone)
                    self._event(request)
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            finally:
                h.bridge.listening(-1)
            return
        return self._send(404, {"error": "not found"})

    def do_POST(self):
        if not self._allowed():
            return self._send(401, {"error": "unauthorized"})
        h = self.host
        path = self.path.split("?")[0]
        try:
            body = self._body()
        except (ValueError, json.JSONDecodeError) as exc:
            return self._send(400, {"error": str(exc)})
        try:
            if path == "/v1/runs":
                if not h.ready:
                    return self._send(503, {"error": h.problem or "Hermes is starting"})
                message = str(body.get("message") or "").strip()
                session = str(body.get("session") or "").strip()[:80] or "axyon-iphone"
                if not message:
                    return self._send(400, {"error": "empty message"})
                run = h.start_run(session, message[:MAX_MESSAGE])
                return self._send(202, {"run_id": run.id})
            if path.startswith("/v1/runs/") and path.endswith("/stop"):
                ok = h.stop_run(path.split("/")[3])
                return self._send(200 if ok else 404, {"stopping": ok})
            if path == "/v1/settings":
                if not h.ready:
                    return self._send(503, {"error": h.problem or "Hermes is starting"})
                return self._send(200, h.set_settings(body))
            if path == "/v1/auth/chatgpt/start":
                if not h.ready:
                    return self._send(503, {"error": h.problem or "Hermes is starting"})
                return self._send(200, h.chatgpt_start())
            if path.startswith("/v1/bridge/"):
                ok = h.bridge.answer(path.split("/")[3], body.get("answer"))
                return self._send(200 if ok else 404, {"ok": ok})
            if path.startswith("/v1/home"):
                if not h.ready or h.house is None:
                    return self._send(503, {"error": h.problem or "Hermes is starting"})
                return self._home_post(path, body)
        except ValueError as exc:
            return self._send(400, {"error": str(exc)})
        except Exception as exc:
            log.error("request %s failed:\n%s", path, traceback.format_exc())
            return self._send(500, {"error": _said(exc)})
        return self._send(404, {"error": "not found"})


def serve(host: Host, port: int) -> ThreadingHTTPServer:
    Handler.host = host
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.daemon_threads = True
    return server


# The server answering now, its port and host: revive() replaces the server, never the host (Hermes stays loaded).
_live: dict[str, Any] = {}


def _listen(host: Host, port: int) -> None:
    server = serve(host, port)
    _live.update(server=server, port=port, host=host)

    def loop() -> None:
        try:
            server.serve_forever(poll_interval=1.0)
        except Exception:
            log.warning("the server stopped:\n%s", traceback.format_exc())

    threading.Thread(target=loop, name="axyon-server", daemon=True).start()


def revive() -> None:
    """The app came back on screen. iOS may take a suspended app's listening socket: when the server no longer
    answers on its port, a new one takes its place there (the app calls this through axyon_python_exec)."""
    import socket

    port, host, old = _live.get("port"), _live.get("host"), _live.get("server")
    if port is None or host is None:
        return
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=1.0):
            return
    except OSError:
        pass
    log.info("the server's socket was gone: listening again")
    try:
        old.server_close()
    except Exception:
        pass
    _listen(host, port)


def main() -> None:
    """The app's entry point: prepare, answer at once, load Hermes, then keep the thread while the app runs."""
    logging.basicConfig(level=logging.INFO, format="%(name)s: %(message)s")
    home = os.environ["AXYON_HERMES_HOME"]
    ios.prepare(home)
    host = Host(home, os.environ["AXYON_HOST_TOKEN"], os.environ.get("AXYON_PORTFOLIO", ""))
    _listen(host, int(os.environ["AXYON_HOST_PORT"]))
    # Listening: the app's watch on a start that hangs (AXYONPython.c) is no longer needed.
    import faulthandler

    faulthandler.cancel_dump_traceback_later()
    os.write(2, f"AXYON: Hermes' host listens on 127.0.0.1:{os.environ['AXYON_HOST_PORT']}\n".encode())
    threading.Thread(target=host.start, name="hermes-start", daemon=True).start()
    while True:
        time.sleep(3600)


if __name__ == "__main__":
    main()
