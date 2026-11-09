"""AXYON's routines on the iPhone: a name, what starts it, and steps done one after the other.

What starts one:
  * the user: a tap in the Home tab, Siri ("rodar Boa noite no AXYON"), the Shortcuts app and its automations (a time,
    arriving or leaving, an NFC tag, a Wi-Fi, CarPlay, an alarm stopped…), through the app's App Intents;
  * a place: arriving at it or leaving it, which the app watches with iOS' own region monitoring (`arrive`, `leave`);
  * a time of day on chosen weekdays (`time`): run by the host's clock while AXYON runs, by iOS' background refresh
    with AXYON closed (when iOS wakes it), and to the minute by a Shortcuts automation that runs the routine;
  * AXYON itself, asked in the chat (home_routines).

The steps: a device of the home (home.py, any action it lists), every light / TV / fan of the home at once, the
iPhone (a notification, a reminder, speaking, the flashlight, music, a link or app, one of the user's Shortcuts),
a pause, and a question to AXYON Core whose answer comes as a notification ("my day: the agenda, the reminders, how
the portfolio opened").

A routine the user makes in the app is theirs: its steps run without asking again, plugs included. One AXYON makes
or changes in the chat is shown to the owner first and kept only with their Face ID (the bridge's "confirm"). Nothing
here unlocks a door, disarms an alarm or sends a message: the home's own lists (home.py) never offer those."""

from __future__ import annotations

import json
import logging
import os
import re
import secrets
import threading
import time
import unicodedata
from datetime import datetime, timedelta
from typing import Any, Callable, Optional

log = logging.getLogger("axyon.routines")

TOOLSET = "coucou"
MAX_ROUTINES = 40
MAX_STEPS = 24
MAX_PAUSE = 120.0
# A time routine whose minute passed while AXYON slept still runs within this window, never later.
LATE = timedelta(hours=2)
TICK = 30.0
TRIGGERS = ("manual", "arrive", "leave", "time")
PHONE_STEPS = ("notify", "reminder", "speak", "torch", "music", "open", "shortcut")
MUSIC = ("play", "pause", "next", "previous")
EVERY_KIND = ("light", "tv", "fan", "switch")
# Turning every plug on at once powers what is unknown: only off.
EVERY_ACTION = {"light": ("turn_on", "turn_off"), "tv": ("turn_off",), "fan": ("turn_on", "turn_off"),
                "switch": ("turn_off",)}
DEVICE_ACTIONS = {"turn_on", "turn_off", "toggle", "set_brightness", "set_color", "set_volume", "volume_up",
                  "volume_down", "mute", "unmute", "channel_up", "channel_down", "open_app", "key", "play_pause",
                  "next", "previous", "open", "close", "stop", "run", "preset", "set_temperature"}


class RoutineError(Exception):
    """Why a routine cannot be kept or run, in words the user (and the model) can act on."""


def _fold(text: str) -> str:
    text = unicodedata.normalize("NFKD", text or "")
    text = "".join(c for c in text if not unicodedata.combining(c))
    return re.sub(r"\s+", " ", text).strip().casefold()


def _text(value: Any, limit: int) -> str:
    return re.sub(r"[\x00-\x08\x0b-\x1f]", "", str(value or "")).strip()[:limit]


# ── What a routine is ────────────────────────────────────────────────────────


def clean_trigger(t: Any) -> dict[str, Any]:
    t = t if isinstance(t, dict) else {"kind": t or "manual"}
    kind = str(t.get("kind") or "manual").lower()
    if kind not in TRIGGERS:
        raise RoutineError(f"A routine starts by hand, on arriving at or leaving a place, or at a time ({kind}?).")
    if kind == "manual":
        return {"kind": "manual"}
    if kind == "time":
        m = re.fullmatch(r"(\d{1,2})[:h](\d{2})", str(t.get("at") or "").strip())
        if not m or int(m.group(1)) > 23 or int(m.group(2)) > 59:
            raise RoutineError("At what time? HH:MM, like 07:30.")
        days = sorted({int(d) for d in (t.get("days") or []) if str(d).isdigit() and 1 <= int(d) <= 7})
        return {"kind": "time", "at": f"{int(m.group(1)):02d}:{m.group(2)}", "days": days}
    place = t.get("place") if isinstance(t.get("place"), dict) else {}
    try:
        lat, lon = float(place.get("lat")), float(place.get("lon"))
    except (TypeError, ValueError):
        raise RoutineError("Which place? The app sets it on the map (or 'where I am now').") from None
    if not (-90 <= lat <= 90 and -180 <= lon <= 180) or (lat == 0 and lon == 0):
        raise RoutineError("That place is not on the map.")
    radius = max(100.0, min(2000.0, float(place.get("radius") or 150)))
    return {"kind": kind, "place": {"name": _text(place.get("name"), 40) or "Casa", "lat": round(lat, 6),
                                    "lon": round(lon, 6), "radius": round(radius)}}


