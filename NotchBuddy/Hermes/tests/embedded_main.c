// The iPhone app's start of Python (HermesEngine.start → axyon_python_run), on a computer: test_embedded.py builds
// this with bridge/AXYONPython.c against a Python 3.14 and runs it on a bundle laid out as the app's.
//   embedded_main <bundle>

#include <pthread.h>
#include <stdio.h>

#include "AXYONPython.h"

static const char *resources;

static void *run(void *unused) {
    char error[2048] = "";
    (void)unused;
    axyon_python_run(resources, error, sizeof error);
    fprintf(stderr, "axyon_python_run returned: %s\n", error);
    return NULL;
}

int main(int argc, char **argv) {
    pthread_attr_t attributes;
    pthread_t thread;
    if (argc != 2) {
        fprintf(stderr, "usage: %s <bundle>\n", argv[0]);
        return 2;
    }
    resources = argv[1];
    // The app's own thread for Python: 16 MB of stack, as HermesEngine gives it.
    pthread_attr_init(&attributes);
    pthread_attr_setstacksize(&attributes, 16 << 20);
    if (pthread_create(&thread, &attributes, run, NULL) != 0) {
        fprintf(stderr, "no thread for Python\n");
        return 2;
    }
    pthread_join(thread, NULL);
    // The host serves while the app runs: its return is a failure.
    return 1;
}
