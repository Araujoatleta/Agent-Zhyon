"""A fake OpenAI-compatible model for the host's tests: it calls coucou_portfolio when asked about "carteira",
coucou_home about "casa" and iphone_status about "iPhone", then answers "Feito: <what the tool returned>". It logs how
many messages it saw."""
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
            sys.stderr.write(f"msgs={len(msgs)} tools={tools}\n")
            sys.stderr.flush()
        call = None
        if last.get("role") == "tool":
            content = "Feito: " + str(last.get("content"))[:400]
        elif "coucou_portfolio" in tools and "carteira" in asked:
            call = {"id": "call_1", "type": "function", "function": {"name": "coucou_portfolio", "arguments": "{}"}}
        elif "coucou_home" in tools and "casa" in asked:
            call = {"id": "call_2", "type": "function", "function": {"name": "coucou_home", "arguments": "{}"}}
        elif "iphone_status" in tools and "iphone" in asked.lower():
            call = {"id": "call_3", "type": "function", "function": {"name": "iphone_status", "arguments": "{}"}}
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