def clean_step(s: Any) -> dict[str, Any]:
    if not isinstance(s, dict):
        raise RoutineError("Each step is an object with `do`.")
    do = str(s.get("do") or "").lower()
    if do == "device":
        device = _text(s.get("device"), 120)
        action = str(s.get("action") or "").strip().lower()
        if not device or action not in DEVICE_ACTIONS:
            raise RoutineError(f"A device step needs the device and one of its actions ({action or 'none'}?).")
        step: dict[str, Any] = {"do": "device", "device": device, "action": action}
        if s.get("value") not in (None, ""):
            step["value"] = s["value"] if isinstance(s["value"], (int, float)) else _text(s["value"], 60)
        if s.get("name"):
            step["name"] = _text(s["name"], 60)
        return step
    if do == "every":
        kind, action = str(s.get("kind") or "").lower(), str(s.get("action") or "").lower()
        if kind not in EVERY_KIND or action not in EVERY_ACTION[kind]:
            raise RoutineError("Every device of a kind: lights on or off, TVs off, fans on or off, plugs off.")
        return {"do": "every", "kind": kind, "action": action}
    if do == "iphone":
        action = str(s.get("action") or "").lower()
        if action not in PHONE_STEPS:
            raise RoutineError(f"The iPhone can: {', '.join(PHONE_STEPS)}.")
        step = {"do": "iphone", "action": action}
        if action == "notify":
            step.update(title=_text(s.get("title"), 80), body=_text(s.get("body") or s.get("text"), 400))
            if not step["body"]:
                raise RoutineError("What does the notification say?")
        elif action == "reminder":
            step["title"] = _text(s.get("title") or s.get("text"), 120)
            if not step["title"]:
                raise RoutineError("What is the reminder?")
            if s.get("inMinutes") not in (None, ""):
                step["inMinutes"] = max(1, min(60 * 24 * 7, int(float(s["inMinutes"]))))
        elif action == "speak":
            step["text"] = _text(s.get("text"), 300)
            if not step["text"]:
                raise RoutineError("What does AXYON say?")
        elif action == "torch":
            step["on"] = bool(s.get("on", True))
        elif action == "music":
            what = str(s.get("what") or s.get("music") or "play").lower()
            step["what"] = what if what in MUSIC else "play"
        elif action == "open":
            url = _text(s.get("url"), 1000)
            if not re.match(r"^[a-z][a-z0-9+.\-]*:", url, re.I) or url.lower().startswith(("javascript:", "file:")):
                raise RoutineError("Which link or app? https://…, spotify:, whatsapp://…")
            step["url"] = url
        elif action == "shortcut":
            step["name"] = _text(s.get("name"), 80)
            if not step["name"]:
                raise RoutineError("Which of your Shortcuts? Its exact name.")
            if s.get("input"):
                step["input"] = _text(s["input"], 400)
        return step
    if do == "pause":
        seconds = float(s.get("seconds") or 0)
        if not 1 <= seconds <= MAX_PAUSE:
            raise RoutineError(f"A pause is 1 to {int(MAX_PAUSE)} seconds.")
        return {"do": "pause", "seconds": round(seconds)}
    if do == "ask":
        prompt = _text(s.get("prompt"), 2000)
        if not prompt:
            raise RoutineError("What does AXYON Core look into?")
        return {"do": "ask", "prompt": prompt}
    raise RoutineError(f"A step is device, every, iphone, pause or ask ({do or 'none'}?).")


