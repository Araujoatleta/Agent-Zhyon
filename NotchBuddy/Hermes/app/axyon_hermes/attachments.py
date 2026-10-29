"""What the user attaches in the iPhone's chat (photos, PDFs, documents, text files) made into the turn Hermes reads,
as Hermes' gateway does for a message with media (gateway/run.py, gateway/run_inbound.py).

The app copies each file into Hermes' home, under inbox/, with the text of a PDF it read itself (PDFKit, or the
iPhone's own text recognition for a scanned one) next to it. A photo goes to the model as an image when the brain sees
(agent/image_routing.py decides, as for the computer's chats), or Hermes' vision tool describes it first when it does
not. A document's text is put in the message; every file stays in the inbox, where iphone_files reads it again."""

from __future__ import annotations

import json
import os
from typing import Any, Callable, Optional

INBOX = "inbox"
MAX_FILES = 10
# Characters of the documents' text put into one message; the rest is read with iphone_files.
MAX_INLINE = 60_000
# A text file read as it is, up to this size.
MAX_TEXT_BYTES = 4 * 1024 * 1024
IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".heic", ".heif", ".bmp", ".tif", ".tiff"}
TEXT_EXTENSIONS = {".txt", ".md", ".markdown", ".csv", ".tsv", ".json", ".xml", ".html", ".htm", ".log", ".yaml",
                   ".yml", ".ics", ".vcf", ".srt", ".py", ".js", ".ts", ".sql", ".ini", ".toml"}
# Read by Hermes' own extraction, with the standard library only (tools/read_extract.py).
EXTRACTED_EXTENSIONS = {".docx", ".xlsx", ".ipynb"}

DESCRIBE_PROMPT = (
    "Concisely describe this image in 2-4 sentences (about 150 words). Cover the main subject, every visible text, "
    "figure or number, and the overall context. If it is a chart, a receipt, a statement or a screenshot, give its "
    "important labels and values. Skip decorative details.")


def inbox(home: str) -> str:
    return os.path.join(home, INBOX)


def _inside(root: str, path: str) -> Optional[str]:
    real = os.path.realpath(path)
    return real if real.startswith(root + os.sep) and os.path.isfile(real) else None


def accept(home: str, raw: Any) -> list[dict[str, Any]]:
    """The app's attachments for a run, each a file it put in Hermes' inbox: anything else is refused."""
    if not raw:
        return []
    if not isinstance(raw, list):
        raise ValueError("attachments must be a list")
    root = os.path.realpath(inbox(home))
    accepted = []
    for item in raw[:MAX_FILES]:
        if not isinstance(item, dict):
            raise ValueError("an attachment is an object")
        path = _inside(root, str(item.get("path") or ""))
        if path is None:
            raise ValueError(f"not an attachment of this iPhone: {item.get('name') or item.get('path')}")
        text = _inside(root, str(item.get("text") or "")) if item.get("text") else None
        extension = os.path.splitext(path)[1].lower()
        kind = "image" if item.get("kind") == "image" or extension in IMAGE_EXTENSIONS else "file"
        accepted.append({"path": path, "name": str(item.get("name") or os.path.basename(path))[:200],
                         "kind": kind, "text": text})
    return accepted


def document_text(path: str, text_path: Optional[str] = None) -> Optional[str]:
    """A document's text: what the app read from a PDF, a text file as it is, a Word or Excel file or a notebook
    through Hermes' own extraction. None for what cannot be read on the iPhone (the file is still there)."""
    if text_path:
        with open(text_path, encoding="utf-8", errors="replace") as f:
            return f.read()
    extension = os.path.splitext(path)[1].lower()
    if extension in TEXT_EXTENSIONS:
        with open(path, "rb") as f:
            return f.read(MAX_TEXT_BYTES).decode("utf-8", errors="replace")
    if extension in EXTRACTED_EXTENSIONS:
        try:
            from tools.read_extract import extract_document_text

            return extract_document_text(path)
        except Exception:
            return None
    return None


def _document_note(item: dict[str, Any], budget: int) -> tuple[str, int]:
    """The note for one document and the budget left: its text inlined as far as the budget goes."""
    name, path = item["name"], item["path"]
    try:
        text = document_text(path, item.get("text"))
    except OSError:
        text = None
    if text is None:
        return (f"[The user sent a file: '{name}'. It is saved at: {path}. Its text cannot be read on the iPhone: "
                f"say so if its content matters, and ask what they need from it.]"), budget
    text = text.strip()
    if not text:
        return (f"[The user sent a document: '{name}', saved at: {path}. It has no text that could be read (a scan "
                f"the iPhone could not read, or an empty file).]"), budget
    shown = text[:budget]
    cut = len(shown) < len(text)
    head = (f"[The user sent a document: '{name}'. It is saved at: {path}. Its text follows"
            + (f" up to character {len(shown)} of {len(text)}: read the rest with iphone_files (do=read, "
               f"path, offset)." if cut else ".") + "]")
    return head + "\n" + shown, budget - len(shown)


