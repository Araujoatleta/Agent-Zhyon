"""Coucou's tools for Hermes on the iPhone: its arms.

The portfolio is the copy the computer gave the iPhone over the link (windows/src-tauri/src/powers/mod.rs,
phone_copy), kept by the app in a file this module reads; prices are read here, from Google Finance, during B3's
session (some 15 minutes late). Mochi's home goes through the app (`bridge`): it asks the paired computer when the
link is up, and a device that asks first is acted on only after the owner's Face ID there.
"""

from __future__ import annotations

import concurrent.futures
import json
import os
import re
import time
import urllib.request
from datetime import datetime, timedelta, timezone
from typing import Any, Callable, Optional

TOOLSET = "coucou"
# B3's own time: its sessions and closes are in Brasília (UTC−3, no daylight saving since 2019).
BRT = timezone(timedelta(hours=-3))
QUOTED = {"acao", "fii", "etf", "bdr"}
MOST_QUOTES = 20
PAGE_TIMEOUT = 8.0
USER_AGENT = ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 "
              "(KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1")

_NUMBER = r"-?[0-9]+(?:\.[0-9]+)?(?:[eE][-+]?[0-9]+)?"
# [ticker, exchange], name, kind, currency, [price, change, percent…], null, previous close, …, [time]
# (windows/powers/src/sources/gfinance.rs, the same pattern).
_ENTRY = re.compile(
    r'\["([A-Z0-9]{4,8})","BVMF"\],"(?:[^"\\]|\\.)*",[0-9]+,"[A-Z]{3}",\[(' + _NUMBER + r'),[^\]]*\],null,('
    + _NUMBER + r'|null),[^\[\]]*?\[([0-9]{9,11})\]')
_TICKER = re.compile(r"^[A-Z0-9]{4,8}$")


def parse_quote(page: str, ticker: str) -> Optional[dict[str, Any]]:
    """The price of `ticker` on its Google Finance page: the newest of its entries. None for a page with none."""
    ticker = ticker.strip().upper()
    best = None
    for m in _ENTRY.finditer(page):
        if m.group(1) != ticker:
            continue
        try:
            price = float(m.group(2))
            at = int(m.group(4))
        except ValueError:
            continue
        if not (0 < price < 1e7):
            continue
        previous = None if m.group(3) == "null" else float(m.group(3))
        if previous is not None and previous <= 0:
            previous = None
        if best is None or at > best["at"]:
            best = {"price": price, "at": at, "previousClose": previous}
    return best


def fetch_quote(ticker: str, opener: Callable[[str], str] | None = None) -> Optional[dict[str, Any]]:
    url = f"https://www.google.com/finance/quote/{ticker.strip().upper()}:BVMF?hl=en"
    try:
        if opener is not None:
            page = opener(url)
        else:
            req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT, "Accept-Language": "en"})
            with urllib.request.urlopen(req, timeout=PAGE_TIMEOUT) as resp:
                page = resp.read(4_000_000).decode("utf-8", "replace")
    except Exception:
        return None
    return parse_quote(page, ticker)


def fetch_quotes(tickers: list[str], opener: Callable[[str], str] | None = None) -> dict[str, dict[str, Any]]:
    wanted = []
    for t in tickers:
        t = str(t).strip().upper()
        if _TICKER.match(t) and t not in wanted:
            wanted.append(t)
    wanted = wanted[:MOST_QUOTES]
    out: dict[str, dict[str, Any]] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        for t, q in zip(wanted, pool.map(lambda t: fetch_quote(t, opener), wanted)):
            if q:
                out[t] = q
    return out


def _when(unix: Optional[float]) -> str:
    if not unix:
        return ""
    return datetime.fromtimestamp(unix, BRT).strftime("%d/%m %H:%M")


def _day_change(q: dict[str, Any], close_before: Optional[float]) -> Optional[float]:
    before = q.get("previousClose") or close_before
    if not before:
        return None
    return round((q["price"] / before - 1.0) * 100.0, 2)


