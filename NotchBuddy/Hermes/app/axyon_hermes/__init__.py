"""Hermes Agent inside AXYON on the iPhone (NotchBuddy/Hermes/README.md).

The app's embedded Python runs `axyon_hermes.host.main()`: Hermes itself is the brain, with its memory, skills and
persona in the app's own folder, and Coucou's tools are its arms (`coucou`). The app talks to it over HTTP on
127.0.0.1 only, with a token it makes for each launch.
"""

PROTOCOL = 1
