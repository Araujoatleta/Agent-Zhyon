"""The web for Hermes on the iPhone, with no key and no account: a search (DuckDuckGo's HTML results, the page any
browser gets) and reading a page as text. Only when Hermes asks: nothing is sent anywhere otherwise."""

from __future__ import annotations

import html
import json
import re
import urllib.parse
import urllib.request
from typing import Any

TOOLSET = "coucou"
TIMEOUT = 15.0
MAX_PAGE = 3_000_000
MAX_TEXT = 14_000
USER_AGENT = ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 "
              "(KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1")

_RESULT = re.compile(r'<a[^>]+class="result__a"[^>]+href="([^"]+)"[^>]*>(.*?)</a>', re.S)
_SNIPPET = re.compile(r'<a[^>]+class="result__snippet"[^>]*>(.*?)</a>', re.S)
_TAG = re.compile(r"<[^>]+>")
_DROP = re.compile(r"<(script|style|noscript|svg|head|nav|footer|form)\b.*?</\1>", re.S | re.I)
_BLOCK = re.compile(r"</?(p|div|br|li|h[1-6]|tr|section|article|header)\b[^>]*>", re.I)


def _text(fragment: str) -> str:
    return re.sub(r"\s+", " ", html.unescape(_TAG.sub("", fragment or ""))).strip()


def _real_link(href: str) -> str:
    """DuckDuckGo's own redirect, back to the address it points to."""
    href = html.unescape(href)
    if href.startswith("//"):
        href = "https:" + href
    parsed = urllib.parse.urlparse(href)
    if parsed.netloc.endswith("duckduckgo.com") and parsed.path.startswith("/l/"):
        target = urllib.parse.parse_qs(parsed.query).get("uddg", [""])[0]
        return target or href
    return href


def parse_results(page: str, limit: int = 8) -> list[dict[str, str]]:
    """Each result's title and address, and its snippet: the first one before the next result."""
    found = list(_RESULT.finditer(page))
    out = []
    for i, m in enumerate(found):
        link = _real_link(m.group(1))
        if not link.startswith(("http://", "https://")):
            continue
        end = found[i + 1].start() if i + 1 < len(found) else len(page)
        snippet = _SNIPPET.search(page, m.end(), end)
        out.append({"title": _text(m.group(2)), "url": link, "snippet": _text(snippet.group(1)) if snippet else ""})
        if len(out) >= limit:
            break
    return out


def page_text(page: str) -> tuple[str, str]:
    """The title and the readable text of an HTML page."""
    title = _text((re.search(r"<title[^>]*>(.*?)</title>", page, re.S | re.I) or [None, ""])[1] if page else "")
    body = _DROP.sub(" ", page)
    body = _BLOCK.sub("\n", body)
    lines = [re.sub(r"[ \t]+", " ", html.unescape(_TAG.sub("", line))).strip() for line in body.split("\n")]
    text = "\n".join(line for line in lines if len(line) > 1)
    return title, re.sub(r"\n{3,}", "\n\n", text)


def _get(url: str, data: bytes | None = None) -> tuple[str, str]:
    req = urllib.request.Request(url, data=data, headers={"User-Agent": USER_AGENT, "Accept-Language": "pt-BR,pt;q=0.9,en;q=0.8"})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
        kind = resp.headers.get("Content-Type", "")
        raw = resp.read(MAX_PAGE)
    charset = (re.search(r"charset=([\w-]+)", kind) or [None, "utf-8"])[1]
    return raw.decode(charset, "replace"), kind


class Web:
    def search(self, args: dict[str, Any], **_kw) -> str:
        query = str(args.get("query") or "").strip()
        if not query:
            return json.dumps({"error": "What to search for?"})
        limit = max(1, min(int(args.get("limit") or 8), 15))
        try:
            page, _ = _get("https://html.duckduckgo.com/html/", urllib.parse.urlencode({"q": query, "kl": "br-pt"}).encode())
        except Exception as exc:
            return json.dumps({"error": f"The search did not answer: {exc}"})
        results = parse_results(page, limit)
        return json.dumps({"query": query, "results": results} if results else
                          {"query": query, "results": [], "note": "No results (or the search engine refused)."},
                          ensure_ascii=False)

    def read(self, args: dict[str, Any], **_kw) -> str:
        url = str(args.get("url") or "").strip()
        if not url.startswith(("http://", "https://")):
            return json.dumps({"error": "Only http and https pages."})
        try:
            page, kind = _get(url)
        except Exception as exc:
            return json.dumps({"error": f"The page did not answer: {exc}"})
        if "html" not in kind and "<html" not in page[:2000].lower():
            text = page[:MAX_TEXT]
            return json.dumps({"url": url, "text": text, "cut": len(page) > MAX_TEXT}, ensure_ascii=False)
        title, text = page_text(page)
        return json.dumps({"url": url, "title": title, "text": text[:MAX_TEXT], "cut": len(text) > MAX_TEXT},
                          ensure_ascii=False)

    def register(self, registry) -> None:
        registry.register(
            name="coucou_web_search", toolset=TOOLSET, handler=self.search, emoji="🔎",
            schema={"name": "coucou_web_search",
                    "description": "Searches the web (DuckDuckGo): titles, addresses and snippets of the first results. "
                                   "For news, prices, opening hours, anything that may have changed.",
                    "parameters": {"type": "object", "properties": {"query": {"type": "string"},
                                                                    "limit": {"type": "integer"}},
                                   "required": ["query"]}})
        registry.register(
            name="coucou_web_read", toolset=TOOLSET, handler=self.read, emoji="📄",
            schema={"name": "coucou_web_read",
                    "description": "Reads a web page as text (its first 14,000 characters).",
                    "parameters": {"type": "object", "properties": {"url": {"type": "string"}},
                                   "required": ["url"]}})
