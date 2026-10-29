"""Google (Gmail, Sheets, Drive, Docs, Calendar) for Hermes on the iPhone, with the user's own Google sign-in.

On a computer, Hermes' google-workspace skill does this with scripts it runs in a terminal (setup.py, google_api.py).
iOS runs no other program, so the same sign-in and the same calls are made here, in Hermes' own process: with the
same files in Hermes' home (google_client_secret.json, google_token.json), the same OAuth (the user's own Google Cloud
"Desktop app" client, with PKCE), and Google's own REST APIs.

The sign-in: google_account(do=connect) gives a link. The user opens it (the app opens it inside itself, so it stays
awake), signs in and allows; Google sends the browser back to this host on 127.0.0.1, which finishes it alone. If the
page could not load (another browser, or the app restarted), the user copies its address and pastes it in the chat,
and google_account(do=finish) finishes it the same way.

Nothing is sent without the user: gmail(do=send) and gmail(do=reply) show the e-mail in the app, and send it only
after the user taps Send there."""

from __future__ import annotations

import base64
import hashlib
import html
import json
import os
import re
import secrets
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
from typing import Any, Callable, Optional

TOOLSET = "google"
TIMEOUT = 30

SERVICES = {
    "email": ["https://www.googleapis.com/auth/gmail.readonly", "https://www.googleapis.com/auth/gmail.send",
              "https://www.googleapis.com/auth/gmail.modify"],
    "calendar": ["https://www.googleapis.com/auth/calendar"],
    "drive": ["https://www.googleapis.com/auth/drive"],
    "sheets": ["https://www.googleapis.com/auth/spreadsheets"],
    "docs": ["https://www.googleapis.com/auth/documents"],
    "contacts": ["https://www.googleapis.com/auth/contacts.readonly"],
}

# Google's own addresses; a test gives its own.
ENDPOINTS = {
    "auth": "https://accounts.google.com/o/oauth2/v2/auth",
    "token": "https://oauth2.googleapis.com/token",
    "revoke": "https://oauth2.googleapis.com/revoke",
    "gmail": "https://gmail.googleapis.com/gmail/v1/users/me",
    "sheets": "https://sheets.googleapis.com/v4/spreadsheets",
    "drive": "https://www.googleapis.com/drive/v3",
    "calendar": "https://www.googleapis.com/calendar/v3",
}

SETUP_STEPS = (
    "To connect Google, the user needs their own Google Cloud OAuth client, once (about 5 minutes): "
    "1) https://console.cloud.google.com/projectselector2/home/dashboard — create or pick a project; "
    "2) https://console.cloud.google.com/apis/library — enable Gmail API, Google Sheets API, Google Drive API, "
    "Google Docs API and Google Calendar API; 3) https://console.cloud.google.com/apis/credentials — Create "
    "credentials → OAuth client ID → Application type “Desktop app” → Create, and download the JSON; "
    "4) https://console.cloud.google.com/auth/audience — while the app is in Testing, add their own Google account "
    "as a test user. Then they send the JSON file in this chat (the clip), or paste the client ID and client secret: "
    "call google_account(do=client) with it, then google_account(do=connect). A client they already made for gog or "
    "for Hermes on the computer works the same.")


class GoogleError(Exception):
    pass


def _b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")


def _now() -> float:
    return time.time()


def spreadsheet_id(value: str) -> str:
    """A spreadsheet's id from its id or any of its links."""
    found = re.search(r"/spreadsheets/d/([A-Za-z0-9_-]+)", value or "")
    return found.group(1) if found else (value or "").strip()


def drive_id(value: str) -> str:
    """A Drive file's id from its id or its link (…/d/<id>/…, ?id=<id>)."""
    found = re.search(r"/d/([A-Za-z0-9_-]{10,})", value or "") or re.search(r"[?&]id=([A-Za-z0-9_-]{10,})", value or "")
    return found.group(1) if found else (value or "").strip()