def portfolio_report(copy: dict[str, Any], live: dict[str, dict[str, Any]], now: float) -> dict[str, Any]:
    """The portfolio with the prices of now: each position's value, day and gain, and how far each asset is from the
    user's own ceiling."""
    known = copy.get("prices") or {}
    ceilings = {str(k).upper(): float(v) for k, v in (copy.get("ceilings") or {}).items() if v}
    rows, total, cost, moved = [], 0.0, 0.0, 0.0
    for p in copy.get("positions") or []:
        ticker = str(p.get("ticker", ""))
        t = ticker.upper()
        qty = float(p.get("quantity") or 0)
        avg = float(p.get("averagePrice") or 0)
        row: dict[str, Any] = {"ticker": ticker, "class": p.get("class", ""), "quantity": qty, "averagePrice": avg}
        if p.get("class") in QUOTED:
            q = live.get(t)
            last = known.get(t) or {}
            if q:
                row["price"], row["priceAt"], row["priceFrom"] = q["price"], _when(q["at"]), "Google Finance, now"
                before = q.get("previousClose") or last.get("close")
                if before:
                    row["dayChangePct"] = round((q["price"] / before - 1.0) * 100.0, 2)
                    row["dayChangeMoney"] = round(qty * (q["price"] - before), 2)
                    moved += qty * (q["price"] - before)
            elif last.get("price"):
                row["price"] = float(last["price"])
                row["priceFrom"] = "last price the computer had" + (f" ({last.get('closeDay')})" if last.get("closeDay") else "")
            if "price" in row:
                row["value"] = round(qty * row["price"], 2)
                if avg > 0:
                    row["gainPct"] = round((row["price"] / avg - 1) * 100, 2)
        else:
            value = p.get("currentValue")
            row["value"] = round(float(value if value is not None else qty * avg), 2)
        if t in ceilings:
            row["ceiling"] = ceilings[t]
            if row.get("price"):
                row["vsCeilingPct"] = round((row["price"] / ceilings[t] - 1) * 100, 2)
        if p.get("note"):
            row["note"] = p["note"]
        total += row.get("value", 0.0)
        cost += qty * avg
        rows.append(row)
    watched = []
    held = {str(p.get("ticker", "")).upper() for p in copy.get("positions") or []}
    for t, c in ceilings.items():
        if t in held:
            continue
        w: dict[str, Any] = {"ticker": t, "ceiling": c}
        q = live.get(t)
        price = q["price"] if q else (known.get(t) or {}).get("price")
        if price:
            w["price"] = price
            w["vsCeilingPct"] = round((price / c - 1) * 100, 2)
        watched.append(w)
    cash = float(copy.get("cash") or 0)
    report: dict[str, Any] = {
        "positions": rows,
        "watchedWithCeiling": watched,
        "totalValue": round(total + cash, 2),
        "cash": cash,
        "invested": round(cost, 2),
        "source": copy.get("source") or "",
        "copyFromComputer": _when(copy.get("receivedAt")) or "unknown",
        "pricesReadAt": datetime.fromtimestamp(now, BRT).strftime("%d/%m %H:%M"),
    }
    if moved:
        report["dayChangeMoney"] = round(moved, 2)
    if copy.get("notes"):
        report["notesInAxyonTab"] = copy["notes"]
    return report


