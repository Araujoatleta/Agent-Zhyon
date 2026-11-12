"""AXYON's memory on the iPhone, for Hermes: everything the intern read (every page of every report, material fact
and notice), the court's verdicts, the user's decisions and theses, what the ears transcribed and the notes kept,
indexed on the iPhone itself and searched by meaning and by words at once (AXYONMemory.swift answers over the bridge).
Nothing is sent anywhere to search it."""

from __future__ import annotations

import json
from typing import Any

TOOLSET = "coucou"


class Memory:
    def __init__(self, bridge):
        self.bridge = bridge

    def _ask(self, request: dict[str, Any]) -> str:
        answer = self.bridge.ask("memory", request, wait=30)
        return json.dumps(answer if answer is not None else {"error": "No answer from the iPhone."}, ensure_ascii=False)

    def search(self, args: dict[str, Any], **_kw) -> str:
        query = str((args or {}).get("query") or "").strip()
        if not query:
            return json.dumps({"error": "What to look for? Give `query`."})
        request: dict[str, Any] = {"do": "search", "query": query[:400]}
        if isinstance((args or {}).get("limit"), (int, float)):
            request["limit"] = int(args["limit"])
        return self._ask(request)

    def note(self, args: dict[str, Any], **_kw) -> str:
        text = str((args or {}).get("text") or "").strip()
        if not text:
            return json.dumps({"error": "What to remember? Give `text`."})
        return self._ask({"do": "note", "text": text[:4000], "title": str((args or {}).get("title") or "")[:80]})

    def register(self, registry) -> None:
        registry.register(
            name="memory_search", toolset=TOOLSET, handler=self.search, emoji="🧠",
            schema={"name": "memory_search",
                    "description": "Searches AXYON's memory on this iPhone by meaning: every page of the documents the "
                                   "intern read (managers' reports, material facts, notices, results), the court's "
                                   "verdicts, the user's decisions and theses, the transcripts of audio (results calls, "
                                   "meetings) and the notes kept. Returns the closest passages, each with its title, "
                                   "ticker, date and page: cite them (\"relatório de 09/2026, p. 4\"). Use it whenever "
                                   "the user asks what a document, the court or a call said, or how something changed "
                                   "over time; ask in the user's own words, in Portuguese.",
                    "parameters": {"type": "object", "properties": {
                        "query": {"type": "string", "description": "What to look for, as a question or a few words."},
                        "limit": {"type": "number", "description": "How many passages (8 by default, at most 16)."}},
                        "required": ["query"]}})
        registry.register(
            name="memory_note", toolset=TOOLSET, handler=self.note, emoji="🧠",
            schema={"name": "memory_note",
                    "description": "Keeps a note in AXYON's memory on this iPhone, to be found later by meaning with "
                                   "memory_search: something the user asked to remember, a conclusion worth keeping.",
                    "parameters": {"type": "object", "properties": {
                        "text": {"type": "string"},
                        "title": {"type": "string", "description": "A few words to name it."}},
                        "required": ["text"]}})