class Google:
    def __init__(self, home: str, bridge=None, port: Optional[Callable[[], int]] = None,
                 endpoints: Optional[dict[str, str]] = None, inbox: Optional[str] = None):
        self.home = home
        self.bridge = bridge
        self.port = port or (lambda: int(os.environ.get("AXYON_HOST_PORT") or 0))
        self.endpoints = dict(ENDPOINTS, **(endpoints or {}))
        self.inbox = inbox or os.path.join(home, "inbox")
        self.client_path = os.path.join(home, "google_client_secret.json")
        self.token_path = os.path.join(home, "google_token.json")
        self.pending_path = os.path.join(home, "google_oauth_pending.json")

    # ── Files ────────────────────────────────────────────────────────────────

    def _read(self, path: str) -> dict[str, Any]:
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
            return data if isinstance(data, dict) else {}
        except (OSError, ValueError):
            return {}

    def _write(self, path: str, data: dict[str, Any]) -> None:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        part = path + ".part"
        with open(os.open(part, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), "w", encoding="utf-8") as f:
            json.dump(data, f)
        os.replace(part, path)

    def client(self) -> dict[str, str]:
        data = self._read(self.client_path)
        inner = data.get("installed") or data.get("web") or {}
        if inner.get("client_id") and inner.get("client_secret"):
            return {"client_id": inner["client_id"], "client_secret": inner["client_secret"]}
        return {}

    # ── HTTP ─────────────────────────────────────────────────────────────────

    def _http(self, method: str, url: str, *, params: Optional[dict[str, Any]] = None, form: Optional[dict] = None,
              body: Any = None, token: Optional[str] = None, raw: bool = False) -> Any:
        if params:
            url += ("&" if "?" in url else "?") + urllib.parse.urlencode(
                {k: v for k, v in params.items() if v is not None}, doseq=True)
        data = None
        headers = {"Accept": "application/json", "User-Agent": "AXYON-iPhone"}
        if form is not None:
            data = urllib.parse.urlencode(form).encode()
            headers["Content-Type"] = "application/x-www-form-urlencoded"
        elif body is not None:
            data = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        if token:
            headers["Authorization"] = "Bearer " + token
        req = urllib.request.Request(url, data=data, method=method, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=TIMEOUT) as resp:
                content = resp.read()
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", "replace")
            try:
                parsed = json.loads(detail)
                error = parsed.get("error")
                detail = (error.get("message") if isinstance(error, dict) else
                          parsed.get("error_description") or error) or detail
            except ValueError:
                pass
            raise GoogleError(f"Google said {exc.code}: {str(detail)[:400]}") from None
        except urllib.error.URLError as exc:
            raise GoogleError(f"Google could not be reached: {exc.reason}") from None
        if raw:
            return content
        return json.loads(content) if content.strip() else {}

    def _token(self) -> str:
        token = self._read(self.token_path)
        if not token.get("refresh_token") and not token.get("token"):
            raise GoogleError("Google is not connected on this iPhone: use google_account(do=status) first.")
        expiry = token.get("expiry") or ""
        try:
            expires = datetime.fromisoformat(expiry.replace("Z", "+00:00")).timestamp()
        except ValueError:
            expires = 0
        if token.get("token") and expires - 60 > _now():
            return token["token"]
        return self._refresh(token)

    def _refresh(self, token: dict[str, Any]) -> str:
        if not token.get("refresh_token"):
            raise GoogleError("Google's sign-in has expired: connect again with google_account(do=connect).")
        fresh = self._http("POST", token.get("token_uri") or self.endpoints["token"], form={
            "grant_type": "refresh_token", "refresh_token": token["refresh_token"],
            "client_id": token.get("client_id", ""), "client_secret": token.get("client_secret", "")})
        token["token"] = fresh["access_token"]
        token["expiry"] = (datetime.now(timezone.utc) + timedelta(seconds=int(fresh.get("expires_in") or 3600))
                           ).strftime("%Y-%m-%dT%H:%M:%SZ")
        self._write(self.token_path, token)
        return token["token"]

    def api(self, method: str, url: str, **kw: Any) -> Any:
        """A Google API call with the user's sign-in, refreshed once if Google refuses it."""
        try:
            return self._http(method, url, token=self._token(), **kw)
        except GoogleError as exc:
            if "said 401" not in str(exc):
                raise
            return self._http(method, url, token=self._refresh(self._read(self.token_path)), **kw)

    # ── The sign-in ──────────────────────────────────────────────────────────

    def status(self) -> dict[str, Any]:
        token = self._read(self.token_path)
        connected = bool(token.get("refresh_token"))
        answer: dict[str, Any] = {"client": bool(self.client()), "connected": connected}
        if connected:
            answer["account"] = token.get("account") or ""
            granted = set(token.get("scopes") or [])
            answer["services"] = [name for name, scopes in SERVICES.items() if set(scopes) <= granted]
        elif not answer["client"]:
            answer["next"] = SETUP_STEPS
        else:
            answer["next"] = "Call google_account(do=connect) and give the user the link it returns."
        return answer

    def set_client(self, path: str = "", client_id: str = "", client_secret: str = "") -> dict[str, Any]:
        """The user's OAuth client: the JSON Google gave them (sent in the chat), or its ID and secret pasted."""
        if path:
            real = os.path.realpath(path)
            if not real.startswith(os.path.realpath(self.inbox) + os.sep) or not os.path.isfile(real):
                raise GoogleError("Send the client's JSON file in the chat first: its path is given with it.")
            data = self._read(real)
            inner = data.get("installed") or data.get("web") or {}
            client_id, client_secret = inner.get("client_id", ""), inner.get("client_secret", "")
            if not client_id:
                raise GoogleError("This is not an OAuth client JSON from Google Cloud (no client_id in it).")
        client_id, client_secret = (client_id or "").strip(), (client_secret or "").strip()
        if not client_id.endswith(".apps.googleusercontent.com") or not client_secret:
            raise GoogleError("A Google OAuth client ID ends with .apps.googleusercontent.com, and comes with its "
                              "client secret.")
        self._write(self.client_path, {"installed": {
            "client_id": client_id, "client_secret": client_secret, "auth_uri": self.endpoints["auth"],
            "token_uri": self.endpoints["token"], "redirect_uris": ["http://localhost"]}})
        return {"ok": True, "next": "Now call google_account(do=connect)."}

    def connect(self, services: Any = None) -> dict[str, Any]:
        client = self.client()
        if not client:
            raise GoogleError(SETUP_STEPS)
        names = services or ["all"]
        if isinstance(names, str):
            names = [n.strip() for n in names.split(",")]
        if "all" in names:
            names = list(SERVICES)
        scopes = sorted({s for n in names for s in SERVICES.get(n, [])})
        if not scopes:
            raise GoogleError(f"Services are among: {', '.join(SERVICES)} or all.")
        verifier = _b64url(secrets.token_bytes(48))
        state = secrets.token_urlsafe(24)
        redirect = f"http://127.0.0.1:{self.port()}/"
        self._write(self.pending_path, {"state": state, "verifier": verifier, "redirect_uri": redirect,
                                        "scopes": scopes, "at": _now()})
        url = self.endpoints["auth"] + "?" + urllib.parse.urlencode({
            "client_id": client["client_id"], "redirect_uri": redirect, "response_type": "code",
            "scope": " ".join(scopes), "access_type": "offline", "prompt": "consent",
            "include_granted_scopes": "true", "state": state,
            "code_challenge": _b64url(hashlib.sha256(verifier.encode()).digest()), "code_challenge_method": "S256"})
        return {"url": url, "how": (
            "Give the user this link as it is, on a line of its own. They open it, sign in to Google and allow; "
            "AXYON finishes by itself and says so (check with google_account(do=status)). If Google says the app "
            "is not verified: Advanced → Go to (their app). If the page then fails to load, they copy its whole "
            "address and paste it here: call google_account(do=finish) with it. Error 403 access_denied: they add "
            "their account as a test user at https://console.cloud.google.com/auth/audience.")}

    def finish(self, pasted: str) -> dict[str, Any]:
        """The sign-in finished with what the browser was sent back with: the whole address, or the code alone."""
        pending = self._read(self.pending_path)
        if not pending:
            raise GoogleError("No sign-in is waiting: call google_account(do=connect) for a new link.")
        pasted = (pasted or "").strip()
        query = urllib.parse.parse_qs(urllib.parse.urlparse(pasted).query) if "code=" in pasted else {}
        if query.get("error"):
            raise GoogleError(f"Google did not allow it: {query['error'][0]}.")
        code = (query.get("code") or [pasted])[0]
        state = (query.get("state") or [""])[0]
        if state and state != pending.get("state"):
            raise GoogleError("This address is from an older link: use the newest one (google_account(do=connect)).")
        if not code:
            raise GoogleError("Paste the whole address the browser was sent back to (it has code= in it).")
        client = self.client()
        tokens = self._http("POST", self.endpoints["token"], form={
            "grant_type": "authorization_code", "code": code, "redirect_uri": pending["redirect_uri"],
            "client_id": client.get("client_id", ""), "client_secret": client.get("client_secret", ""),
            "code_verifier": pending["verifier"]})
        if not tokens.get("refresh_token"):
            raise GoogleError("Google gave no lasting sign-in: open the newest link again and allow every service.")
        granted = (tokens.get("scope") or " ".join(pending.get("scopes") or [])).split()
        token = {"type": "authorized_user", "client_id": client["client_id"],
                 "client_secret": client["client_secret"], "refresh_token": tokens["refresh_token"],
                 "token": tokens.get("access_token", ""), "token_uri": self.endpoints["token"], "scopes": granted,
                 "expiry": (datetime.now(timezone.utc) + timedelta(seconds=int(tokens.get("expires_in") or 3600))
                            ).strftime("%Y-%m-%dT%H:%M:%SZ")}
        self._write(self.token_path, token)
        try:
            os.remove(self.pending_path)
        except OSError:
            pass
        token["account"] = self._account()
        self._write(self.token_path, token)
        return {"ok": True, **self.status()}

    def _account(self) -> str:
        """The signed-in address, from Gmail or Drive, whichever was allowed."""
        for url, key in ((self.endpoints["gmail"] + "/profile", "emailAddress"),
                         (self.endpoints["drive"] + "/about?fields=user", "user")):
            try:
                found = self.api("GET", url)
                value = found.get(key)
                return value.get("emailAddress", "") if isinstance(value, dict) else str(value or "")
            except GoogleError:
                continue
        return ""

    def callback(self, query: str) -> tuple[int, str]:
        """Google's browser coming back to this host (http://127.0.0.1:<port>/?state=…&code=…): finishes the
        sign-in when it is the one waiting, and says so on the page."""
        params = urllib.parse.parse_qs(query)
        state = (params.get("state") or [""])[0]
        pending = self._read(self.pending_path)
        if not pending or not state or not secrets.compare_digest(state, str(pending.get("state", ""))):
            return 400, _page("This sign-in is not the one AXYON is waiting for.", "Ask AXYON for a new link.")
        try:
            self.finish("http://127.0.0.1/?" + query)
        except GoogleError as exc:
            return 400, _page("Google is not connected.", str(exc))
        return 200, _page("AXYON is connected to Google.", "You can close this page and go back to the chat.")

    def disconnect(self) -> dict[str, Any]:
        token = self._read(self.token_path)
        if token.get("refresh_token"):
            try:
                self._http("POST", self.endpoints["revoke"], form={"token": token["refresh_token"]})
            except GoogleError:
                pass
        for path in (self.token_path, self.pending_path):
            try:
                os.remove(path)
            except OSError:
                pass
        return {"ok": True, "connected": False}

    # ── Gmail ────────────────────────────────────────────────────────────────

    def gmail_search(self, query: str, limit: int = 10) -> dict[str, Any]:
        base = self.endpoints["gmail"]
        found = self.api("GET", base + "/messages", params={"q": query or "", "maxResults": max(1, min(limit, 25))})
        messages = []
        for item in found.get("messages") or []:
            m = self.api("GET", f"{base}/messages/{item['id']}", params={
                "format": "metadata", "metadataHeaders": ["From", "To", "Subject", "Date"]})
            headers = {h["name"].lower(): h["value"] for h in (m.get("payload") or {}).get("headers") or []}
            messages.append({"id": m.get("id"), "threadId": m.get("threadId"), "from": headers.get("from", ""),
                             "subject": headers.get("subject", ""), "date": headers.get("date", ""),
                             "snippet": html.unescape(m.get("snippet", "")),
                             "unread": "UNREAD" in (m.get("labelIds") or [])})
        return {"messages": messages, "more": bool(found.get("nextPageToken"))}

    def gmail_read(self, message_id: str) -> dict[str, Any]:
        m = self.api("GET", f"{self.endpoints['gmail']}/messages/{message_id}", params={"format": "full"})
        payload = m.get("payload") or {}
        headers = {h["name"].lower(): h["value"] for h in payload.get("headers") or []}
        text, page, files = [], [], []

        def walk(part: dict[str, Any]) -> None:
            mime = part.get("mimeType", "")
            data = (part.get("body") or {}).get("data")
            if part.get("filename"):
                files.append({"name": part["filename"], "type": mime,
                              "bytes": (part.get("body") or {}).get("size", 0)})
            elif data and mime == "text/plain":
                text.append(base64.urlsafe_b64decode(data + "===").decode("utf-8", "replace"))
            elif data and mime == "text/html":
                page.append(base64.urlsafe_b64decode(data + "===").decode("utf-8", "replace"))
            for sub in part.get("parts") or []:
                walk(sub)

        walk(payload)
        body = "\n".join(text) or _strip_html("\n".join(page))
        return {"id": m.get("id"), "threadId": m.get("threadId"), "from": headers.get("from", ""),
                "to": headers.get("to", ""), "cc": headers.get("cc", ""), "subject": headers.get("subject", ""),
                "date": headers.get("date", ""), "body": body[:30_000], "attachments": files}

    def _mail(self, to: str, subject: str, body: str, cc: str = "", reply_to: Optional[dict[str, Any]] = None
              ) -> dict[str, Any]:
        message = EmailMessage()
        message["To"] = to
        if cc:
            message["Cc"] = cc
        message["Subject"] = subject
        if reply_to:
            message["In-Reply-To"] = reply_to["messageId"]
            message["References"] = (reply_to.get("references", "") + " " + reply_to["messageId"]).strip()
        message.set_content(body)
        raw = {"raw": _b64url(message.as_bytes())}
        if reply_to and reply_to.get("threadId"):
            raw["threadId"] = reply_to["threadId"]
        return raw

    def gmail_draft(self, to: str, subject: str, body: str, cc: str = "") -> dict[str, Any]:
        draft = self.api("POST", self.endpoints["gmail"] + "/drafts",
                         body={"message": self._mail(to, subject, body, cc)})
        return {"ok": True, "draft": draft.get("id"),
                "said": "Saved as a draft in Gmail: the user sends it from Gmail when they want."}

    def _approved(self, kind: str, to: str, subject: str, body: str, cc: str = "") -> bool:
        """The user's own tap on Send, on the e-mail as the app shows it in full; nothing else counts."""
        if self.bridge is None:
            return False
        answer = self.bridge.ask("iphone", {"action": "mail", "kind": kind, "to": to, "cc": cc,
                                            "subject": subject, "body": body}, wait=300)
        return isinstance(answer, dict) and answer.get("send") is True

    def gmail_send(self, to: str, subject: str, body: str, cc: str = "") -> dict[str, Any]:
        if not to or not subject:
            raise GoogleError("An e-mail needs `to` and `subject`.")
        if not self._approved("send", to, subject, body, cc):
            return {"sent": False, "said": "The user did not tap Send in AXYON: nothing was sent."}
        sent = self.api("POST", self.endpoints["gmail"] + "/messages/send", body=self._mail(to, subject, body, cc))
        return {"sent": True, "id": sent.get("id")}

    def gmail_reply(self, message_id: str, body: str) -> dict[str, Any]:
        m = self.api("GET", f"{self.endpoints['gmail']}/messages/{message_id}", params={
            "format": "metadata", "metadataHeaders": ["From", "Reply-To", "Subject", "Message-ID", "References"]})
        headers = {h["name"].lower(): h["value"] for h in (m.get("payload") or {}).get("headers") or []}
        to = headers.get("reply-to") or headers.get("from", "")
        subject = headers.get("subject", "")
        if not subject.lower().startswith("re:"):
            subject = "Re: " + subject
        if not self._approved("reply", to, subject, body):
            return {"sent": False, "said": "The user did not tap Send in AXYON: nothing was sent."}
        raw = self._mail(to, subject, body, reply_to={"messageId": headers.get("message-id", ""),
                                                      "references": headers.get("references", ""),
                                                      "threadId": m.get("threadId")})
        sent = self.api("POST", self.endpoints["gmail"] + "/messages/send", body=raw)
        return {"sent": True, "id": sent.get("id")}

    # ── Sheets ───────────────────────────────────────────────────────────────

    def sheets_tabs(self, sheet: str) -> dict[str, Any]:
        found = self.api("GET", f"{self.endpoints['sheets']}/{spreadsheet_id(sheet)}",
                         params={"fields": "properties.title,sheets.properties(title,gridProperties)"})
        return {"title": (found.get("properties") or {}).get("title", ""),
                "tabs": [{"title": s["properties"]["title"],
                          "rows": (s["properties"].get("gridProperties") or {}).get("rowCount"),
                          "columns": (s["properties"].get("gridProperties") or {}).get("columnCount")}
                         for s in found.get("sheets") or []]}

    def sheets_read(self, sheet: str, cells: str) -> dict[str, Any]:
        found = self.api("GET", f"{self.endpoints['sheets']}/{spreadsheet_id(sheet)}/values/"
                         + urllib.parse.quote(cells, safe=""), params={"valueRenderOption": "FORMATTED_VALUE"})
        return {"range": found.get("range", cells), "values": found.get("values") or []}

    def sheets_write(self, sheet: str, cells: str, values: list[list[Any]]) -> dict[str, Any]:
        found = self.api("PUT", f"{self.endpoints['sheets']}/{spreadsheet_id(sheet)}/values/"
                         + urllib.parse.quote(cells, safe=""), params={"valueInputOption": "USER_ENTERED"},
                         body={"range": cells, "values": values})
        return {"ok": True, "updated": found.get("updatedRange"), "cells": found.get("updatedCells")}

    def sheets_append(self, sheet: str, cells: str, values: list[list[Any]]) -> dict[str, Any]:
        found = self.api("POST", f"{self.endpoints['sheets']}/{spreadsheet_id(sheet)}/values/"
                         + urllib.parse.quote(cells, safe="") + ":append",
                         params={"valueInputOption": "USER_ENTERED", "insertDataOption": "INSERT_ROWS"},
                         body={"values": values})
        updates = found.get("updates") or {}
        return {"ok": True, "updated": updates.get("updatedRange"), "rows": updates.get("updatedRows")}

    # ── Drive and Docs ───────────────────────────────────────────────────────

    def drive_search(self, query: str, limit: int = 20) -> dict[str, Any]:
        text = (query or "").strip()
        if text and not re.search(r"\b(contains|=|in parents|mimeType)\b", text):
            text = "name contains '" + text.replace("\\", "\\\\").replace("'", "\\'") + "'"
        q = " and ".join(p for p in (text, "trashed = false") if p)
        found = self.api("GET", self.endpoints["drive"] + "/files", params={
            "q": q, "pageSize": max(1, min(limit, 50)), "orderBy": "modifiedTime desc",
            "fields": "files(id,name,mimeType,modifiedTime,size,webViewLink)"})
        return {"files": found.get("files") or []}

    EXPORTS = {"application/vnd.google-apps.document": ("text/plain", ".txt"),
               "application/vnd.google-apps.spreadsheet": ("text/csv", ".csv"),
               "application/vnd.google-apps.presentation": ("text/plain", ".txt")}

    def drive_read(self, file: str) -> dict[str, Any]:
        """A Drive file's text: Docs and Slides as text, a Sheet as CSV (its first tab), text files as they are,
        PDFs, Word and Excel files through the iPhone (they are saved in the inbox for iphone_files too)."""
        file_id = drive_id(file)
        meta = self.api("GET", f"{self.endpoints['drive']}/files/{file_id}",
                        params={"fields": "id,name,mimeType,size,webViewLink"})
        mime, name = meta.get("mimeType", ""), meta.get("name", file_id)
        if mime in self.EXPORTS:
            content = self.api("GET", f"{self.endpoints['drive']}/files/{file_id}/export",
                               params={"mimeType": self.EXPORTS[mime][0]}, raw=True)
            text = content.decode("utf-8", "replace")
            return {"name": name, "type": mime, "text": text[:40_000], "length": len(text),
                    "link": meta.get("webViewLink")}
        if mime.startswith("application/vnd.google-apps"):
            return {"name": name, "type": mime, "link": meta.get("webViewLink"),
                    "said": "This kind of Google file has no text to read: give the user its link."}
        if int(meta.get("size") or 0) > 25 * 1024 * 1024:
            return {"name": name, "type": mime, "link": meta.get("webViewLink"), "said": "Too large to read here."}
        content = self.api("GET", f"{self.endpoints['drive']}/files/{file_id}", params={"alt": "media"}, raw=True)
        os.makedirs(self.inbox, exist_ok=True)
        safe = re.sub(r"[^\w.\- ]", "_", name)[:120] or file_id
        path = os.path.join(self.inbox, f"drive-{file_id[:8]}-{safe}")
        with open(path, "wb") as f:
            f.write(content)
        from .attachments import document_text

        text = None
        if path.lower().endswith(".pdf") and self.bridge is not None:
            answer = self.bridge.ask("iphone", {"action": "pdf_text", "path": path}, wait=90)
            if isinstance(answer, dict) and answer.get("text") is not None:
                text = str(answer["text"])
                with open(path + ".txt.extracted", "w", encoding="utf-8") as f:
                    f.write(text)
        if text is None:
            try:
                text = document_text(path)
            except OSError:
                text = None
        if text is None and mime.startswith("text/"):
            text = content.decode("utf-8", "replace")
        answer = {"name": name, "type": mime, "path": path, "link": meta.get("webViewLink")}
        if text is None:
            answer["said"] = "Saved on the iPhone, but its text cannot be read here."
        else:
            answer.update(text=text[:40_000], length=len(text))
        return answer

    # ── Calendar ─────────────────────────────────────────────────────────────

    def calendar_list(self, start: str = "", end: str = "") -> dict[str, Any]:
        now = datetime.now().astimezone()
        begin = _when(start) or now.replace(hour=0, minute=0, second=0, microsecond=0)
        until = _when(end) or begin + timedelta(days=7)
        found = self.api("GET", self.endpoints["calendar"] + "/calendars/primary/events", params={
            "timeMin": begin.isoformat(), "timeMax": until.isoformat(), "singleEvents": "true",
            "orderBy": "startTime", "maxResults": 50})
        return {"events": [{"id": e.get("id"), "title": e.get("summary", ""),
                            "start": (e.get("start") or {}).get("dateTime") or (e.get("start") or {}).get("date"),
                            "end": (e.get("end") or {}).get("dateTime") or (e.get("end") or {}).get("date"),
                            "location": e.get("location", ""), "link": e.get("htmlLink", "")}
                           for e in found.get("items") or []]}

    def calendar_add(self, title: str, start: str, end: str = "", location: str = "", notes: str = ""
                     ) -> dict[str, Any]:
        begin = _when(start)
        if not title or begin is None:
            raise GoogleError("An event needs a title and a start (ISO 8601, e.g. 2026-11-03T10:00).")
        until = _when(end) or begin + timedelta(hours=1)
        event = self.api("POST", self.endpoints["calendar"] + "/calendars/primary/events", body={
            "summary": title, "location": location or None, "description": notes or None,
            "start": {"dateTime": begin.isoformat()}, "end": {"dateTime": until.isoformat()}})
        return {"ok": True, "id": event.get("id"), "link": event.get("htmlLink", "")}

    # ── The tools ────────────────────────────────────────────────────────────

    def _run(self, work: Callable[[], dict[str, Any]]) -> str:
        try:
            return json.dumps(work(), ensure_ascii=False)
        except GoogleError as exc:
            return json.dumps({"error": str(exc)}, ensure_ascii=False)
        except Exception as exc:  # a malformed answer from Google, a full disk
            return json.dumps({"error": f"Google: {exc.__class__.__name__}: {exc}"[:500]}, ensure_ascii=False)

    def account_tool(self, args: dict[str, Any], **_kw) -> str:
        do = args.get("do")
        actions = {
            "status": self.status,
            "client": lambda: self.set_client(args.get("path", ""), args.get("client_id", ""),
                                              args.get("client_secret", "")),
            "connect": lambda: self.connect(args.get("services")),
            "finish": lambda: self.finish(args.get("pasted", "")),
            "disconnect": self.disconnect,
        }
        return self._run(actions.get(do) or (lambda: {"error": "do: status, client, connect, finish, disconnect"}))

    def gmail_tool(self, args: dict[str, Any], **_kw) -> str:
        do = args.get("do")
        actions = {
            "search": lambda: self.gmail_search(args.get("query", ""), int(args.get("max") or 10)),
            "read": lambda: self.gmail_read(args.get("id", "")),
            "draft": lambda: self.gmail_draft(args.get("to", ""), args.get("subject", ""), args.get("body", ""),
                                              args.get("cc", "")),
            "send": lambda: self.gmail_send(args.get("to", ""), args.get("subject", ""), args.get("body", ""),
                                            args.get("cc", "")),
            "reply": lambda: self.gmail_reply(args.get("id", ""), args.get("body", "")),
        }
        return self._run(actions.get(do) or (lambda: {"error": "do: search, read, draft, send, reply"}))

    def sheets_tool(self, args: dict[str, Any], **_kw) -> str:
        do, sheet, cells = args.get("do"), args.get("spreadsheet", ""), args.get("range", "")
        values = args.get("values") or []
        actions = {
            "tabs": lambda: self.sheets_tabs(sheet),
            "read": lambda: self.sheets_read(sheet, cells),
            "write": lambda: self.sheets_write(sheet, cells, values),
            "append": lambda: self.sheets_append(sheet, cells, values),
        }
        return self._run(actions.get(do) or (lambda: {"error": "do: tabs, read, write, append"}))

    def drive_tool(self, args: dict[str, Any], **_kw) -> str:
        do = args.get("do")
        actions = {
            "search": lambda: self.drive_search(args.get("query", ""), int(args.get("max") or 20)),
            "read": lambda: self.drive_read(args.get("id", "")),
        }
        return self._run(actions.get(do) or (lambda: {"error": "do: search, read"}))

    def calendar_tool(self, args: dict[str, Any], **_kw) -> str:
        do = args.get("do")
        actions = {
            "list": lambda: self.calendar_list(args.get("from", ""), args.get("to", "")),
            "add": lambda: self.calendar_add(args.get("title", ""), args.get("start", ""), args.get("end", ""),
                                             args.get("location", ""), args.get("notes", "")),
        }
        return self._run(actions.get(do) or (lambda: {"error": "do: list, add"}))

    def register(self, registry) -> None:
        for schema, handler, emoji in ((ACCOUNT, self.account_tool, "🔑"), (GMAIL, self.gmail_tool, "✉️"),
                                       (SHEETS, self.sheets_tool, "📊"), (DRIVE, self.drive_tool, "📁"),
                                       (CALENDAR, self.calendar_tool, "📅")):
            registry.register(name=schema["name"], toolset=TOOLSET, schema=schema, handler=handler, emoji=emoji)


