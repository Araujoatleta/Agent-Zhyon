"""Hermes' scheduled tasks (its cronjob tool) on the iPhone.

On a computer, Hermes' gateway runs the clock: every minute it runs the jobs that are due (cron/scheduler.py, tick)
and posts each result to Telegram, Discord and the like. On the iPhone the host runs that clock in its own process
while AXYON runs, the app asks for a tick when iOS wakes it in the background (POST /v1/cron/tick), and each result
comes as a notification on the iPhone. A job's answer of [SILENT] stays silent, as Hermes' cron means it; failures
come too, unless the job says failures stay local."""

from __future__ import annotations

import logging
import threading
import time
from typing import Any

log = logging.getLogger("axyon.scheduled")

TICK = 60.0
# How long the app's notification may take: the result is kept in Hermes' cron output either way.
NOTIFY_WAIT = 20.0
BODY_LIMIT = 1800


class Scheduled:
    def __init__(self, bridge):
        self.bridge = bridge
        self.busy = threading.Lock()
        self.thread: threading.Thread | None = None
        self.delivered: list[dict[str, Any]] = []

    def install(self) -> None:
        """Each result Hermes' scheduler delivers goes to the iPhone instead of a gateway's channels."""
        import cron.scheduler as scheduler

        def deliver(job, content, adapters=None, loop=None, *, for_failure=False):
            return self.deliver(job, content, for_failure=for_failure)

        scheduler._deliver_result = deliver

    def deliver(self, job: dict[str, Any], content: str, *, for_failure: bool = False) -> str | None:
        """None when the iPhone took it (Hermes records the delivery as done), else why not."""
        text = (content or "").strip()
        if not text or (for_failure and str(job.get("failure_deliver") or "") == "local"):
            return None
        name = str(job.get("name") or job.get("id") or "AXYON")
        body = text if len(text) <= BODY_LIMIT else text[:BODY_LIMIT].rstrip() + "…"
        answer = self.bridge.ask("iphone", {"action": "notify", "title": name, "body": body}, wait=NOTIFY_WAIT)
        self.delivered = (self.delivered + [{"job": job.get("id"), "name": name, "at": time.time()}])[-20:]
        if isinstance(answer, dict) and answer.get("error"):
            return str(answer["error"])
        return None

    def start(self) -> None:
        """The clock, while AXYON runs: a tick at each minute."""
        if self.thread is not None:
            return
        self.thread = threading.Thread(target=self._clock, name="axyon-cron", daemon=True)
        self.thread.start()

    def _clock(self) -> None:
        while True:
            time.sleep(TICK - (time.time() % TICK) + 0.5)
            self.tick()

    def tick(self, wait: float = 0) -> int:
        """The due jobs, run now, one tick at a time (the app's waits `wait` seconds for the clock's); how many
        ran."""
        if not (self.busy.acquire(timeout=wait) if wait > 0 else self.busy.acquire(blocking=False)):
            return 0
        try:
            from cron.scheduler import tick

            return int(tick(verbose=False, sync=True) or 0)
        except Exception:
            log.exception("the scheduled tasks' tick failed")
            return 0
        finally:
            self.busy.release()

    def overview(self) -> dict[str, Any]:
        """The jobs as the app's Settings show them."""
        from cron.jobs import list_jobs

        jobs = []
        for job in list_jobs(include_disabled=True):
            schedule = job.get("schedule") or {}
            jobs.append({"id": job.get("id"), "name": job.get("name") or job.get("id"),
                         "schedule": schedule.get("display") if isinstance(schedule, dict) else str(schedule),
                         "next": job.get("next_run_at"), "last": job.get("last_run_at"),
                         "status": job.get("last_status"), "enabled": job.get("enabled", True)})
        return {"jobs": jobs}
