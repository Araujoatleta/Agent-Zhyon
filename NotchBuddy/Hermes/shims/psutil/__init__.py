"""psutil for Hermes inside the iPhone app.

iOS has no processes but the app's own: an app cannot start one, nor see another. Hermes imports psutil to manage the
processes its tools start (terminal, browser, MCP servers), none of which exist here. This stands in for it with the
same names: the app's own process is known, every other pid does not exist, and what iOS does not tell an app raises
AccessDenied, which Hermes already handles as it does on a locked-down computer.
"""

from __future__ import annotations

import collections
import os
import sys
import time

version_info = (7, 2, 2)
__version__ = "7.2.2"

LINUX = WINDOWS = MACOS = OSX = FREEBSD = OPENBSD = NETBSD = SUNOS = AIX = BSD = False
POSIX = True

STATUS_RUNNING = "running"
STATUS_SLEEPING = "sleeping"
STATUS_DISK_SLEEP = "disk-sleep"
STATUS_STOPPED = "stopped"
STATUS_TRACING_STOP = "tracing-stop"
STATUS_ZOMBIE = "zombie"
STATUS_DEAD = "dead"
STATUS_IDLE = "idle"
STATUS_WAKING = "waking"
STATUS_LOCKED = "locked"
STATUS_WAITING = "waiting"

_STARTED = time.time()


class Error(Exception):
    """Base of psutil's errors."""


class NoSuchProcess(Error):
    def __init__(self, pid=None, name=None, msg=None):
        super().__init__(msg or f"process no longer exists (pid={pid})")
        self.pid, self.name, self.msg = pid, name, msg


class ZombieProcess(NoSuchProcess):
    def __init__(self, pid=None, name=None, ppid=None, msg=None):
        super().__init__(pid, name, msg or f"process still exists but it's a zombie (pid={pid})")
        self.ppid = ppid


class AccessDenied(Error):
    def __init__(self, pid=None, name=None, msg=None):
        super().__init__(msg or f"not available on iOS (pid={pid})")
        self.pid, self.name, self.msg = pid, name, msg


class TimeoutExpired(Error):
    def __init__(self, seconds, pid=None, name=None):
        super().__init__(f"timeout after {seconds} seconds (pid={pid})")
        self.seconds, self.pid, self.name = seconds, pid, name


pmem = collections.namedtuple("pmem", ["rss", "vms"])
svmem = collections.namedtuple("svmem", ["total", "available", "percent", "used", "free"])
sdiskusage = collections.namedtuple("sdiskusage", ["total", "used", "free", "percent"])
pcputimes = collections.namedtuple("pcputimes", ["user", "system", "children_user", "children_system"])


def _rss() -> int:
    try:
        import resource
        # Bytes on Apple platforms.
        return int(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)
    except Exception:
        return 0


class Process:
    """The app's own process; any other pid does not exist for an iOS app."""

    def __init__(self, pid: int | None = None):
        self.pid = os.getpid() if pid is None else int(pid)
        if self.pid != os.getpid():
            raise NoSuchProcess(self.pid)

    def __eq__(self, other):
        return isinstance(other, Process) and other.pid == self.pid

    def __hash__(self):
        return hash(self.pid)

    def __repr__(self):
        return f"psutil.Process(pid={self.pid}, name={self.name()!r}, status='running')"

    def oneshot(self):
        import contextlib
        return contextlib.nullcontext()

    def is_running(self) -> bool:
        return True

    def name(self) -> str:
        return os.path.basename(sys.executable or "") or "AXYON"

    def exe(self) -> str:
        return sys.executable or ""

    def cmdline(self) -> list[str]:
        return list(sys.argv)

    def status(self) -> str:
        return STATUS_RUNNING

    def create_time(self) -> float:
        return _STARTED

    def ppid(self) -> int:
        return os.getppid()

    def parent(self):
        return None

    def parents(self) -> list:
        return []

    def children(self, recursive: bool = False) -> list:
        return []

    def cwd(self) -> str:
        return os.getcwd()

    def environ(self) -> dict:
        return dict(os.environ)

    def username(self) -> str:
        return os.environ.get("USER", "mobile")

    def memory_info(self):
        rss = _rss()
        return pmem(rss, rss)

    def cpu_times(self):
        t = os.times()
        return pcputimes(t.user, t.system, 0.0, 0.0)

    def cpu_percent(self, interval=None) -> float:
        return 0.0

    def num_threads(self) -> int:
        import threading
        return threading.active_count()

    def open_files(self) -> list:
        return []

    def connections(self, kind: str = "inet") -> list:
        return []

    net_connections = connections

    def wait(self, timeout=None):
        raise TimeoutExpired(timeout or 0, self.pid)

    def send_signal(self, sig):
        raise AccessDenied(self.pid, msg="the app does not signal itself")

    terminate = kill = suspend = resume = send_signal

    def __getattr__(self, name):
        def unavailable(*args, **kwargs):
            raise AccessDenied(self.pid, msg=f"Process.{name} is not available on iOS")
        return unavailable


def pid_exists(pid) -> bool:
    try:
        return int(pid) == os.getpid()
    except (TypeError, ValueError):
        return False


def pids() -> list[int]:
    return [os.getpid()]


def process_iter(attrs=None, ad_value=None):
    proc = Process()
    if attrs is not None:
        info = {}
        for a in attrs:
            try:
                info[a] = getattr(proc, a)()
            except Error:
                info[a] = ad_value
        proc.info = info
    yield proc


def wait_procs(procs, timeout=None, callback=None):
    """Every process given is already gone, but the app's own (which never ends here)."""
    gone, alive = [], []
    for p in procs:
        (alive if getattr(p, "pid", None) == os.getpid() else gone).append(p)
    for p in gone:
        p.returncode = None
        if callback:
            callback(p)
    return gone, alive


def cpu_count(logical: bool = True) -> int:
    return os.cpu_count() or 1


def cpu_percent(interval=None, percpu: bool = False):
    return [0.0] * cpu_count() if percpu else 0.0


def virtual_memory():
    try:
        total = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")
    except (ValueError, OSError, AttributeError):
        total = 4 * 1024 ** 3
    used = _rss()
    free = max(total - used, 0)
    return svmem(total, free, round(used * 100.0 / total, 1) if total else 0.0, used, free)


def disk_usage(path):
    st = os.statvfs(path)
    total = st.f_blocks * st.f_frsize
    free = st.f_bavail * st.f_frsize
    used = total - st.f_bfree * st.f_frsize
    return sdiskusage(total, used, free, round(used * 100.0 / total, 1) if total else 0.0)


def boot_time() -> float:
    return _STARTED


def sensors_battery():
    return None


def users() -> list:
    return []
