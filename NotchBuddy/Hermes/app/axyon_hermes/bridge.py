"""What Hermes asks of the app itself: the requests wait in a queue the app reads (GET /v1/bridge, a stream it keeps
open), and the app answers each one (POST /v1/bridge/<id>). A request no one answers in time ends with an error the
model can read: the app is not listening, or the computer behind it did not answer."""

from __future__ import annotations

import itertools
import threading
from typing import Any, Callable, Optional

ANSWER_WAIT = 45.0
# How long a request waits for the app's stream to be there.
LISTENER_WAIT = 5.0
MAX_WAITING = 32


class Bridge:
    def __init__(self) -> None:
        self._lock = threading.Condition()
        self._queue: list[dict[str, Any]] = []
        self._answers: dict[str, Any] = {}
        # The requests someone still waits on: an answer to any other is dropped.
        self._pending: set[str] = set()
        self._ids = itertools.count(1)
        self.listeners = 0

    def ask(self, kind: str, request: dict[str, Any], wait: float = ANSWER_WAIT) -> Any:
        """Sends `request` to the app and waits for its answer (a JSON value). An app that is not listening gets a
        moment to come back (its stream reopens after a launch or a suspension), never the whole wait."""
        with self._lock:
            if not self._lock.wait_for(lambda: self.listeners > 0, timeout=min(LISTENER_WAIT, wait)):
                return {"error": "The AXYON app is not listening right now."}
            if len(self._queue) >= MAX_WAITING:
                return {"error": "Too many requests are waiting for the app."}
            rid = f"b{next(self._ids)}"
            self._pending.add(rid)
            self._queue.append({"id": rid, "kind": kind, "request": request})
            self._lock.notify_all()
            try:
                if not self._lock.wait_for(lambda: rid in self._answers, timeout=wait):
                    self._queue = [q for q in self._queue if q["id"] != rid]
                    return {"error": "The app did not answer in time."}
                return self._answers.pop(rid)
            finally:
                self._pending.discard(rid)

    def next_request(self, timeout: float, gone: Optional[Callable[[], bool]] = None) -> Optional[dict[str, Any]]:
        """The oldest request not handed out yet, or None after `timeout` seconds (the stream then sends a keepalive).
        A stream the app already closed (`gone`, asked before a request is taken) ends instead, and leaves the request
        to the app's new one: the app opens another after a launch or a suspension, and the host sees the old one
        closed only when it next writes to it."""
        with self._lock:
            if not self._lock.wait_for(lambda: bool(self._queue), timeout=timeout):
                return None
            if gone is not None and gone():
                raise ConnectionResetError("the app closed this stream")
            return self._queue.pop(0)

    def answer(self, rid: str, value: Any) -> bool:
        with self._lock:
            if rid not in self._pending or rid in self._answers:
                return False
            self._answers[rid] = value
            self._lock.notify_all()
            return True

    def listening(self, delta: int) -> None:
        with self._lock:
            self.listeners = max(0, self.listeners + delta)
            self._lock.notify_all()