def compose(message: str, items: list[dict[str, Any]], *, native: bool,
            describe: Callable[[str], str]) -> str | list[dict[str, Any]]:
    """The turn as Hermes reads it: the documents' notes and text before the user's words, and the photos either as
    images for a brain that sees (`native`) or as `describe`d notes before them."""
    notes = []
    budget = MAX_INLINE
    for item in items:
        if item["kind"] == "file":
            note, budget = _document_note(item, budget)
            notes.append(note)
    images = [item["path"] for item in items if item["kind"] == "image"]
    text = "\n\n".join(notes + ([message] if message else []))
    if not images:
        return text
    if native:
        from agent.image_routing import build_native_content_parts

        parts, skipped = build_native_content_parts(text, images)
        unread = "\n".join(f"[A photo the user sent could not be read: {path}]" for path in skipped)
        if len(parts) > 1:
            if unread:
                parts[0]["text"] += "\n" + unread
            return parts
        return "\n\n".join(p for p in (unread, text) if p)
    return "\n\n".join([describe(path) for path in images] + ([text] if text else []))


def describe_with_vision(path: str) -> str:
    """A photo described by Hermes' vision tool, for a brain that does not see, with where it is for a closer look."""
    import asyncio

    try:
        from tools.vision_tools import vision_analyze_tool

        result = json.loads(asyncio.run(vision_analyze_tool(image_url=path, user_prompt=DESCRIBE_PROMPT)))
        if result.get("success") and result.get("analysis"):
            return (f"[The user sent a photo. What it shows:\n{result['analysis']}]\n"
                    f"[For a closer look, use vision_analyze with image_url: {path}]")
    except Exception:
        pass
    return (f"[The user sent a photo, saved at: {path}. It could not be described this time: try vision_analyze "
            f"with image_url: {path}]")


# ── iphone_files: the attachments again, for Hermes ─────────────────────────────

TOOLSET = "coucou"
READ_LIMIT = 20_000

SCHEMA = {
    "name": "iphone_files",
    "description": (
        "The files the user sent in AXYON's chat on this iPhone (photos, PDFs, Word, Excel, text): `do`=list the "
        "latest, or read one's text (`path` as given in the conversation or by list; `offset` to go on from a "
        "character). Photos are seen with vision_analyze and their path."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["list", "read"]},
        "path": {"type": "string"},
        "offset": {"type": "integer"}}, "required": ["do"]},
}


class Files:
    def __init__(self, home: str):
        self.home = home

    def handle(self, args: dict[str, Any], **_kw) -> str:
        return json.dumps(self.call(args or {}), ensure_ascii=False)

    def call(self, args: dict[str, Any]) -> dict[str, Any]:
        root = os.path.realpath(inbox(self.home))
        if args.get("do") == "list":
            files = []
            if os.path.isdir(root):
                for name in os.listdir(root):
                    path = os.path.join(root, name)
                    if os.path.isfile(path) and not name.endswith(".txt.extracted"):
                        files.append((os.path.getmtime(path), name, path, os.path.getsize(path)))
            files.sort(reverse=True)
            return {"files": [{"name": n, "path": p, "bytes": s} for _, n, p, s in files[:30]]}
        if args.get("do") == "read":
            path = _inside(root, str(args.get("path") or ""))
            if path is None:
                return {"error": "No such file among the user's attachments: use do=list."}
            if os.path.splitext(path)[1].lower() in IMAGE_EXTENSIONS:
                return {"error": "A photo: look at it with vision_analyze and this path.", "path": path}
            sidecar = path + ".txt.extracted"
            text = document_text(path, sidecar if os.path.isfile(sidecar) else None)
            if text is None:
                return {"error": "This kind of file cannot be read on the iPhone.", "path": path}
            offset = max(0, int(args.get("offset") or 0))
            part = text[offset:offset + READ_LIMIT]
            more = offset + len(part) < len(text)
            return {"path": path, "offset": offset, "text": part, "length": len(text),
                    **({"next_offset": offset + len(part)} if more else {})}
        return {"error": "do must be list or read"}

    def register(self, registry) -> None:
        registry.register(name="iphone_files", toolset=TOOLSET, schema=SCHEMA, handler=self.handle, emoji="📎")
