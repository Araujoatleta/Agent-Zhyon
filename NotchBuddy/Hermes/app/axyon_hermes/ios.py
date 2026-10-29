"""What an iPhone app's Python is allowed, set up before Hermes is imported.

iOS gives an app no processes of its own (CPython there has no fork or exec, and subprocess refuses), a read-only
bundle, and no system certificate file. Hermes runs within that: its home is in the app's own data folder, TLS trusts
certifi's authorities, and what it would hand to a process (a terminal, a browser, MCP servers over stdio) is left out
of its tools.
"""

from __future__ import annotations

import os
import sys

# Toolsets that need nothing but Python and the network: Coucou's arms and the user's Google account, then Hermes' own
# memory, skills, to-dos, questions back to the user, its past sessions, image reading and making, scheduled tasks
# (scheduled.py runs their clock) and helpers it delegates to. A terminal, files through a shell, code and a browser
# need programs iOS does not let an app run.
TOOLSETS = ["coucou", "google", "memory", "skills", "todo", "clarify", "session_search", "vision", "image_gen",
            "cronjob", "delegation"]

# The image maker that goes with each brain (Hermes' image_gen plugins): the user's ChatGPT subscription, or
# OpenRouter's key. The other brains have none of their own.
IMAGE_MAKERS = {"openai-codex": "openai-codex", "openrouter": "openrouter"}


def on_ios() -> bool:
    return sys.platform == "ios"


def prepare(home: str) -> None:
    """The environment Hermes reads at import: its home, the certificates, and the switches that keep it from
    starting what an iPhone cannot run."""
    os.makedirs(home, exist_ok=True)
    os.environ["HERMES_HOME"] = home
    os.environ.setdefault("HOME", os.path.dirname(home))
    try:
        import certifi

        os.environ.setdefault("SSL_CERT_FILE", certifi.where())
        os.environ.setdefault("REQUESTS_CA_BUNDLE", certifi.where())
    except ImportError:
        pass
    # No terminal of its own: a command never runs here, whatever a model asks.
    os.environ.setdefault("TERMINAL_ENV", "local")
    # Someone answers in the app, as in Hermes' gateway, which sets this for its whole process: it is what offers
    # the cronjob tool (tools/cronjob_tools.py). It also says commands ask first, and there are none here.
    os.environ.setdefault("HERMES_EXEC_ASK", "1")
    os.environ.setdefault("NO_COLOR", "1")
    os.environ.setdefault("PYTHONIOENCODING", "utf-8")
    if on_ios():
        # The bundle is signed: no bytecode is ever written into it.
        sys.dont_write_bytecode = True
