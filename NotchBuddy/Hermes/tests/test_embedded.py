"""The iPhone app's start of Python, on a computer: bridge/AXYONPython.c as the app runs it (HermesEngine.start), built
against a Python 3.14 with Hermes' dependencies and run on a bundle laid out as the app's (python, app, app_packages:
build-ios.sh), with links instead of copies. Its host must listen and Hermes become ready, and a start that fails must
say why, at once. Needs Hermes' checkout and a C compiler: `HERMES_SRC=… PYTHON=… python3 test_embedded.py`."""
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(ROOT))
failures = 0


def check(label, ok):
    global failures
    print(("  ✓ " if ok else "  ✗ ") + label)
    if not ok:
        failures += 1


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    return port


def python_config(python):
    """Where the Python to embed keeps its headers, library and packages."""
    code = (
        "import json, sys, sysconfig\n"
        "v = sysconfig.get_config_var\n"
        "print(json.dumps({'include': sysconfig.get_paths()['include'], 'libdir': v('LIBDIR'),\n"
        "                  'version': v('LDVERSION') or v('VERSION'),\n"
        "                  'libs': ' '.join(filter(None, [v('LIBS'), v('SYSLIBS')])),\n"
        "                  'prefix': sys.base_prefix, 'packages': sysconfig.get_paths()['purelib']}))\n"
    )
    return json.loads(subprocess.check_output([python, "-c", code], text=True))


def build(config, work):
    binary = os.path.join(work, "embedded")
    subprocess.run([os.environ.get("CC", "cc"), "-g", "-Wall", "-I" + os.path.join(ROOT, "bridge"),
                    "-I" + config["include"], os.path.join(HERE, "embedded_main.c"),
                    os.path.join(ROOT, "bridge", "AXYONPython.c"), "-L" + config["libdir"],
                    "-lpython" + config["version"], "-Wl,-rpath," + config["libdir"], "-lpthread",
                    *config["libs"].split(), "-o", binary], check=True)
    return binary


def bundle(config, hermes_src, work, name, with_app=True):
    """The app's resources as build-ios.sh lays them out: Python's home, the packages, and Hermes, the host and the
    iOS stand-ins in app/."""
    resources = os.path.join(work, name)
    os.makedirs(resources)
    os.symlink(config["prefix"], os.path.join(resources, "python"))
    os.symlink(config["packages"], os.path.join(resources, "app_packages"))
    if not with_app:
        return resources
    app = os.path.join(resources, "app")
    os.makedirs(os.path.join(app, "axyon_hermes"))
    for entry in os.listdir(hermes_src):
        if not entry.startswith("."):
            os.symlink(os.path.join(hermes_src, entry), os.path.join(app, entry))
    ours = os.path.join(ROOT, "app", "axyon_hermes")
    for entry in os.listdir(ours):
        if entry != "__pycache__":
            os.symlink(os.path.join(ours, entry), os.path.join(app, "axyon_hermes", entry))
    os.symlink(os.path.join(REPO, "windows", "hook", "src", "house_tools.json"),
               os.path.join(app, "axyon_hermes", "house_tools.json"))
    for entry in os.listdir(os.path.join(ROOT, "shims")):
        target = os.path.join(app, entry)
        if os.path.lexists(target):
            os.remove(target)
        os.symlink(os.path.join(ROOT, "shims", entry), target)
    return resources


def environment(work, name, port, token):
    """What HermesEngine.start sets before it starts Python; nothing of a Python of the computer's."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("PYTHON")}
    env.update(AXYON_HERMES_HOME=os.path.join(work, name, "hermes"), AXYON_HOST_PORT=str(port),
               AXYON_HOST_TOKEN=token, AXYON_PORTFOLIO=os.path.join(work, name, "portfolio.json"),
               AXYON_POWERS=os.path.join(work, name, "powers"), AXYON_LANGUAGE="pt-BR")
    return env


def health(port, token):
    req = urllib.request.Request(f"http://127.0.0.1:{port}/health")
    req.add_header("Authorization", "Bearer " + token)
    try:
        with urllib.request.urlopen(req, timeout=5) as r:
            return json.loads(r.read() or b"{}")
    except (urllib.error.URLError, ConnectionError, TimeoutError):
        return None


def main():
    hermes_src = os.environ["HERMES_SRC"]
    python = os.environ.get("PYTHON", sys.executable)
    work = tempfile.mkdtemp(prefix="axyon-embedded-")
    config = python_config(python)
    print(f"Building the app's start of Python against Python {config['version']}")
    binary = build(config, work)

    print("The app's start: the host listens, Hermes becomes ready")
    port, token = free_port(), secrets.token_hex(16)
    resources = bundle(config, hermes_src, work, "Coucou.app")
    output = os.path.join(work, "app-stderr.log")
    app = subprocess.Popen([binary, resources], env=environment(work, "home", port, token),
                           stdout=subprocess.DEVNULL, stderr=open(output, "w"))
    try:
        state, started = None, time.time()
        while time.time() - started < 180 and app.poll() is None:
            state = health(port, token)
            if state and (state.get("ready") or state.get("problem")):
                break
            time.sleep(0.5)
        said = open(output, encoding="utf-8", errors="replace").read()
        check("Python still running", app.poll() is None)
        check("the host listens on its port", f"AXYON: Hermes' host listens on 127.0.0.1:{port}" in said)
        check("Hermes ready inside the embedded Python", bool(state) and state.get("ready") is True)
        check("the embedded Python is 3.14", bool(state) and str(state.get("python", "")).startswith("3.14"))
        check("ready said on the app's output", "AXYON: Hermes ready in" in said)
        if failures:
            print(said[-4000:])
    finally:
        app.terminate()
        app.wait(timeout=10)

    print("A start that fails says why, at once")
    port, token = free_port(), secrets.token_hex(16)
    resources = bundle(config, hermes_src, work, "Broken.app", with_app=False)
    output = os.path.join(work, "broken-stderr.log")
    broken = subprocess.Popen([binary, resources], env=environment(work, "broken", port, token),
                              stdout=subprocess.DEVNULL, stderr=open(output, "w"))
    try:
        code = broken.wait(timeout=60)
    except subprocess.TimeoutExpired:
        broken.kill()
        code = None
    said = open(output, encoding="utf-8", errors="replace").read()
    check("Python ends instead of hanging", code is not None)
    check("the traceback on the app's output", "AXYON: Traceback" in said and "axyon_hermes" in said)
    check("the start's error for the app", "AXYON: Hermes' host stopped with an error" in said)
    kept = os.path.join(work, "broken", "hermes", "host-error.txt")
    check("the traceback kept where the app shows it from",
          os.path.exists(kept) and "ModuleNotFoundError" in open(kept, encoding="utf-8").read())
    if failures:
        print(said[-3000:])

    print("All embedded start tests passed." if not failures else f"{failures} failed.")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
