# Hermes inside the iPhone

The sideloaded iPhone app can carry **Hermes Agent** itself (NousResearch/hermes-agent, the commit the computer
installs, `windows/src-tauri/src/hermes_setup.rs`): CPython 3.14 for iOS runs it inside the app, so AXYON thinks and
acts with no computer at all. Hermes is the brain; the iPhone is its body (reminders, calendar, contacts, apps and
links, Shortcuts, notifications, location, flashlight, music, the clipboard, speaking), with the web, the B3's prices
and, when a computer is paired, its home, agents and portfolio. The user guide is
[docs/IPHONE_HERMES.md](../../docs/IPHONE_HERMES.md) ([português](../../docs/IPHONE_HERMES.pt-BR.md)).

Only the build made with this folder carries it. The App Store build and the plain sideload build use `project.yml`
alone: none of this is compiled or copied into them.

## Layout

| Path | What |
|---|---|
| `build-ios.sh` | Downloads and builds what the app carries, into `build/` (git-ignored). macOS, Xcode, Rust, Python 3.14. |
| `app/axyon_hermes/` | The host: Hermes behind a small HTTP API on 127.0.0.1, and AXYON's tools for it. |
| `shims/psutil/` | An iOS stand-in for psutil (no other process can be seen on iOS). |
| `bridge/` | The C that starts CPython (`axyon_python_run`) and runs a line in it (`axyon_python_exec`). |
| `Swift/` | `HermesEngine` (starts and talks to the host), `HermesArms` (what Hermes asks of the iPhone), `HermesWire` (its events and the conversation, tested on Linux), `AXYONStyle` and `AXYONViews` (the screens). |
| `utils.sh`, `iOS-dylib-Info-template.plist` | CPython's own iOS install steps: every compiled module becomes a framework. |
| `requirements-*.txt` | Hermes' packages: pure Python, and the three with iOS wheels on PyPI. |
| `tests/` | `test_units.py` (the tools, no Hermes), `test_home.py` (the home against a simulated Samsung TV, ADB box and Home Assistant), `test_host.py` (the host end to end with Hermes, iOS limits emulated), `simulator.sh` (CI: the app in the iOS Simulator, Hermes started for real and answering with an iPhone tool). |

## Build

```
cd NotchBuddy
Hermes/build-ios.sh                    # build/support (CPython), build/app (Hermes + host), build/app_packages
xcodegen --spec project-hermes.yml     # project.yml + Hermes, in the CoucouPhone target, with HERMES
xcodebuild -scheme CoucouPhone -configuration Release -destination 'generic/platform=iOS' \
    CODE_SIGNING_ALLOWED=NO 'SWIFT_ACTIVE_COMPILATION_CONDITIONS=$(inherited) SIDELOAD HERMES' build
```

CI does exactly this (`.github/workflows/iphone-sideload.yml`, job `ipa-hermes`) and uploads
`AXYON-iPhone-Hermes.ipa`, which Sideloadly or AltStore sign with the user's free Apple Account.

What `build-ios.sh` puts in the app:

- **CPython 3.14 for iOS**: BeeWare's Python-Apple-support, the build CPython's iOS docs point to.
- **Hermes** at the computer's commit, without its tests, website and apps; every `.py` compiled to bytecode that
  never goes stale (the bundle is signed, nothing can be written in it).
- **Packages**: 53 pure ones, three iOS wheels from PyPI (charset-normalizer, markupsafe, pillow), and pydantic-core
  and jiter, which publish no iOS wheel, built here with cargo for `aarch64-apple-ios` against the Python framework.
  Packages that need a native binary nobody builds for iOS are left out (watchfiles, uvloop, httptools, cryptography,
  pillow-heif, resvg): Hermes runs without them.

## How it runs

`HermesEngine.start()` (once per launch: CPython cannot start twice) picks a free port, makes a 32-byte token and
starts CPython on a thread of its own with a 16 MB stack. `axyon_python_run` initializes it as CPython's iOS testbed
does (isolated, UTF-8, output to the system log, no bytecode written) and runs `axyon_hermes.host.main()`, which:

- points Hermes at `Application Support/Hermes` (config, sign-ins, memories, skills, sessions), the CA bundle at
  certifi's, and reports iOS as it is (no subprocess, no fork: Hermes' terminal and code tools are not offered);
- serves on `127.0.0.1` only, refusing any request without the token or with an `Origin` header;
- seeds `config.yaml` (ChatGPT as the brain, every tool offered directly) and an AXYON `SOUL.md`, which it rewrites
  only while it is still the one it wrote.

