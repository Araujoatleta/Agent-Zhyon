"""AXYON's superpowers for Hermes on the iPhone, as on the computer: the portfolio against B3, the CVM, FundosNet and
the Banco Central, the ceilings, taxes, the committee and the decision journal, and the studies, cards, English and
training. The engine is the computer's own (windows/powers), built into the app (windows/powers-ffi), and the app
answers each call with it (HermesArms.swift, "powers"); the tools are the computer's own too (house_tools.json, the
file AXYON on the computer gives Hermes, copied next to this one by build-ios.sh).

The month's content kit stays on the computer, where its files are saved."""

from __future__ import annotations

import json
import os
from typing import Any

TOOLSET = "coucou"
HERE = os.path.dirname(os.path.abspath(__file__))
# Next to this file in the app; the computer's own in a checkout.
TOOLS_FILES = [os.path.join(HERE, "house_tools.json"),
               os.path.join(HERE, "..", "..", "..", "..", "windows", "hook", "src", "house_tools.json")]
LEFT_OUT = {"life_content"}
# The engine reads the sources it needs first: about a minute at most, then it answers with what it has.
WAIT = 100.0


def load_tools(files: list[str] | None = None) -> list[dict[str, Any]]:
    """The wealth_ and life_ tools of the computer's tool file, as it describes them."""
    for path in files or TOOLS_FILES:
        try:
            with open(path, encoding="utf-8") as f:
                tools = json.load(f)
        except (OSError, ValueError):
            continue
        return [t for t in tools if isinstance(t, dict) and str(t.get("name", "")).startswith(("wealth_", "life_"))
                and t["name"] not in LEFT_OUT]
    return []


class Powers:
    def __init__(self, bridge, files: list[str] | None = None):
        self.bridge = bridge
        self.tools = load_tools(files)

    def call(self, tool: str, args: dict[str, Any]) -> str:
        """The engine's answer, in English for the model; a refusal as an error it can read."""
        answer = self.bridge.ask("powers", {"tool": tool, "input": args or {}}, wait=WAIT)
        if not isinstance(answer, dict):
            return json.dumps({"error": "No answer from the superpowers."})
        if "error" in answer:
            return json.dumps({"error": answer["error"]}, ensure_ascii=False)
        text = str(answer.get("text") or "")
        return text if answer.get("ok") else json.dumps({"error": text}, ensure_ascii=False)

    def _handler(self, tool: str):
        def run(args: dict[str, Any], **_kw) -> str:
            return self.call(tool, args)
        return run

    def register(self, registry) -> None:
        for t in self.tools:
            schema = {"name": t["name"], "description": t.get("description", ""),
                      "parameters": t.get("inputSchema") or {"type": "object", "properties": {}}}
            registry.register(name=t["name"], toolset=TOOLSET, schema=schema, handler=self._handler(t["name"]),
                              emoji="📈" if t["name"].startswith("wealth_") else "🎯")
