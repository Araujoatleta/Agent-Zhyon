// Hermes inside the iPhone app: CPython embedded, started once (NotchBuddy/Hermes/README.md). Swift sees these through
// the bridging header of the iPhone build that carries Hermes (project-hermes.yml).

#ifndef AXYONPython_h
#define AXYONPython_h

#include <stddef.h>

/// Starts CPython with the app bundle's standard library (`<resources>/python`), its packages (`app_packages`) and
/// code (`app`), then runs `axyon_hermes.host.main()` on the calling thread, which it keeps while the app runs. Returns
/// only if Python could not start or the host stopped, with why in `error`.
int axyon_python_run(const char *resources, char *error, size_t error_size);

/// Runs a line of Python on the running interpreter from any thread (it takes the GIL). 0 when it ran.
int axyon_python_exec(const char *code);

#endif
