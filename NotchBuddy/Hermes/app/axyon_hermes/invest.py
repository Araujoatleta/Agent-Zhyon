"""AXYON's investing brain on the iPhone: the reader's digest of a document, and the committee's meeting, asked of
Hermes' model with no tools, always in Brazilian Portuguese. The engine (windows/powers, coucou_powers::invest `app`,
through the app's "powers" bridge) writes the brief, with the facts its code computed (each with its source, dates
and formula) and the documents' pages, and keeps the answer: every number is the code's, every point cites its
source, and the five minds are archetypes. The committee meets in two answers: the minds, then an auditor who reads
what the engine's code found in them, revises the minds it must and writes the two final opinions (the asset, and
its fit in the portfolio). Nothing is bought or sold, and the decision stays the user's.

  POST /v1/invest/digest {key}                   the digest of a document the reader read, kept with it
  POST /v1/invest/committee {proposal, ticker, reason, document}
                                                 the committee's verdict, audited and kept (reason: asked, document
                                                 or thesis)
"""

from __future__ import annotations

import threading
from typing import Any, Callable, Optional

# The engine reads the sources it needs first: about a minute at most.
WAIT = 100.0
SYSTEM = ("Você responde para a tela de investimentos do AXYON. Siga o roteiro exatamente, responda só com o JSON "
          "que ele pede, com todo o texto em português do Brasil, e nunca invente um número, um fato ou uma fonte.")
# The reports are the user's, in their language: Portuguese, whatever the iPhone's language.
LANGUAGE = "Brazilian Portuguese"


class InvestError(Exception):
    pass


def ask_brain(prompt: str, system: str = SYSTEM) -> str:
    """One answer of the brain the user chose in AXYON (as host.py's runs use it), with no tools, no memory and no
    session kept."""
    from hermes_cli.runtime_provider import resolve_runtime_provider
    from run_agent import AIAgent

    runtime = resolve_runtime_provider()
    model = runtime.pop("model", None) or ""
    if not model:
        try:
            from hermes_cli.config import load_config

            config = dict(load_config().get("model") or {})
            model = config.get("default") or config.get("model") or ""
        except Exception:
            model = ""
    if not model and runtime.get("provider"):
        try:
            from hermes_cli.models import get_default_model_for_provider

            model = get_default_model_for_provider(runtime["provider"]) or ""
        except Exception:
            model = ""
    agent = AIAgent(
        model=model, api_key=runtime.get("api_key"), base_url=runtime.get("base_url"),
        provider=runtime.get("provider"), requested_provider=runtime.get("requested_provider"),
        api_mode=runtime.get("api_mode"), credential_pool=runtime.get("credential_pool"),
        request_overrides=runtime.get("request_overrides"),
        enabled_toolsets=[], skip_memory=True, skip_context_files=True, quiet_mode=True, platform="api_server",
        max_iterations=2, ephemeral_system_prompt=system)
    result = agent.run_conversation(user_message=prompt)
    if not isinstance(result, dict):
        raise InvestError("The brain did not answer.")
    if result.get("failed"):
        raise InvestError(str(result.get("error") or "The brain could not answer."))
    text = str(result.get("final_response") or "").strip()
    if not text:
        raise InvestError("The brain's answer was empty.")
    return text


class Invest:
    def __init__(self, bridge, ask: Optional[Callable[[str], str]] = None):
        self.bridge = bridge
        self.ask = ask or ask_brain
        # One meeting or digest at a time: each is a long answer of the user's brain.
        self.lock = threading.Lock()

    def _engine(self, tool: str, data: dict[str, Any]) -> Any:
        answer = self.bridge.ask("powers", {"tool": tool, "input": data}, wait=WAIT)
        if not isinstance(answer, dict):
            raise InvestError("The superpowers did not answer.")
        if "error" in answer:
            raise InvestError(str(answer["error"]))
        if not answer.get("ok"):
            raise InvestError(str(answer.get("text") or "The superpowers refused."))
        return answer.get("data")

    def digest(self, body: dict[str, Any]) -> dict[str, Any]:
        key = str(body.get("key") or "").strip()
        if not key:
            raise ValueError("the document's key is missing")
        with self.lock:
            brief = self._engine("invest_docs", {"do": "brief", "key": key, "language": LANGUAGE})
            said = self.ask(str((brief or {}).get("prompt") or ""))
            return self._engine("invest_docs", {"do": "digest", "key": key, "answer": said})

    def committee(self, body: dict[str, Any]) -> dict[str, Any]:
        proposal = str(body.get("proposal") or "").strip()
        if not proposal:
            raise ValueError("the committee needs the proposal")
        asked = {"do": "brief", "proposal": proposal[:600], "ticker": str(body.get("ticker") or ""), "language": LANGUAGE}
        with self.lock:
            brief = self._engine("invest_committee", asked) or {}
            # The facts and documents of this meeting go back with each step: the audit and the verdict check
            # the minds against them.
            meeting = {"ticker": brief.get("ticker") or asked["ticker"], "proposal": proposal[:300],
                       "facts": brief.get("facts") or [], "docs": brief.get("docs") or []}
            said = self.ask(str(brief.get("prompt") or ""))
            verdict = {"do": "verdict", "answer": said, "reason": str(body.get("reason") or "asked"),
                       "document": str(body.get("document") or ""), **meeting}
            try:
                review = self._engine("invest_committee", {"do": "audit", "answer": said, **meeting}) or {}
            except InvestError:
                # The minds' answer cannot be read: the verdict step says why.
                return self._engine("invest_committee", verdict)
            try:
                verdict["audit"] = self.ask(str(review.get("prompt") or ""))
            except Exception as e:  # the auditor's answer failed: the verdict is kept, saying the audit was not done
                verdict["auditFailed"] = str(e) or "o auditor não respondeu"
            return self._engine("invest_committee", verdict)