def _when(value: str) -> Optional[datetime]:
    """An ISO 8601 date or time, the iPhone's own time zone when none is given."""
    if not value:
        return None
    try:
        moment = datetime.fromisoformat(value.strip().replace("Z", "+00:00"))
    except ValueError:
        return None
    return moment if moment.tzinfo else moment.replace(tzinfo=datetime.now().astimezone().tzinfo)


def _strip_html(page: str) -> str:
    text = re.sub(r"(?is)<(script|style).*?</\1>", " ", page)
    text = re.sub(r"(?i)<br\s*/?>|</p>|</div>|</tr>", "\n", text)
    text = re.sub(r"<[^>]+>", " ", text)
    return re.sub(r"[ \t]+", " ", html.unescape(text)).strip()


def _page(title: str, detail: str) -> str:
    return ("<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'>"
            "<title>AXYON</title><body style='font:17px -apple-system,sans-serif;background:#05070d;color:#e8eefc;"
            "padding:48px 24px;text-align:center'><h2>" + html.escape(title) + "</h2><p style='color:#9aa6c0'>"
            + html.escape(detail) + "</p></body>")


_S = {"type": "string"}
_GRID = {"type": "array", "items": {"type": "array", "items": {}},
         "description": "Rows of cells, e.g. [[\"2026-10-29\", \"BTLG11\", 10, 101.5]]."}

