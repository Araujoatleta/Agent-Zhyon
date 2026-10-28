"""Runs the host as on an iPhone, on a computer: no processes (CPython on iOS has no fork or exec), sys.platform
'ios', and the packages the iPhone build leaves out absent. Used by test_host.py."""
import importlib.abc
import os
import sys
import types

sys.platform = "ios"
import platform  # noqa: E402

platform.ios_ver = lambda *a, **k: platform.IOSVersionInfo("iOS", "18.6", "iPhone", False)
sys.modules["_ios_support"] = types.SimpleNamespace(objc=None, get_platform_ios=lambda: ("iOS", "18.6", "iPhone", False))
import subprocess  # noqa: E402

subprocess._can_fork_exec = False
for name in ("fork", "forkpty", "execv", "execve", "execvp", "spawnv", "posix_spawn", "posix_spawnp", "system", "popen"):
    if hasattr(os, name):
        delattr(os, name)

# Left out of the iPhone build (NotchBuddy/Hermes/build-ios.sh): compiled packages with no iOS build, or no use there.
ABSENT = {"nemo_relay", "watchfiles", "httptools", "uvloop", "cryptography", "pillow_heif", "resvg_py",
          "firecrawl_anydoc"}


class _Absent(importlib.abc.MetaPathFinder):
    def find_spec(self, name, path=None, target=None):
        if name.split(".")[0] in ABSENT:
            raise ModuleNotFoundError(f"No module named {name!r}", name=name)
        return None


sys.meta_path.insert(0, _Absent())

from axyon_hermes.host import main  # noqa: E402

main()