def clean(body: dict[str, Any], old: Optional[dict[str, Any]] = None) -> dict[str, Any]:
    name = _text(body.get("name"), 60)
    if not name:
        raise RoutineError("Give the routine a name (it is how you and Siri call it).")
    steps = body.get("steps")
    if not isinstance(steps, list) or not steps:
        raise RoutineError("A routine needs at least one step.")
    if len(steps) > MAX_STEPS:
        raise RoutineError(f"At most {MAX_STEPS} steps.")
    icon = str(body.get("icon") or (old or {}).get("icon") or "sparkles")
    if not re.fullmatch(r"[a-z0-9.]{1,40}", icon):
        icon = "sparkles"
    routine = {
        "id": (old or {}).get("id") or "r-" + secrets.token_hex(5),
        "name": name,
        "icon": icon,
        "enabled": bool(body.get("enabled", (old or {}).get("enabled", True))),
        "trigger": clean_trigger(body.get("trigger")),
        "steps": [clean_step(s) for s in steps],
        "by": (old or {}).get("by") or body.get("by") or "app",
    }
    for kept in ("lastRun", "firedSlot"):
        if old and kept in old:
            routine[kept] = old[kept]
    return routine


def describe_trigger(t: dict[str, Any]) -> str:
    kind = t.get("kind")
    if kind == "time":
        days = t.get("days") or []
        names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        when = "every day" if not days or len(days) == 7 else ", ".join(names[d - 1] for d in days)
        return f"at {t.get('at')}, {when}"
    if kind in ("arrive", "leave"):
        place = t.get("place") or {}
        return f"on {'arriving at' if kind == 'arrive' else 'leaving'} {place.get('name') or 'a place'}"
    return "by hand, Siri or Shortcuts"


def describe_step(s: dict[str, Any]) -> str:
    do = s.get("do")
    if do == "device":
        value = f" ({s['value']})" if "value" in s else ""
        return f"{s['action']} {s.get('name') or s['device']}{value}"
    if do == "every":
        return f"{s['action']} every {s['kind']}"
    if do == "pause":
        return f"wait {s['seconds']} s"
    if do == "ask":
        return f"ask AXYON Core: {s['prompt'][:80]}"
    detail = s.get("body") or s.get("title") or s.get("text") or s.get("url") or s.get("name") or s.get("what") or ""
    return f"iPhone {s['action']} {detail}".strip()


# ── The routines ─────────────────────────────────────────────────────────────


