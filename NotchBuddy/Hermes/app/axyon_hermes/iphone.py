"""The iPhone as Hermes' body: its tools for what the app does with the iPhone itself (HermesArms.swift answers them
over the bridge). Notifications, reminders, the calendar, contacts, opening apps and links, the user's Shortcuts, the
clipboard, the iPhone's state and place, the flashlight, music and speaking.

Nothing here sends a message, an e-mail or calls anyone: iphone_open opens the app concerned with everything filled
in, and the user sends."""

from __future__ import annotations

import json
from typing import Any

TOOLSET = "coucou"


def _schema(name: str, description: str, properties: dict[str, Any] | None = None,
            required: list[str] | None = None) -> dict[str, Any]:
    parameters: dict[str, Any] = {"type": "object", "properties": properties or {}}
    if required:
        parameters["required"] = required
    return {"name": name, "description": description, "parameters": parameters}


_S = {"type": "string"}

TOOLS = [
    ("iphone_notify", "notify", _schema(
        "iphone_notify",
        "A notification on the user's iPhone, now or at a time: a reminder that comes even with the app closed. "
        "`at` is a local date and time (ISO 8601, e.g. 2026-10-28T18:30), or `in_seconds` from now.",
        {"title": _S, "body": _S, "at": _S, "in_seconds": {"type": "number"}}, ["title"])),
    ("iphone_reminders", "reminders", _schema(
        "iphone_reminders",
        "The user's Reminders on the iPhone: `do`=list (the open ones), add (title, optional due as ISO 8601 and "
        "notes), or complete (id from list).",
        {"do": {"type": "string", "enum": ["list", "add", "complete"]}, "title": _S, "due": _S, "notes": _S, "id": _S},
        ["do"])),
    ("iphone_calendar", "calendar", _schema(
        "iphone_calendar",
        "The user's Calendar on the iPhone: `do`=list the events between `from` and `to` (ISO 8601; this week by "
        "default), or add one (title, start, end, location, notes).",
        {"do": {"type": "string", "enum": ["list", "add"]}, "from": _S, "to": _S, "title": _S, "start": _S,
         "end": _S, "location": _S, "notes": _S}, ["do"])),
    ("iphone_contacts", "contacts", _schema(
        "iphone_contacts", "Finds people in the user's Contacts by name: their phones and e-mails.",
        {"query": _S}, ["query"])),
    ("iphone_open", "open", _schema(
        "iphone_open",
        "Opens a link or an app on the iPhone, filled in, for the user to finish: https://… pages, tel:+55…, "
        "sms:+55…&body=…, mailto:x@y?subject=…&body=…, whatsapp://send?phone=55…&text=…, maps://?q=…, "
        "spotify:search:…, youtube://…, music://…, calshow:, app-settings: (AXYON's settings). Messages, mails "
        "and calls are sent or placed by the user, never by you.",
        {"url": _S}, ["url"])),
    ("iphone_shortcut", "shortcut", _schema(
        "iphone_shortcut",
        "Runs one of the user's Shortcuts by its exact name, with an optional text input. Shortcuts can do almost "
        "anything on an iPhone (alarms, Focus, Wi-Fi, HomeKit scenes, messages): ask the user which they have.",
        {"name": _S, "input": _S}, ["name"])),
    ("iphone_clipboard", "clipboard", _schema(
        "iphone_clipboard", "The iPhone's clipboard: `do`=get its text, or set it to `text`.",
        {"do": {"type": "string", "enum": ["get", "set"]}, "text": _S}, ["do"])),
    ("iphone_status", "status", _schema(
        "iphone_status", "The iPhone now: battery, charging, low power mode, free storage, model, system, time zone.")),
    ("iphone_location", "location", _schema(
        "iphone_location", "Where the iPhone is: coordinates and the city, region and street.")),
    ("iphone_torch", "torch", _schema(
        "iphone_torch", "Turns the iPhone's flashlight on or off (`on`; toggles when not given).",
        {"on": {"type": "boolean"}})),
    ("iphone_music", "music", _schema(
        "iphone_music", "The Music app: `do`=play, pause, next, previous, or now (what is playing).",
        {"do": {"type": "string", "enum": ["play", "pause", "next", "previous", "now"]}}, ["do"])),
    ("iphone_speak", "speak", _schema(
        "iphone_speak", "Says a short text aloud on the iPhone.", {"text": _S}, ["text"])),
    ("iphone_alerts", "alerts", _schema(
        "iphone_alerts",
        "The superpowers' alerts with AXYON closed, from the official data: an asset going under the user's own "
        "ceiling or back above it, a strong day for an asset or the portfolio, an asset under one of its ceilings, "
        "and the morning's day. `do`=status, on, off, or look (check now and notify what is worth it). iOS decides "
        "when it checks with AXYON closed (a few times a day; Background App Refresh must be on).",
        {"do": {"type": "string", "enum": ["status", "on", "off", "look"]}}, ["do"])),
]

# The model's argument names that the app reads under another name.
_RENAMED = {"in_seconds": "inSeconds"}


class IPhone:
    def __init__(self, bridge):
        self.bridge = bridge

    def _handler(self, action: str):
        def run(args: dict[str, Any], **_kw) -> str:
            request = {"action": action}
            for key, value in (args or {}).items():
                if value is not None:
                    request[_RENAMED.get(key, key)] = value
            answer = self.bridge.ask("iphone", request)
            return json.dumps(answer if answer is not None else {"error": "No answer from the iPhone."},
                              ensure_ascii=False)
        return run

    def register(self, registry) -> None:
        for name, action, schema in TOOLS:
            registry.register(name=name, toolset=TOOLSET, schema=schema, handler=self._handler(action), emoji="📱")
