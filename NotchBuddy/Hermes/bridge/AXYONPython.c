// Hermes inside the iPhone app: CPython embedded as CPython's own iOS testbed does it (Apple/testbed in CPython's
// sources), isolated from any environment, with UTF-8 everywhere, its output in the system log, and no bytecode
// written into the signed bundle.

#include "AXYONPython.h"

#include <Python.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int started = 0;

static int failed(char *error, size_t size, const char *what, PyStatus status) {
    snprintf(error, size, "%s: %s", what, status.err_msg ? status.err_msg : "unknown error");
    return -1;
}

int axyon_python_run(const char *resources, char *error, size_t error_size) {
    PyStatus status;
    PyPreConfig preconfig;
    PyConfig config;
    char home[4096];

    if (started) {
        snprintf(error, error_size, "Python is already running");
        return -1;
    }
    started = 1;

    PyPreConfig_InitIsolatedConfig(&preconfig);
    preconfig.utf8_mode = 1;
    status = Py_PreInitialize(&preconfig);
    if (PyStatus_Exception(status)) {
        return failed(error, error_size, "Python did not pre-initialize", status);
    }

    PyConfig_InitIsolatedConfig(&config);
    config.buffered_stdio = 0;
    config.write_bytecode = 0;
    // The app's threads: Python never takes the signals over.
    config.install_signal_handlers = 0;
    // print() and tracebacks go to the system log (Console, or a device's sysdiagnose).
    config.use_system_logger = 1;

    snprintf(home, sizeof home, "%s/python", resources);
    status = PyConfig_SetBytesString(&config, &config.home, home);
    if (PyStatus_Exception(status)) {
        PyConfig_Clear(&config);
        return failed(error, error_size, "Python's home was refused", status);
    }
    status = PyConfig_Read(&config);
    if (PyStatus_Exception(status)) {
        PyConfig_Clear(&config);
        return failed(error, error_size, "Python's configuration could not be read", status);
    }
    status = Py_InitializeFromConfig(&config);
    PyConfig_Clear(&config);
    if (PyStatus_Exception(status)) {
        return failed(error, error_size, "Python did not start", status);
    }

    // The packages (with their .pth files) and the app's code, then the host, which serves until the app ends. Its
    // traceback, if it stops, is kept where the app shows it from.
    setenv("AXYON_BUNDLE", resources, 1);
    const char *code =
        "import os, site, sys, traceback\n"
        "bundle = os.environ['AXYON_BUNDLE']\n"
        "site.addsitedir(os.path.join(bundle, 'app_packages'))\n"
        "sys.path.insert(0, os.path.join(bundle, 'app'))\n"
        "try:\n"
        "    import axyon_hermes.host\n"
        "    axyon_hermes.host.main()\n"
        "except BaseException:\n"
        "    text = traceback.format_exc()\n"
        "    print(text, file=sys.stderr)\n"
        "    home = os.environ.get('AXYON_HERMES_HOME')\n"
        "    if home:\n"
        "        os.makedirs(home, exist_ok=True)\n"
        "        with open(os.path.join(home, 'host-error.txt'), 'w', encoding='utf-8') as f:\n"
        "            f.write(text)\n"
        "    raise\n";
    if (PyRun_SimpleString(code) != 0) {
        snprintf(error, error_size, "Hermes' host stopped with an error");
        return -1;
    }
    snprintf(error, error_size, "Hermes' host ended");
    return -1;
}

int axyon_python_exec(const char *code) {
    if (!started || !Py_IsInitialized()) {
        return -1;
    }
    PyGILState_STATE gil = PyGILState_Ensure();
    int result = PyRun_SimpleString(code);
    PyGILState_Release(gil);
    return result;
}