ACCOUNT = {"name": "google_account", "description": (
    "The user's Google account on this iPhone (Gmail, Sheets, Drive, Docs, Calendar): `do`=status (connected? what "
    "is next), client (their OAuth client: `path` of the JSON they sent in the chat, or `client_id` and "
    "`client_secret` pasted), connect (a link to sign in; `services` among email, calendar, drive, sheets, docs, "
    "contacts or all), finish (`pasted`: the address the browser was sent back to, when it did not finish by itself), "
    "disconnect. Never ask for their Google password: the sign-in is Google's own page."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["status", "client", "connect", "finish", "disconnect"]},
        "path": _S, "client_id": _S, "client_secret": _S, "services": {"type": "array", "items": _S},
        "pasted": _S}, "required": ["do"]}}

GMAIL = {"name": "gmail", "description": (
    "The user's Gmail: `do`=search (`query` in Gmail's own syntax, e.g. is:unread newer_than:2d, "
    "from:x@y.com subject:BTLG11; `max`), read (`id`), draft (`to`, `subject`, `body`, `cc`: saved in Gmail's "
    "drafts), send (same fields) or reply (`id`, `body`). Send and reply show the e-mail to the user in AXYON and "
    "go only when they tap Send there."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["search", "read", "draft", "send", "reply"]},
        "query": _S, "max": {"type": "integer"}, "id": _S, "to": _S, "cc": _S, "subject": _S, "body": _S},
        "required": ["do"]}}