class Routines:
    def __init__(self, folder: str, house: Any, bridge: Any = None,
                 ask: Optional[Callable[[str, str, float], str]] = None):
        self.path = os.path.join(folder, "axyon-routines.json")
        self.house = house
        self.bridge = bridge
        # AXYON Core's answer to a question (host.Host.ask): set once Hermes is up.
        self.ask = ask
        self.lock = threading.RLock()
        self.running: dict[str, dict[str, Any]] = {}
        self.thread: Optional[threading.Thread] = None

    # ── What is kept ─────────────────────────────────────────────────────────

    def load(self) -> list[dict[str, Any]]:
        try:
            with open(self.path, encoding="utf-8") as f:
                data = json.load(f)
            if isinstance(data, dict) and isinstance(data.get("routines"), list):
                return [r for r in data["routines"] if isinstance(r, dict) and r.get("id")]
        except (OSError, ValueError):
            pass
        return []

    def save(self, routines: list[dict[str, Any]]) -> None:
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump({"version": 1, "routines": routines}, f, ensure_ascii=False, indent=1)
        os.replace(tmp, self.path)

    def find(self, wanted: str, routines: Optional[list[dict[str, Any]]] = None) -> Optional[dict[str, Any]]:
        routines = self.load() if routines is None else routines
        wanted = (wanted or "").strip()
        for r in routines:
            if r["id"] == wanted:
                return r
        folded = _fold(wanted)
        exact = [r for r in routines if _fold(r["name"]) == folded]
        if exact:
            return exact[0]
        close = [r for r in routines if folded and folded in _fold(r["name"])]
        return close[0] if len(close) == 1 else None

    def overview(self) -> dict[str, Any]:
        routines = self.load()
        for r in routines:
            r["summary"] = describe_trigger(r["trigger"])
            r["running"] = r["id"] in self.running
        return {"routines": routines}

    def put(self, body: dict[str, Any]) -> dict[str, Any]:
        with self.lock:
            routines = self.load()
            old = self.find(str(body.get("id") or ""), routines) if body.get("id") else None
            if body.get("id") and old is None:
                raise RoutineError("No such routine (it may have been removed).")
            routine = clean(body, old)
            clash = [r for r in routines if r["id"] != routine["id"] and _fold(r["name"]) == _fold(routine["name"])]
            if clash:
                raise RoutineError(f"There is already a routine called {routine['name']}.")
            if old is None and len(routines) >= MAX_ROUTINES:
                raise RoutineError(f"At most {MAX_ROUTINES} routines.")
            if old is not None and old.get("trigger") != routine["trigger"]:
                routine.pop("firedSlot", None)
            routines = [routine if r["id"] == routine["id"] else r for r in routines]
            if old is None:
                routines.append(routine)
            self.save(routines)
        return routine

    def remove(self, wanted: str) -> bool:
        with self.lock:
            routines = self.load()
            r = self.find(wanted, routines)
            if r is None:
                return False
            self.save([x for x in routines if x["id"] != r["id"]])
        return True

    def _record(self, rid: str, **changes: Any) -> None:
        with self.lock:
            routines = self.load()
            for r in routines:
                if r["id"] == rid:
                    r.update(changes)
            self.save(routines)

    # ── Running ──────────────────────────────────────────────────────────────

    def run(self, wanted: str, origin: str = "app", wait: float = 60.0) -> dict[str, Any]:
        """Runs a routine (its steps in a thread of their own) and waits up to `wait` seconds for its end: what each
        step did, or that it still runs."""
        r = self.find(wanted)
        if r is None:
            raise RoutineError(f"No routine called {wanted} (home_routines lists them).")
        with self.lock:
            current = self.running.get(r["id"])
            if current is None:
                current = {"steps": [], "done": threading.Event(), "origin": origin, "started": time.time()}
                self.running[r["id"]] = current
                threading.Thread(target=self._execute, args=(r, current), name=f"routine-{r['id']}",
                                 daemon=True).start()
        current["done"].wait(timeout=max(0.0, wait))
        out = {"id": r["id"], "name": r["name"], "steps": list(current["steps"]),
               "running": not current["done"].is_set()}
        out["ok"] = not out["running"] and all(s["ok"] for s in out["steps"])
        return out

    def _execute(self, r: dict[str, Any], current: dict[str, Any]) -> None:
        try:
            for i, step in enumerate(r["steps"]):
                try:
                    said = self._step(step, r)
                    current["steps"].append({"step": i, "ok": True, "said": said, "what": describe_step(step)})
                except Exception as e:  # one step that fails never stops the next ones
                    current["steps"].append({"step": i, "ok": False, "said": str(e) or e.__class__.__name__,
                                             "what": describe_step(step)})
            ok = all(s["ok"] for s in current["steps"])
            self._record(r["id"], lastRun={"at": datetime.now().isoformat(timespec="seconds"),
                                           "origin": current["origin"], "ok": ok,
                                           "steps": current["steps"][-MAX_STEPS:]})
            log.info("routine %s ran (%s): %s", r["id"], current["origin"], "ok" if ok else "with failures")
        except Exception:
            log.exception("routine %s failed", r["id"])
        finally:
            with self.lock:
                self.running.pop(r["id"], None)
            current["done"].set()

    def _phone(self, request: dict[str, Any], wait: float = 20.0) -> str:
        if self.bridge is None:
            raise RoutineError("The AXYON app is not listening right now.")
        answer = self.bridge.ask("iphone", request, wait=wait)
        if not isinstance(answer, dict):
            raise RoutineError("No answer from the iPhone.")
        if answer.get("error"):
            raise RoutineError(str(answer["error"]))
        return "Done."

    def _step(self, step: dict[str, Any], r: dict[str, Any]) -> str:
        do = step["do"]
        if do == "device":
            done = self.house.act(step["device"], step["action"], step.get("value"))
            if not done.get("ok", True):
                raise RoutineError(str(done.get("message") or done.get("error") or "Not done."))
            return str(done.get("did") or "Done.")
        if do == "every":
            devices = [d for d in self.house.overview(states=False)["devices"]
                       if d.get("kind") == step["kind"] and step["action"] in (d.get("can") or [])]
            if not devices:
                return f"No {step['kind']} in the home."
            failed = []
            for d in devices:
                try:
                    self.house.act(d["id"], step["action"])
                except Exception as e:
                    failed.append(f"{d.get('name')}: {e}")
            if failed and len(failed) == len(devices):
                raise RoutineError("; ".join(failed)[:300])
            return f"{len(devices) - len(failed)} of {len(devices)} done." + (f" {'; '.join(failed)[:200]}" if failed else "")
        if do == "pause":
            time.sleep(min(MAX_PAUSE, float(step["seconds"])))
            return "Waited."
        if do == "ask":
            if self.ask is None:
                raise RoutineError("AXYON Core is still starting.")
            session = f"axyon-routine-{r['id']}-{datetime.now():%Y%m%d%H%M}"
            answer = (self.ask(step["prompt"], session, 150.0) or "").strip()
            if not answer or "[SILENT]" in answer:
                return "Nothing to tell."
            body = answer if len(answer) <= 1500 else answer[:1500].rstrip() + "…"
            self._phone({"action": "notify", "title": r["name"], "body": body})
            return "Told you."
        action = step["action"]
        if action == "notify":
            return self._phone({"action": "notify", "title": step.get("title") or r["name"], "body": step["body"]})
        if action == "reminder":
            request: dict[str, Any] = {"action": "reminders", "do": "add", "title": step["title"]}
            if step.get("inMinutes"):
                due = datetime.now() + timedelta(minutes=int(step["inMinutes"]))
                request["due"] = due.isoformat(timespec="minutes")
            return self._phone(request)
        if action == "speak":
            return self._phone({"action": "speak", "text": step["text"]})
        if action == "torch":
            return self._phone({"action": "torch", "on": step.get("on", True)})
        if action == "music":
            return self._phone({"action": "music", "do": step.get("what", "play")})
        if action == "open":
            return self._phone({"action": "open", "url": step["url"]})
        if action == "shortcut":
            return self._phone({"action": "shortcut", "name": step["name"], "input": step.get("input", "")})
        raise RoutineError(f"Unknown step {action}.")

    # ── The clock ────────────────────────────────────────────────────────────

    @staticmethod
    def slot(r: dict[str, Any], now: datetime) -> Optional[str]:
        """The minute this time routine came due today and has not run for yet, or None."""
        t = r.get("trigger") or {}
        if not r.get("enabled", True) or t.get("kind") != "time":
            return None
        hour, minute = (int(x) for x in str(t.get("at") or "99:99").split(":"))
        at = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
        if at > now or now - at > LATE:
            return None
        days = t.get("days") or []
        if days and at.isoweekday() not in days:
            return None
        key = at.isoformat(timespec="minutes")
        return None if r.get("firedSlot") == key else key

    def tick(self, now: Optional[datetime] = None, wait: float = 0.0) -> list[str]:
        """The time routines that came due, run now (each once per minute it is due); their ids."""
        now = now or datetime.now()
        started = []
        with self.lock:
            routines = self.load()
            for r in routines:
                key = self.slot(r, now)
                if key:
                    r["firedSlot"] = key
                    started.append(r["id"])
            if started:
                self.save(routines)
        deadline = time.time() + wait
        for rid in started:
            self.run(rid, origin="time", wait=max(0.0, deadline - time.time()))
        return started

    def start(self) -> None:
        if self.thread is not None:
            return
        self.thread = threading.Thread(target=self._clock, name="axyon-routines", daemon=True)
        self.thread.start()

    def _clock(self) -> None:
        while True:
            time.sleep(TICK - (time.time() % TICK) + 0.2)
            try:
                self.tick()
            except Exception:
                log.exception("the routines' tick failed")

    # ── AXYON's tool ─────────────────────────────────────────────────────────

    def _confirm(self, what: str) -> bool:
        if self.bridge is None:
            return False
        answer = self.bridge.ask("iphone", {"action": "confirm", "text": what[:150]}, wait=60)
        return isinstance(answer, dict) and answer.get("ok") is True

    def tool(self, args: dict[str, Any], **_kw) -> str:
        do = str(args.get("do") or "list").lower()
        try:
            if do == "list":
                routines = self.load()
                out = [{"id": r["id"], "name": r["name"], "enabled": r.get("enabled", True),
                        "when": describe_trigger(r["trigger"]), "steps": [describe_step(s) for s in r["steps"]],
                        "lastRun": (r.get("lastRun") or {}).get("at")} for r in routines]
                note = None if out else ("No routine yet. Offer to make one: for arriving home, leaving, waking up, "
                                         "going to sleep, a film…")
                return json.dumps({"routines": out, **({"note": note} if note else {})}, ensure_ascii=False)
            if do == "run":
                return json.dumps(self.run(str(args.get("routine") or ""), origin="axyon", wait=90.0),
                                  ensure_ascii=False)
            if do in ("save", "add", "update"):
                body = args.get("routine") if isinstance(args.get("routine"), dict) else dict(args)
                body = {k: v for k, v in body.items() if k != "do"}
                body["by"] = "axyon"
                if body.get("trigger") is None:
                    body["trigger"] = {"kind": "manual"}
                old = self.find(str(body.get("id") or body.get("name") or ""))
                if old is not None and not body.get("id"):
                    body["id"] = old["id"]
                preview = clean(body, old)
                summary = (f"{'change' if old else 'make'} the routine {preview['name']} "
                           f"({describe_trigger(preview['trigger'])}): "
                           + "; ".join(describe_step(s) for s in preview["steps"]))
                if not self._confirm(summary):
                    return json.dumps({"ok": False, "reason": "declined",
                                       "message": "The owner did not confirm on the iPhone."}, ensure_ascii=False)
                saved = self.put(body)
                return json.dumps({"ok": True, "routine": saved["id"], "name": saved["name"],
                                   "when": describe_trigger(saved["trigger"])}, ensure_ascii=False)
            if do == "remove":
                r = self.find(str(args.get("routine") or ""))
                if r is None:
                    raise RoutineError("No such routine.")
                if not self._confirm(f"remove the routine {r['name']}"):
                    return json.dumps({"ok": False, "reason": "declined"}, ensure_ascii=False)
                return json.dumps({"ok": self.remove(r["id"])}, ensure_ascii=False)
            raise RoutineError("do is list, run, save or remove.")
        except RoutineError as e:
            return json.dumps({"ok": False, "error": str(e)}, ensure_ascii=False)
        except Exception as e:
            log.exception("home_routines failed")
            return json.dumps({"ok": False, "error": f"The routine could not be handled ({e})."}, ensure_ascii=False)

    def register(self, registry) -> None:
        registry.register(
            name="home_routines", toolset=TOOLSET, handler=self.tool, emoji="⚡",
            schema={"name": "home_routines",
                    "description": (
                        "The user's routines on this iPhone (AXYON's automations): `do`=list, run (routine: its "
                        "name or id), save (create or change one; the owner confirms with Face ID) or remove. A "
                        "routine has a `name`, an `icon` (an SF Symbol: moon.stars.fill, sun.max.fill, house.fill, "
                        "car.fill, film.fill, bed.double.fill, figure.walk, briefcase.fill, sparkles), a `trigger` and "
                        "`steps` run in order. trigger: {kind: manual} (a tap, Siri: \"rodar <name> no AXYON\", or "
                        "the Shortcuts app's automations), {kind: time, at: \"07:30\", days: [1..7, Monday=1; empty "
                        "= every day]}, or {kind: arrive|leave, place: {name, lat, lon, radius metres}} (use "
                        "iphone_location for 'here'). steps: {do: device, device: <id or name from home_devices>, "
                        "action, value?}; {do: every, kind: light|tv|fan|switch, action: turn_on|turn_off} (TVs and "
                        "plugs only off); {do: iphone, action: notify (title, body) | reminder (title, inMinutes?) | "
                        "speak (text) | torch (on) | music (what: play|pause|next|previous) | open (url) | shortcut "
                        "(name, input?)}; {do: pause, seconds: 1-120}; {do: ask, prompt} (AXYON Core looks into it "
                        "when the routine runs, e.g. the day's agenda and the portfolio, and the answer comes as a "
                        "notification). Time routines run on time while AXYON is open; with AXYON closed, iOS "
                        "wakes it now and then, so for the exact minute suggest a Shortcuts automation (Shortcuts → "
                        "Automation → Time of Day → Run AXYON routine). Opening links and Shortcuts only works with "
                        "the iPhone unlocked."),
                    "parameters": {"type": "object", "properties": {
                        "do": {"type": "string", "enum": ["list", "run", "save", "remove"]},
                        "routine": {"description": "For run/remove: the routine's name or id. For save: the routine "
                                                   "object {id?, name, icon?, enabled?, trigger, steps}."}},
                        "required": ["do"]}})
