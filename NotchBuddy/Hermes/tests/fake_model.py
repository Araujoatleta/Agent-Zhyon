"""A fake OpenAI-compatible model for the host's tests: it calls wealth_portfolio when asked about "carteira",
life_english (15 minutes logged) about "inglês", coucou_home about "casa", iphone_status about "iPhone" and cronjob_manage
(a task every 5 minutes) about "agenda", then answers "Feito: <what the tool returned>". With no tools, AXYON's
committee gets its five minds as JSON in a fence, then its auditor's review, and its reader a document's digest. Sent files are counted: it
answers "Vi <images> imagem(ns) e <documents> documento(s): <the documents' first words>". It logs how many messages it
saw."""
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class Model(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.rstrip("/").endswith("/models"):
            return self._json(200, {"object": "list", "data": [{"id": "fake-model", "object": "model"}]})
        self._json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
        msgs = req.get("messages", [])
        tools = [t.get("function", {}).get("name") for t in req.get("tools") or []]
        users = [m for m in msgs if m.get("role") == "user"]
        asked = json.dumps(users[-1].get("content") if users else "", ensure_ascii=False)
        last = msgs[-1] if msgs else {}
        if tools:
            content = users[-1].get("content") if users else ""
            parts = [p.get("type") for p in content if isinstance(p, dict)] if isinstance(content, list) else ["str"]
            sys.stderr.write(f"msgs={len(msgs)} parts={parts} tools={tools}\n")
            sys.stderr.flush()
        call = None
        said = users[-1].get("content") if users else ""
        images = sum(1 for p in said if isinstance(p, dict) and p.get("type") == "image_url") if isinstance(said, list) else 0
        flat = json.dumps(said, ensure_ascii=False)
        documents = flat.count("[The user sent a document")
        if last.get("role") == "tool":
            content = "Feito: " + str(last.get("content"))[:400]
        elif last.get("role") == "user" and (images or documents or "The user sent a photo" in flat):
            first = flat.split("Its text follows.]\\n")[1][:40] if "Its text follows.]\\n" in flat else ""
            photo = images or flat.count("The user sent a photo")
            content = f"Vi {photo} imagem(ns) e {documents} documento(s): {first}"
        elif not tools and "Você é o auditor do comitê" in asked:
            # The committee's auditor: what it found, a mind revised, the two final opinions.
            sys.stderr.write(f"invest audit: tools={len(tools)}\n")
            sys.stderr.flush()
            content = json.dumps({"audit": [{"severity": "material", "minds": ["forensic"], "text": "Falta de dado contábil não é prejuízo.",
                                             "sources": ["F2"]}],
                                  "revisions": [{"id": "forensic", "vote": "neutral", "confidence": 40, "why": "Sem dados contábeis.",
                                                 "points": [{"text": "DRE não publicada", "source": "", "kind": "ausente"}]}],
                                  "asset": {"view": "neutro", "confidence": 55, "why": "Vacância subiu.", "sources": ["D1 p.2"]},
                                  "portfolio": {"view": "evitar_novo_aporte", "why": "Acima da meta.", "sources": ["F2"]},
                                  "chair": {"question": "Manter GGRC11?", "dissent": "P/VP baixo.", "wouldChange": "Vacância de volta.",
                                            "fit": "FIIs acima da meta."},
                                  "toVerify": ["A vacância do próximo relatório"]}, ensure_ascii=False)
        elif not tools and "Você é o comitê de investimentos do AXYON" in asked:
            # The committee's five minds, asked with no tools: their JSON, in a fence, as models often answer.
            sys.stderr.write(f"invest committee: tools={len(tools)}\n")
            sys.stderr.flush()
            minds = [{"id": m, "vote": "against" if m in ("legal", "risk", "forensic") else "for", "confidence": 70,
                      "veto": m == "risk", "points": [{"text": f"{m}: a vacância subiu", "source": "D1 p.2", "kind": "fato"}]}
                     for m in ("valuation", "legal", "forensic", "risk", "macro")]
            content = "```json\n" + json.dumps({"minds": minds, "asset": {"view": "neutro"}, "portfolio": {"view": "manter"},
                                                 "chair": {"question": "Manter GGRC11?", "dissent": "P/VP baixo.",
                                                           "wouldChange": "Vacância de volta.", "fit": "FIIs acima da meta."}}) + "\n```"
        elif not tools and "You are AXYON's reader" in asked:
            content = json.dumps({"headline": "Vacância subiu", "points": [{"text": "Vacância de 6% para 8,5%", "page": 2}],
                                  "tone": "worse", "thesisRisk": True})
        elif "wealth_portfolio" in tools and "carteira" in asked:
            call = {"id": "call_1", "type": "function", "function": {"name": "wealth_portfolio", "arguments": "{}"}}
        elif "life_english" in tools and "ingl" in asked.lower():
            call = {"id": "call_4", "type": "function",
                    "function": {"name": "life_english", "arguments": json.dumps({"action": "log", "minutes": 15})}}
        elif "coucou_home" in tools and "casa" in asked:
            call = {"id": "call_2", "type": "function", "function": {"name": "coucou_home", "arguments": "{}"}}
        elif "iphone_status" in tools and "iphone" in asked.lower():
            call = {"id": "call_3", "type": "function", "function": {"name": "iphone_status", "arguments": "{}"}}
        elif "cronjob_manage" in tools and "agenda" in asked.lower():
            call = {"id": "call_5", "type": "function", "function": {"name": "cronjob_manage", "arguments": json.dumps(
                {"action": "create", "schedule": "every 5m", "name": "Cotação BTLG11",
                 "prompt": "Diga como está o BTLG11"})}}
        else:
            content = "ok"
        model = req.get("model", "fake-model")
        if not req.get("stream"):
            msg = {"role": "assistant", "content": None if call else content}
            if call:
                msg["tool_calls"] = [call]
            return self._json(200, {"id": "c1", "object": "chat.completion", "created": int(time.time()), "model": model,
                                    "choices": [{"index": 0, "message": msg,
                                                 "finish_reason": "tool_calls" if call else "stop"}],
                                    "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        base = {"id": "c1", "object": "chat.completion.chunk", "created": int(time.time()), "model": model}

        def send(obj):
            self.wfile.write(f"data: {json.dumps(obj)}\n\n".encode())
            self.wfile.flush()

        send({**base, "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": None}]})
        if call:
            send({**base, "choices": [{"index": 0, "delta": {"tool_calls": [{"index": 0, **call}]}, "finish_reason": None}]})
            send({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]})
        else:
            for word in content.split(" "):
                send({**base, "choices": [{"index": 0, "delta": {"content": word + " "}, "finish_reason": None}]})
            send({**base, "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})
        send({**base, "choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}})
        self.wfile.write(b"data: [DONE]\n\n")
        self.wfile.flush()


if __name__ == "__main__":
    ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1])), Model).serve_forever()