SHEETS = {"name": "google_sheets", "description": (
    "The user's Google Sheets: `do`=tabs (the tabs of `spreadsheet`), read (`range`, e.g. AXYON!A1:H50), write "
    "(`range` and `values`, replacing those cells) or append (`values` as new rows after the table in `range`, e.g. "
    "Aportes!A:F). `spreadsheet` is its id or link. Formulas and numbers are typed as the user would type them."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["tabs", "read", "write", "append"]},
        "spreadsheet": _S, "range": _S, "values": _GRID}, "required": ["do", "spreadsheet"]}}

DRIVE = {"name": "google_drive", "description": (
    "The user's Google Drive and Docs: `do`=search (`query`: words of a file's name, or Drive's own query syntax; "
    "`max`), read (`id`, or a link: a Doc as text, a Sheet as CSV, PDFs, Word and Excel files read on the iPhone)."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["search", "read"]}, "query": _S, "id": _S, "max": {"type": "integer"}},
        "required": ["do"]}}

CALENDAR = {"name": "google_calendar", "description": (
    "The user's Google Calendar (primary): `do`=list (`from`, `to` as ISO 8601; this week by default) or add "
    "(`title`, `start`, `end`, `location`, `notes`). The iPhone's own calendar (iphone_calendar) shows it too when "
    "the account is on the iPhone."),
    "parameters": {"type": "object", "properties": {
        "do": {"type": "string", "enum": ["list", "add"]}, "from": _S, "to": _S, "title": _S, "start": _S,
        "end": _S, "location": _S, "notes": _S}, "required": ["do"]}}