The API (bearer token on every call):

| Call | |
|---|---|
| `GET /health` | `ready`, or the `problem` that stopped Hermes from loading. |
| `POST /v1/runs` `{session, message}` | Starts an answer: `{run_id}`. |
| `GET /v1/runs/<id>/events` | Server-sent events: `delta`, `tool`, `tool_done`, `done`, `error`. |
| `POST /v1/runs/<id>/stop` | Interrupts it. |
| `GET/POST /v1/settings` | The brain, the model, the address of an OpenAI-compatible server, the keys (in memory only). |
| `POST /v1/auth/chatgpt/start`, `GET /v1/auth/chatgpt` | The ChatGPT sign-in, as `hermes auth add openai-codex` runs it: a code to type on OpenAI's page. |
| `GET /v1/portfolio` | The portfolio with prices read now. |
| `GET /v1/bridge`, `POST /v1/bridge/<id>` | What Hermes asks of the iPhone, and the app's answer. |
| `GET /v1/home`, `POST /v1/home/scan`, `/v1/home/devices`, `/v1/home/devices/<id>/pair`, `…/action`, `…/remove`, `/v1/home/assistant` | The iPhone's own home. |

Keys stay in the iPhone's Keychain (`WhenUnlockedThisDeviceOnly`, not in backups) and reach Hermes in memory at each
start; nothing writes them to disk. The ChatGPT sign-in is the iPhone's own: refresh tokens are single-use, so the
iPhone never shares the computer's.

## AXYON's tools for Hermes

- `iphone_*` (`app/axyon_hermes/iphone.py`, answered by `Swift/HermesArms.swift`): notify, reminders, calendar,
  contacts, open, shortcut, clipboard, status, location, torch, music, speak. Each asks iOS's permission the first
  time. Nothing sends a message, an e-mail or calls anyone: `iphone_open` opens the app concerned, filled in, and the
  user sends.
- `coucou_web_search`, `coucou_web_read` (`web.py`): DuckDuckGo's HTML results, a page as text. No key, no account.
- `coucou_portfolio`, `coucou_quote` (`coucou.py`): the copy of the portfolio a paired computer gives the iPhone
  (positions, the user's ceilings, last prices, the notes of AXYON's tab: `windows/src-tauri/src/powers/mod.rs`,
  `phone_copy`; capability `portfolio` of the phone link), with B3 prices read now from Google Finance.
- `home_devices`, `home_control` (`home.py`): the home this iPhone drives itself on the Wi-Fi, no computer: Samsung
  TVs (REST on 8001, the remote over TLS on 8002, which asks once on the TV and then knows AXYON by its token; the
  TV's certificate pinned on first contact), Android TVs and boxes over ADB on 5555 (an RSA-2048 key made on the
  iPhone, kept in its Keychain, offered once: "Allow USB debugging?"; then only the commands of a fixed list: keys,
  the device's own apps, its state), and the user's Home Assistant (a long-lived token pasted in the app, in the
  Keychain; only the services of `ACTIONS`: never a lock or an alarm). Plugs and switches ask the owner's Face ID
  before Hermes acts on them. Only local addresses (or the tailnet) are contacted, and a Home Assistant over https.
  The app's Home tab adds them (a scan of the Wi-Fi it is on, or an address), pairs them and drives them.
- `coucou_home`, `coucou_home_action`: the paired computer's home, through the phone link (a device that asks first
  waits for Face ID there).
- Hermes' own `memory`, `skills`, `todo`, `clarify`, `session_search` and `vision`.

## What iOS does not allow

iOS suspends an app soon after it leaves the screen: Hermes thinks while AXYON is open, and an answer under way gets
the half minute iOS grants to end (then a notification brings it). Reminders and timed notifications are iOS's own,
so they come with the app closed. iOS runs no other process: Hermes' terminal, code execution and browser tools are
not there.

## Tests

```
python3 tests/test_units.py                                   # the tools, quotes, portfolio, web, bridge
python3 tests/test_home.py                                    # the home (needs websockets and openssl)
HERMES_SRC=…/hermes-agent PYTHON=…/venv/bin/python python3 tests/test_host.py   # the host with Hermes, end to end
bash ../../scripts/test-pc-link.sh                            # HermesWire.swift with the phone link's wire
```

`test_host.py` runs the host under `tests/ios_like.py`, which takes away what iOS does not have (subprocess, fork,
`sys.platform` is `ios`), against `tests/fake_model.py`, a local OpenAI-compatible model that streams and calls tools.
