# Hermes inside the iPhone

The sideloaded iPhone app can carry **Hermes Agent** itself (NousResearch/hermes-agent, the commit the computer
installs, `windows/src-tauri/src/hermes_setup.rs`): CPython 3.14 for iOS runs it inside the app, so AXYON thinks and
acts with no computer at all. Hermes is the brain; the iPhone is its body (reminders, calendar, contacts, apps and
links, Shortcuts, notifications, location, flashlight, music, the clipboard, speaking), with the web, its own home
(TVs, boxes, Home Assistant), the computer's superpowers engine (portfolio, funds, ceilings, taxes, studies, English,
training) and, when a computer is paired, its home, agents and portfolio. The user guide is
[docs/IPHONE_HERMES.md](../../docs/IPHONE_HERMES.md) ([português](../../docs/IPHONE_HERMES.pt-BR.md)).

Only the build made with this folder carries it. The App Store build and the plain sideload build use `project.yml`
alone: none of this is compiled or copied into them.

## Layout

| Path | What |
|---|---|
| `build-ios.sh` | Downloads and builds what the app carries, into `build/` (git-ignored). macOS, Xcode, Rust, Python 3.14. |
| `app/axyon_hermes/` | The host: Hermes behind a small HTTP API on 127.0.0.1, and AXYON's tools for it. |
| `shims/psutil/` | An iOS stand-in for psutil (no other process can be seen on iOS). |
| `bridge/` | The C that starts CPython (`axyon_python_run`) and runs a line in it (`axyon_python_exec`), and the superpowers' C interface (`AXYONPowers.h`, from `windows/powers-ffi`). |
| `Swift/` | `HermesEngine` (starts and talks to the host), `HermesArms` (what Hermes asks of the iPhone, and `HermesPowers`, the superpowers' engine), `HermesWire` (its events and the conversation, tested on Linux), `AXYONStyle`, `AXYONViews` and `AXYONHouse` (the screens), `AXYONAlerts` (the superpowers' alerts and Hermes' scheduled tasks with the app closed), `AXYONInbox` (the files sent, Google's sign-in and the scheduled tasks' screens). |
| `utils.sh`, `iOS-dylib-Info-template.plist` | CPython's own iOS install steps: every compiled module becomes a framework. |
| `requirements-*.txt` | Hermes' packages: pure Python, and the three with iOS wheels on PyPI. |
| `tests/` | `test_units.py` (the tools, no Hermes), `test_home.py` (the home against a simulated Samsung TV, ADB box and Home Assistant), `test_host.py` (the host end to end with Hermes, iOS limits emulated: files sent, Google's status, a scheduled task run and delivered), `test_embedded.py` (the app's C start of Python, on a computer), `simulator.sh` (CI: the app in the iOS Simulator, Hermes started for real, answering with an iPhone tool, a superpower the engine answers inside iOS, a document sent and a scheduled task run at the app's tick). |

## Build

```
cd NotchBuddy
Hermes/build-ios.sh                    # build/support (CPython), build/app (Hermes + host), build/app_packages,
                                       # build/powers (the superpowers' engine)
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
- **The superpowers**: the computer's own engine (`windows/powers`) behind two C functions (`windows/powers-ffi`),
  built with cargo as a static library for `aarch64-apple-ios` and linked into the app (`OTHER_LDFLAGS` in
  `project-hermes.yml`), and the tools the computer gives Hermes (`windows/hook/src/house_tools.json`).

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
| `GET /v1/portfolio` | The wallet: the superpowers' own record on the iPhone, or the computer's newer copy, with prices read now. |
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
- `wealth_*` and `life_*` (`powers.py`, answered by `HermesPowers` in `Swift/HermesArms.swift`): the computer's
  superpowers, its own engine inside the app: the portfolio against B3's quotes, the CVM's and FundosNet's documents
  and the Banco Central's figures (read on demand, as on the computer: about a minute at most), the funds, the
  events, a company and the user's thesis, trades, ceilings (Bazin, Graham, Lynch), the twin plan, taxes, the
  decision journal and committee, the economy, and the studies, cards, English, training and the day. Its files are
  `Application Support/AXYON/powers` (`powers.json`, `powers-market.json`), the computer's two. The positions, cash
  and ceilings a paired computer gives (`windows/src-tauri/src/powers/mod.rs`, `phone_copy`; capability `portfolio`
  of the phone link) replace the iPhone's at each newer copy: the computer and its sheet stay the record when there
  is one. The content kit stays on the computer.
- The alerts with AXYON closed (`Swift/AXYONAlerts.swift`, opt-in in Settings): a background app refresh
  (`fr.louisraille.Coucou.powers-watch`, at iOS's pace) asks the engine what is worth telling
  (`axyon_powers_watch`: the computer's `ceiling::to_tell` and `alerts::to_tell`, the sources read for 20 seconds at
  most first) and each alert becomes a notification in the user's language, once; between 7 and 11, once a day, the
  day's briefing (`engine::day_plan`, as the computer's `brief`) comes as one more.
- `coucou_quote` (`coucou.py`): B3 prices read now from Google Finance.
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
- The files the user sends (`attachments.py`, `Swift/AXYONInbox.swift`): the chat's **+** copies photos (library,
  camera) and files (Files, iCloud Drive) into `Application Support/Hermes/inbox`, a photo as a JPEG at most 2048
  pixels long, a PDF's text read by PDFKit (its scanned pages by Vision's text recognition) into `<file>.txt.extracted`.
  `POST /v1/runs` takes them (`attachments`, paths in the inbox only); the turn gets the documents' text (60 000
  characters at most, Word and Excel through Hermes' own `tools/read_extract.py`) and the photos as images when the
  brain sees (Hermes' `agent/image_routing.py`, as its gateway does), or described by `vision_analyze` when it does
  not. `iphone_files` reads them again. The session keeps their names and paths, not their text.
- Google (`google.py`, `Swift/AXYONInbox.swift`'s `AXYONGoogleView`): `google_account`, `gmail`, `google_sheets`,
  `google_drive`, `google_calendar`. The google-workspace skill signs in by running `setup.py` in a terminal, which
  iOS forbids: the same OAuth is made in Hermes' process, with the user's own Desktop-app client and PKCE, Google's
  REST APIs over urllib, and the skill's own files (`google_client_secret.json`, `google_token.json`). The redirect is
  `http://127.0.0.1:<port>/`: the app opens Google's page inside itself (SFSafariViewController), so the host hears
  the browser come back and finishes alone (only for its own random `state`); a pasted address finishes it otherwise.
  `gmail` send and reply show the e-mail whole in the chat (`HermesMailAsk`) and go only on the user's tap on Send.
- Hermes' scheduled tasks (`scheduled.py`): its own `cronjob` tool (offered once `HERMES_EXEC_ASK` is set, as Hermes'
  gateway sets it), its scheduler ticked every minute in the host while the app runs, and by `POST /v1/cron/tick` when
  iOS wakes the app (`AXYONAlerts.backgroundLook`, which starts Python only when `cron/jobs.json` says a task is
  due). Each result Hermes' scheduler delivers becomes an iPhone notification (`cron.scheduler._deliver_result` is
  the iPhone's); `[SILENT]` stays silent.
- `iphone_alerts`: the alerts above, turned on or off, or a look now, from the chat.
- Investing (`Swift/AXYONInvest.swift`, `Swift/AXYONInvestViews.swift`, `app/axyon_hermes/invest.py`): the Wallet
  tab's screen. The engine (`coucou_powers::invest::app`, through `axyon_powers_call` with `invest_*` names, never
  offered to the model) gives the screen's JSON and keeps everything: the committee (`committee.rs`: five minds and a
  chair, facts numbered with their sources, legal and risk veto), the reader's library (`library.rs`,
  `powers-docs.json`: what is due from FundosNet and the CVM's IPE list, a download told apart, the figures with their
  pages, what changed), the theses (`thesis.rs`), the learning (`learn.rs`) and the week's script (`weekly.rs`). The
  engine downloads a document (`invest_docs` `fetch`); the app reads a PDF's pages (PDFKit, Vision for scans) and
  gives them back (`ingest`). The brain is asked once, with no tools, by the host: `POST /v1/invest/digest {key}` and
  `POST /v1/invest/committee {proposal, ticker, reason, document}` (the engine's brief, the brain's JSON, the engine
  keeps the verdict). The reader runs when the app comes to the foreground (at most every half hour), one text-only
  document in a background refresh; the brain's digests and the committee's own meetings once AXYON Core runs; the
  reader's and the theses' news come with the alerts (`axyon_powers_watch`: kinds `document` and `thesis`). Hermes'
  own tools `wealth_documents` and `wealth_learn` read the same library.