class Coucou:
    """The tools, registered in Hermes' registry under the `coucou` toolset."""

    def __init__(self, portfolio_file: str, bridge=None, opener: Callable[[str], str] | None = None):
        self.portfolio_file = portfolio_file
        self.bridge = bridge
        self.opener = opener

    def _copy(self) -> Optional[dict[str, Any]]:
        try:
            with open(self.portfolio_file, encoding="utf-8") as f:
                data = json.load(f)
            return data if isinstance(data, dict) and data.get("positions") is not None else None
        except (OSError, ValueError):
            return None

    # Each handler gets the tool's arguments as a dict and returns a JSON string, as Hermes' tools do.

    def portfolio(self, args: dict[str, Any], **_kw) -> str:
        copy = self._copy()
        if copy is None:
            return json.dumps({"error": "No portfolio on this iPhone yet: it arrives from AXYON on the computer the next "
                                        "time the iPhone connects to it (with the superpowers on there)."})
        tickers = [p.get("ticker", "") for p in copy.get("positions") or [] if p.get("class") in QUOTED]
        tickers += list((copy.get("ceilings") or {}).keys())
        live = fetch_quotes(tickers, self.opener)
        return json.dumps(portfolio_report(copy, live, time.time()), ensure_ascii=False)

    def quote(self, args: dict[str, Any], **_kw) -> str:
        tickers = args.get("tickers") or []
        if isinstance(tickers, str):
            tickers = re.split(r"[,\s]+", tickers)
        live = fetch_quotes([str(t) for t in tickers], self.opener)
        out = {}
        for t, q in live.items():
            out[t] = {"price": q["price"], "at": _when(q["at"]), "previousClose": q.get("previousClose"),
                      "dayChangePct": _day_change(q, None)}
        missing = [str(t).upper() for t in tickers if str(t).upper() not in out]
        result: dict[str, Any] = {"quotes": out, "source": "Google Finance (B3, about 15 minutes late)"}
        if missing:
            result["noPrice"] = missing
        return json.dumps(result, ensure_ascii=False)

    def home(self, args: dict[str, Any], **_kw) -> str:
        if self.bridge is None:
            return json.dumps({"error": "Mochi's home is not reachable from here."})
        return json.dumps(self.bridge.ask("home", {"what": "status"}), ensure_ascii=False)

    def home_action(self, args: dict[str, Any], **_kw) -> str:
        if self.bridge is None:
            return json.dumps({"error": "Mochi's home is not reachable from here."})
        request = {k: args.get(k) for k in ("device", "action", "value") if args.get(k) is not None}
        return json.dumps(self.bridge.ask("homeAction", request), ensure_ascii=False)

    def register(self, registry) -> None:
        tools = [
            ("coucou_portfolio", self.portfolio,
             "The user's investment portfolio as AXYON on their computer last gave it to this iPhone, with B3 prices "
             "read now from Google Finance (about 15 minutes late): each position's quantity, average price, price, "
             "value, day change and gain, the user's own ceiling (preço teto) and how far the price is from it, the "
             "assets they watch with a ceiling, and the total. Use it for any question about their investments.",
             {"type": "object", "properties": {}}),
            ("coucou_quote", self.quote,
             "Prices of B3 assets (shares, FIIs, ETFs, BDRs) from Google Finance, about 15 minutes late: price, "
             "previous close and day change.",
             {"type": "object", "properties": {"tickers": {"type": "array", "items": {"type": "string"},
                                                           "description": "Tickers, e.g. [\"GARE11\", \"PETR4\"]"}},
              "required": ["tickers"]}),
            ("coucou_home", self.home,
             "Mochi's home: the user's devices (TVs, lights…) and routines on their paired computer, with what each "
             "can do. Works while the iPhone reaches the computer (at home, or through Tailscale).",
             {"type": "object", "properties": {}}),
            ("coucou_home_action", self.home_action,
             "Act on one of the user's devices through their computer: turn_on, turn_off, set_volume, volume_up, "
             "volume_down, mute, unmute, open_app, set_channel, channel_up, channel_down, set_brightness. A device "
             "that asks first waits for the owner's Face ID on the iPhone.",
             {"type": "object", "properties": {
                 "device": {"type": "string", "description": "The device's id, from coucou_home"},
                 "action": {"type": "string"},
                 "value": {"description": "The volume, app, channel or brightness, when the action takes one"}},
              "required": ["device", "action"]}),
        ]
        for name, handler, description, parameters in tools:
            registry.register(
                name=name, toolset=TOOLSET, handler=handler, emoji="🏠" if "home" in name else "📈",
                schema={"name": name, "description": description, "parameters": parameters})