- Hermes' own `memory`, `skills`, `todo`, `clarify`, `session_search`, `vision`, `delegation` and `image_gen` (with
  the brain's own image maker: ChatGPT's or OpenRouter's; the images made in a run go to the app as an `images`
  event and show under the answer).

## What iOS does not allow

iOS suspends an app soon after it leaves the screen: Hermes thinks while AXYON is open, and an answer under way gets
the half minute iOS grants to end (then a notification brings it). Reminders and timed notifications are iOS's own,
so they come with the app closed; scheduled tasks run when iOS wakes the app. iOS runs no other process: Hermes'
terminal, code execution, browser and shell-backed file tools are not there, and no app may see or touch another
app's screen, so computer use (cua-driver) cannot exist on an iPhone.

## Tests

```
python3 tests/test_units.py                                   # the tools, quotes, portfolio, web, powers, bridge
(cd ../../windows && cargo test -p coucou-powers-ffi)         # the superpowers' C interface
python3 tests/test_home.py                                    # the home (needs websockets and openssl)
HERMES_SRC=…/hermes-agent PYTHON=…/venv/bin/python python3 tests/test_host.py   # the host with Hermes, end to end
HERMES_SRC=…/hermes-agent PYTHON=…/venv/bin/python python3 tests/test_embedded.py   # the app's C start of Python
bash ../../scripts/test-pc-link.sh                            # HermesWire.swift with the phone link's wire
```

`test_host.py` runs the host under `tests/ios_like.py`, which takes away what iOS does not have (subprocess, fork,
`sys.platform` is `ios`), against `tests/fake_model.py`, a local OpenAI-compatible model that streams and calls tools.
`test_embedded.py` builds `bridge/AXYONPython.c` with `tests/embedded_main.c` against that Python and starts it as the
app does, on a bundle laid out as the app's (with links), isolated from the computer's environment: the host must
listen and Hermes become ready, and a start that fails must end at once with its traceback where the app shows it.

Both take `APP_PACKAGES`, a folder with only the packages the app carries, installed for the computer, so a package
Hermes needs and the app lacks fails here rather than on an iPhone (test_host.py then wants a `PYTHON` with no packages
of its own, such as the one uv installs):

```
uv pip install --python …/python3.14 --target /tmp/app-packages --no-deps \
    -r requirements-pure.txt -r requirements-ios-wheels.txt pydantic-core==2.46.4 jiter==0.16.0
APP_PACKAGES=/tmp/app-packages HERMES_SRC=… PYTHON=…/python3.14 python3 tests/test_host.py
```
