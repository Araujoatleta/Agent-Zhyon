# Changelog

## Unreleased

- **Mochi in the iPhone's Dynamic Island, for a Windows or Linux computer** — the sideload build has no Apple push (a free Apple Account cannot have it), so the iPhone now starts Mochi's Live Activity itself: with Coucou open, while an agent on the computer is working or waiting for you, Mochi comes into the Dynamic Island and the Lock Screen with its step and time, and Allow (Face ID) and Deny, answered through the link. iOS pauses the app half a minute after you leave it, so the island then says "Open Coucou to see the latest" until you open it again, and ntfy still announces what comes while it is closed. Guide: [docs/IPHONE_PC.md](docs/IPHONE_PC.md#mochi-in-the-dynamic-island) (Português: [docs/IPHONE_PC.pt-BR.md](docs/IPHONE_PC.pt-BR.md#o-mochi-na-ilha-dinâmica))
- **Zheus One, and the island in Liquid Glass** *(Windows and Linux)* — "Zheus, I'm on my way home" is now one mission instead of fifteen orders: your assistant looks at the whole house (the **World State**: each device you share, who is home, each room's temperature, and what ties them together), works out a plan — the air conditioning on in time, the vacuum done before you arrive, the lights after dark, the TV, your day's summary — and shows it with **Foresight**: what clashes and the ways around it ("you arrive in 25 min, the cleaning takes 35: clean only the bedroom now?"). Nothing runs until you answer yes, in the chat, on your phone or by clicking Approve on the island; a device set to ask still asks. At your arrival Zheus welcomes you with what was done and how your day went. The **Autopilot** keeps standing goals (less energy wasted, a tidy house, your day prepared, your business moving on), each off, suggesting, or acting within your limits — spending, publishing, writing to people and the house's safety are never done alone. "Show our situation" puts the house, the plan and the decisions waiting full screen on the TV or projector (**TV mode**). Mochi's home now has robot vacuums (room by room with Home Assistant 2026.3), air conditioners and temperature and humidity sensors through Home Assistant, and Hermes gets the `zheus_world` and `zheus_mission` tools. And the island has a new look, **Liquid Glass** — frosted cards over a soft field of color, light along the rims, each device a tile with its own switch — as does Settings → Zheus One, its cards in glass with Off · Suggest · Act for each goal; the classic black island is one click away. Off by default; tested with simulated devices so far. Guide: [docs/ZHEUS_ONE.md](docs/ZHEUS_ONE.md) (Português: [docs/ZHEUS_ONE.pt-BR.md](docs/ZHEUS_ONE.pt-BR.md))
- **Hermes runs steadily, with a diagnosis in plain words** *(Windows and Linux)* — the gateway Coucou runs is now sturdier and self-healing, so you never touch PowerShell: Coucou tells a closed port from another program on it, from Hermes refusing the key, from Hermes ready (a real health check, not just "the port answers"); a gateway already on the port is adopted instead of starting a second; a gateway that falls is brought back with a growing wait (2, 4, 8, 16, 32 s, then given up after five quick stops, never a tight loop); an adopted gateway is watched every minute and before each message; and when Coucou runs Hermes it now starts with Windows, so the gateway comes back after a reboot. Settings → Hermes Agent has a **Diagnosis** panel (all in your language): Hermes installed, the API up, the key valid, with **Repair the key** (a new key written to Hermes' `.env`, the old file kept, and the keystore — the authentication is never turned off), **Let only Coucou run Hermes** (removes Hermes' own start-up entry when two of them fought for the port), and **Copy the report** (no secret in it). **Connect** now reads the `API_SERVER_KEY` from Hermes' `.env` by itself when you leave it blank.
- **Android TVs, projectors and TV boxes in Mochi's home** *(Windows and Linux)* — over ADB, Android's network debugging (port 5555), spoken by Coucou itself: add the device's address (or **Add** next to it in the network search), press **Pair**, and choose **Allow** on its screen. Then Mochi wakes it and puts it to sleep, sets its volume (up, down, a number, mute), opens the apps of its own list, and reads whether it is on, its volume and the app on screen. The RSA key Android asks for is made by Coucou and kept in the system's keystore; only Coucou's own fixed commands are ever sent. Tested on a simulated device so far. Guide: [docs/SMART_HOME.md](docs/SMART_HOME.md#android-tvs-projectors-and-tv-boxes-adb) (Português: [docs/SMART_HOME.pt-BR.md](docs/SMART_HOME.pt-BR.md#tvs-android-projetores-e-tv-boxes-adb))
- **Hermes on your screen, quick** *(Windows and Linux)* — with **Let it use this computer** on, Hermes no longer reaches for its own browser, which runs hidden (it is now off for the API server Coucou talks to, and given back when the switch goes off; a Coucou updated with the switch already on turns it off once, at its next start). Coucou tells Hermes the short way with each message: find the exact link, open it in your own browser with one terminal command (a YouTube video link plays on its own), and click only for what a link cannot do, rather than a search page clicked through and its element file searched, each step a round trip to the model. And **Its speed**: **Fast** has Hermes think less before each step, **Thorough** longer, given with each message without restarting Hermes. Guide: [docs/HERMES.md](docs/HERMES.md#its-speed) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md#a-velocidade-dele))
- **A portable Coucou fleet on your VPS** *(Windows + Android)* — every computer can now have its own revocable relay key and a one-click Windows enrollment ZIP. `vpsctl` installs/updates the stack, enrolls or revokes one computer without disturbing the others, validates the public route and exports the whole VPS state as one portable folder. A dedicated VPS gets automatic HTTPS through Caddy; an existing Nginx VPS keeps the private `127.0.0.1` mode. The relay remains content-blind and every pairing keeps its own end-to-end key. Guide: [docs/VPS_ARCHITECTURE.pt-BR.md](docs/VPS_ARCHITECTURE.pt-BR.md)
- **Hermes uses your computer, and you pick its model** *(Windows and Linux)* — Settings → Hermes Agent → **Let it use this computer (screen, mouse, keyboard)**: Hermes then sees the screen and clicks, scrolls and types in your apps and websites, in the background, without taking your mouse ("open YouTube and play the trailer"), with its own `computer_use` tool and cua-driver, installed by Hermes' own command; what it does on the screen waits for your OK, on the island or the phone. Off by default. And **Its model**: the ChatGPT subscription's models (Hermes' list, best first) or one you type, instead of opening Hermes; the usual one is now `gpt-5.6-sol`, since an account refused `gpt-5.5`. Also: when Hermes' installer stops, Settings now says why (the cause from its output, the whole output in `%LOCALAPPDATA%\Coucou\hermes-install.log`), a step that fails is tried once more, and uv trusts the certificates the computer trusts (company networks). Guide: [docs/HERMES.md](docs/HERMES.md#using-the-computer-screen-mouse-keyboard) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md#usar-o-computador-tela-mouse-teclado))
- **Your assistant, set up by the installer** *(Windows)* — the setup.exe has a new page, **Your assistant**, right after the install folder: the brain (your ChatGPT subscription first, or an API key, stored straight into Windows Credential Manager and never in a file), the assistant's name and tone, skill packs, and whether your iPhone may change them, with what will be downloaded, installed and written said before you click. At its first start Coucou installs and sets up Hermes with those choices and opens Settings on Hermes Agent: after that, just talk to it. English, French and Portuguese. Guide: [docs/HERMES.md](docs/HERMES.md#straight-from-the-installer-setupexe) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md#direto-no-instalador-setupexe))
- **Your ChatGPT subscription as the assistant's brain, its tone and its skills** *(Windows and Linux)* — Settings → Hermes Agent: pick **ChatGPT (your subscription)** and Coucou runs Hermes' own sign-in, shows the code and opens OpenAI's page (ChatGPT Plus or Pro; the sign-in stays in Hermes' own files, Coucou never reads it). Pick a tone (friendly, direct, professional, playful, butler, or your own words) that Coucou writes to Hermes' SOUL.md, keeping a copy of the one there before, and skill packs (free web search, news feeds) that Coucou installs with Hermes' own commands and never removes when you installed them yourself. Changing the brain restarts Hermes with the new one. Guide: [docs/HERMES.md](docs/HERMES.md#chatgpt-your-subscription-no-key) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md#chatgpt-a-sua-assinatura-sem-chave))
- **The iPhone app, redesigned** — deep dark Liquid Glass, with **Chat**, **Agents**, **Home** and **Settings** tabs for a Windows or Linux computer: the chat with Mochi or your assistant as a tab of its own, kept between launches, with Allow and Deny right in the conversation (Face ID), the model and Mochi's hands at hand, quick prompts, and copy, read aloud and send again on each message. Settings → Assistant changes its name, tone, brain (Face ID; the ChatGPT sign-in code shows on the phone), skills and whether Coucou runs it; the Home tab runs Mochi's home (TVs, lights, routines; what asks first asks for Face ID); plan usage, the files a session changed with their diffs, and service cards come along too. Its screens now also speak Portuguese, French and Spanish (a few messages from the link are still in English). Guide: [docs/IPHONE_PC.md](docs/IPHONE_PC.md#the-assistant-and-mochis-home-from-the-iphone) (Português: [docs/IPHONE_PC.pt-BR.md](docs/IPHONE_PC.pt-BR.md#o-assistente-e-a-casa-do-mochi-pelo-iphone))
- **Phone link: the assistant and Mochi's home** *(Windows and Linux)* — two new switches in Settings → Phone, both off by default (the installer's page can turn them on): "Let the phone change the assistant" and "Let the phone use Mochi's home", with the new `assistant` and `home` capabilities in [the protocol](docs/ANDROID_LINK.md). Every request is checked on the computer, rate-limited and answered once; a device or routine that asks first is acted on only after the phone's own Face ID; Hermes is never installed from the phone; and no address, key, path or e-mail ever crosses the link
- **Zheus: Coucou installs and runs Hermes for you** *(Windows and Linux)* — Settings → Hermes Agent: give your assistant a name ("Zheus"), pick its brain among the AIs Coucou already has a key or a server for, read what will happen, and click Install. Coucou downloads Hermes' official installer at a pinned version, checks its SHA-256 before anything runs, runs its steps with no question asked (each one on screen), sets Hermes up with its own `hermes config set`, then runs Hermes' server on this computer only (127.0.0.1) whenever Coucou runs, with a private key, and connects the chat, the phone and Mochi's home to it. The AI key goes to Hermes in memory only, never into a file; Hermes ends with Coucou. A Hermes already installed is run as it is, and one already running is never started twice. Also fixed: Hermes' key could not be kept in the keystore, so Connect always failed. Guide: [docs/HERMES.md](docs/HERMES.md#the-simplest-way-coucou-installs-and-runs-it-all-zheus) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md#o-jeito-mais-simples-o-coucou-instala-e-liga-tudo-zheus))
- **iPhone away from home, directly** *(Windows and Linux, and the iPhone app)* — Settings → Phone → Away from home, directly: Coucou asks your own router (UPnP) to open the link's port and reads your home's internet address from it, and the paired iPhone learns those addresses (IPv6 and IPv4) over the link. Away from home it tries the local network first, then goes straight to your computer, with the same pinned certificate and pairing code: no relay, no VPS, no one's server, and only the router is asked. Callers from the internet get a small pool of their own, so they never take the phone's place at home. Settings says plainly what cannot work (UPnP off, a provider sharing one IPv4 between homes, IPv6 kept closed by the router). Off by default; turning it off, or quitting Coucou, closes the port again. Guide: [docs/IPHONE_PC.md](docs/IPHONE_PC.md#away-from-home-directly-no-ones-server) (Português: [docs/IPHONE_PC.pt-BR.md](docs/IPHONE_PC.pt-BR.md#fora-de-casa-direto-no-pc-sem-servidor-de-ninguém))
- **Hermes Agent as the chat's brain** *(Windows and Linux)* — Settings → Hermes Agent connects the [Hermes Agent](https://github.com/NousResearch/hermes-agent) running on the computer (its API server, with its key kept in the system's keystore and only sent to that address): the chat on the island and on the paired phone is then answered by Hermes, with its memory, skills and tools (Google Calendar and Gmail, Spotify, Home Assistant, scheduled jobs…). Each tool Hermes uses shows as a step, an action Hermes wants approved comes up as Mochi's own requests do (Face ID on the phone) and the answer goes back to Hermes, and Stop stops it. And **Mochi's home for Hermes** (off by default): `coucou-hook --mcp` lets Hermes, or another MCP client on the computer, use the devices of Mochi's home with Mochi's rules (only the ticked devices, a click for what asks first, every action in the record). Tested against a simulated Hermes so far. Guide: [docs/HERMES.md](docs/HERMES.md) (Português: [docs/HERMES.pt-BR.md](docs/HERMES.pt-BR.md))
- **Mochi's home** *(Windows and Linux)* — Settings → Mochi's home lets the chat, on the computer or on a paired phone, use the devices of your house: "Mochi, liga a TV da sala e abre o YouTube". Through Home Assistant's REST API (media players, lights, switches) with a long-lived token, or straight to the TV: Samsung TVs since 2016 through their own remote control API (they ask once on their screen whether to allow Coucou, then give it a token; apps from the TV's list, volume and mute through its UPnP renderer) and Philips TVs over JointSpace (Android TVs paired once with the PIN they show); their certificates pinned, Wake-on-LAN from deep standby. A TV of another kind is not added: Coucou says which known ways in it answers on. Mochi sees only the devices you tick and does only what each one reported it can do; every action becomes one call of a fixed list, plugs and switches wait for Allow (Face ID on the phone), and after each action he says what the device reports, not what he hoped. Routines ("Good night", "Movie mode"), a network search (mDNS, SSDP), a test button for every action, and a record of every action on this computer. Tokens and pairings in the system's keystore; only local addresses are contacted. Off by default; tested on simulated devices so far. Guide: [docs/SMART_HOME.md](docs/SMART_HOME.md) (Português: [docs/SMART_HOME.pt-BR.md](docs/SMART_HOME.pt-BR.md))
- **Answers read aloud** *(Windows and Linux)* — Settings → Mochi's home → Voice: the island's chat reads Mochi's answers with the computer's own voices, in the app's language (only voices that run on the computer, so nothing is sent anywhere). Off by default. On Windows, Windows + H dictates in the chat
- **Talk to Mochi from the iPhone** — the chat with a Windows or Linux computer has a microphone: a pause sends what you said, and the answer is read aloud with the iPhone's voices (the chat's menu turns it off and picks the dictation language)
- **iPhone notifications with Coucou closed** *(Windows and Linux)* — Settings → Phone → Notifications with Coucou closed: iOS pauses the sideload iPhone app shortly after you leave it, so a request still waiting while no phone is connected is announced through [ntfy](https://ntfy.sh) (ntfy.sh or your own server). It only says who is waiting, never the command; a tap opens Coucou. The topic and an optional access token are kept in the system's keystore. Off by default
- **Mochi's hands** *(Windows and Linux)* — Settings → Mochi's hands lets the chat act on your PC, Codex-style, with whichever provider the chat uses: it lists, reads and searches your files, writes and edits them, runs commands (PowerShell or bash), fetches or opens web pages. Off by default. Looking inside the working folder you choose needs no click; every change, command and web request waits for Allow, on the island or on a paired phone, and so do paths outside the folder and files that look like keys or passwords. Each step shows in the chat as he works, a file change comes with its diff, and Stop ends the running command and everything it started
- **Coucou on iPhone pairs with Windows and Linux** — scan the pairing code of Coucou for Windows or Linux with the iPhone's Camera: sessions, Allow with Face ID and Deny, answers to Claude Code's questions and the chat with Mochi (his hands included) over your Wi-Fi, with the computer's certificate pinned, no iCloud and no server. A GitHub Actions workflow builds an unsigned sideload `.ipa` for a free Apple Account. Guide: [docs/IPHONE_PC.md](docs/IPHONE_PC.md) (Português: [docs/IPHONE_PC.pt-BR.md](docs/IPHONE_PC.pt-BR.md))
- **Phone link** *(Windows and Linux)*: Settings → Android phone is now Settings → Phone (Android and iPhone), with a new switch, "Let the phone use Mochi's hands" (off by default), and the new `agent` capability in [the protocol](docs/ANDROID_LINK.md)
- **Coucou for Android**, by @Rhytam23 — your agents, Allow and Deny with your fingerprint or screen lock (even on a locked phone), answers to Claude's questions, chat through the computer's own keys, plan usage and services, an optional island over other apps, a widget, a quick-settings tile and an ongoing notification. It pairs with Coucou 0.4.0 or later on Windows or Linux. Android 11 or later, English only; the first signed APK is on its way. Build it today from [`android/`](android/README.md) (#501 by @Rhytam23)

## Windows and Linux 0.4.0 — October 11, 2026

- **Your Android phone** — Settings → Android phone: the link for [Coucou for Android](android/README.md), an opt-in server on your local network, off by default (nothing listens until you turn it on), paired with a QR code and a pinned certificate. "Pair again" disconnects every phone, a command too long to show in full can only be allowed on the computer, and each extra (session details, answers, chat through your own keys, file changes, plan usage, services) has its own switch (#500 by @Rhytam23)
- **Plan usage**: Settings and the README say that the Claude gauge needs the Claude Code command-line tool, not the Claude desktop app (#479 by @Rhytam23)
- Newer versions of the GitHub actions in the build workflows (#499 by @Rhytam23)

## 0.3.0 — October 11, 2026

- **« OK Coucou »: talk to Mochi** *(macOS, GitHub build)* — say « OK Coucou » and ask anything, in English or French. Claude is Coucou's brain, with your own Anthropic API key: add or swap pills, play music, ask how much you made on Stripe today, your GitHub stars, your Vercel deploys, what your agents are doing, search the web, open an app. It keeps the thread, asks back when something is missing, and listens again for your answer. Speech recognition runs on your Mac; your words and the data Coucou reads to answer go to Anthropic, never audio. Turn it on in Settings → Voice
- **Emails by voice** — « send an email to Enzo to tell him the image is ready »: Coucou finds the address in your contacts, writes the email, opens it in the notch, and you drop an attachment on the notch if you want one. Nothing is sent until you click Send
- **Mochi lives the conversation** — he listens (his eyes follow your voice), thinks, talks back, and winks when he's done. The answer shows under the notch: click it to read it in full. You can open the island while he talks
- **His voice** — the Mac's voices with Woman / Man and pitch, the best installed voice picked automatically (Premium voices recommended), or ElevenLabs with your own key
- **Smoother and lighter** — animations at the display's full rate, a hidden island that really rests, and less background work everywhere (pollers, logs, Mochi on the desktop)

## Windows and Linux 0.3.0 — October 9, 2026

The first Linux release since 0.1.1, so on Linux it also brings everything in Windows and Linux 0.2.0 below.

- **Open on hover**: the island opens when the pointer reaches it and folds again shortly after it leaves; a click inside keeps it open — Settings → General, off by default
- **Your own sounds**: put a file named like one of Mochi's sounds (`finish.wav`, `approval.mp3`, `greet.m4a`…) in the sounds folder to replace it — Settings → General → Open sounds folder, Reload sounds
- **A colour of your own for each Mochi**: click a pill's colour dot in Settings (#320 by @shakibbinkabir)
- **Mochi to the desktop from the keyboard**: `Ctrl+Alt+D` sends him out and brings him home, like ⌃⌥D on the Mac
- **Keyboard in the open island**: `Ctrl+↑` / `Ctrl+↓` move through the GitHub lists and `Ctrl+O` opens the row; `Ctrl+E` opens the latest diff
- **Spotify pill** *(Linux)*: the track, play/pause and next on the pill; the cover, progress with seek, shuffle, repeat and volume on the card, read from Spotify over MPRIS — and Mochi dances to it, in the island and on the desktop
- **Global shortcuts on Wayland** *(Linux)*: registered with the desktop through the GlobalShortcuts portal (KDE Plasma 6, GNOME 48+, Hyprland, COSMIC…); where there is none, the commands to bind by hand stay in Settings → Shortcuts
- **Open terminal brings the terminal forward** *(Linux)*: on X11 and KDE Plasma (Wayland too), and the right tab in kitty; GNOME on Wayland still opens the folder in VS Code
- Antigravity runs its tools again: Coucou answers "ask", so Antigravity keeps its own prompt and nothing is allowed on its own (#317 by @kobaltgit)

## Windows and Linux 0.2.0 — October 8, 2026

The Windows and Linux app catches up with the Mac, from 0.1.1 to 0.2.1 — everything except Apple Music and the iPhone, which depend on macOS and iCloud.

- **Agents**: Codex, GitHub Copilot CLI and Muse Code sessions with Allow / Deny in the island; Gemini CLI, Antigravity, Cursor Agent, OpenCode, Amp and Hermes sessions on their own pills; Claude Code sessions from the Claude app on Windows. Install them all from Settings → Agents, which shows the diff and takes a dated backup before writing — one hardened writer for every agent's config (#278 by @Totopo27, #298 by @kobaltgit, #231 by @BeyondBirthday07)
- **Questions**: answer Claude Code's multiple-choice questions from the island; answers are checked against the questions asked, and a question answered in the terminal takes the card down (#216 by @PythonTilk)
- **The permission card** comes up for every agent, brings its pill forward, can be folded without answering, and never decides on its own
- **Chat**: Anthropic, Google AI, OpenAI and OpenRouter, switchable by clicking the model name; local models through Ollama, LM Studio or any OpenAI-compatible server, streamed, thinking hidden; Markdown answers with a copy button; full answers; your first name in the greeting; `COUCOU_ANTHROPIC_BASE_URL` for a gateway, https only (#161 by @4rchila, #166 by @AlphaIsYour, #173 by @AinzDerErste, #206 by @Totopo27)
- **Plan usage**: Claude's 5-hour and weekly limits and Codex's, in the island header (#171 by @AinzDerErste)
- **Live diff**: each file Claude edits shows in the ticker with its +N −M, and a click opens the diff; the finished card shows Claude's final message
- **GitHub**: your pull requests with their CI, reviews waiting for you, the CI of your default branches, alerts when CI turns red or green, and your contribution grid
- **Mochi**: the wardrobe and seasonal outfits, the new greeting and its sound, and Mochi on the desktop (Windows, X11 and layer-shell compositors)
- **Keyboard shortcuts** from anywhere, changeable in Settings → Shortcuts; the defaults never type an AltGr character on French, German, Spanish, Italian or Portuguese keyboards
- **Weekly recap** on Monday mornings, shareable as an image; history stays on your computer
- **Pills**: declare the tools you use and pick your main one; hook-based pills no longer ask for a key; "Open terminal" brings the session's own window forward on Windows
- **10 languages**: English, 中文, हिन्दी, Español, العربية, Français, বাংলা, Português (Brasil), Русский, Bahasa Indonesia — Settings → General → Language (#228; picker from #226 by @alexisrja)
- **File drop** works from every Explorer view, and Cancel works (#240 by @KauaDc, #126); only files a real drop delivered can be read
- **Linux**: auto-close on KDE/Wayland and GNOME (#160 by @4rchila, #136), the island at the top on GNOME (#149 by @betodoescher), pinned to its display on Hyprland and Sway with a display picker (#227 by @chuxclay), GNOME large text no longer cuts the island (#122), an Arch Linux PKGBUILD (#299 by @FabioLukas123, #230)
- The step ticker no longer stops at a session's 20th step (#265 by @PythonTilk), ticker steps keep their own line (from #203 by @shakibbinkabir), `tauri dev` no longer crashes on EBUSY (#202 by @Andrev-91)

## 0.2.3 — October 8, 2026

- **Terminal sessions**: Claude Code sessions started in Warp, Terminal, iTerm, Ghostty, cmux or Orca show in the notch, and "Open terminal" brings back the app the session runs in. Answering their questions and permissions from the notch is opt-in — Settings → Agents (#282 by @guerraOrzc, #238 by @mateuslamaral)
- **Spotify pill**: what's playing in Spotify, with play/pause and skip (#246 by @JhoanG956)
- **A colour of your own for each Mochi**: click a pill's colour dot in Settings → Active pills (#320 by @shakibbinkabir)
- **Dictate in the chat** *(GitHub build)*: click the mic and talk in any of your languages — Coucou listens in your Mac's languages and keyboard layouts and keeps the one you spoke; right-click the mic to pick a language. On-device when the Mac supports it (#116 by @xynlaze234)
- **Open on hover**: the island opens when the pointer reaches the notch and closes when it leaves — Settings → General → Behavior, off by default (asked by felix11zx)
- **Your own sounds**: drop a file named after one of Mochi's sounds in the sounds folder to replace it — Settings → General → Sound (#116)
- Antigravity runs its tools again: Coucou answers "ask", so Antigravity keeps its own prompt and nothing is allowed on its own (#317 by @kobaltgit)

## 0.2.2 — October 8, 2026

- **Choose Mochi's screen**: the screen with the notch, the main screen, a specific display, or "Follow the mouse" — Settings → General → Display. The island moves right away and finds its place again when screens are plugged in or out; Mochi's gaze is right on any display arrangement (#236 by @steeven-th)
- **Claude Desktop pill**: Claude Code sessions started from the Claude app's Code tab get their own pill instead of being ignored; their permission prompts stay in the Claude app (#191 by @samuelmtz2000)
- **Codex plan usage**: a Codex pill next to the Claude one shows your Codex limits and free resets, read from the Codex CLI — Settings → Agents → Plan usage *(GitHub build)* (#244 by @Ace3Z)
- **Questions** show in full, with each option's description (#249 by @Mehdi-fsn)
- A pending permission can be folded away with Escape in the notch or the toggle shortcut, without answering it; Escape typed in another app never hides it (#290 by @jhannesreimann)
- Pills that run on hooks (Claude Code, Cursor, Codex, Gemini CLI, Antigravity, Copilot CLI, Muse Code, OpenCode, Amp, Hermes) say whether their hooks are installed instead of asking for a key (#183 by @TheodoreRiant)
- The chat keeps Claude's whole answer — web-search answers were cut after the first block (#67 by @RAMZI0TO99)
- `~/.claude/settings.json` is never rewritten from scratch when it can't be read, the backup must succeed before anything is written, and nothing is written if the file changed since the preview (#243 by @Fabian-2026)
- The auto-close delay set in Settings is respected (#25 by @Kamasoutra); reopening Coucou brings the island back (#270 by @AndersonPGS)
- Fixed a crash an hour after a file edit (#286 by @i87ce)
- Lighter when hidden: the island checks the pointer 8 times a second instead of 60 while it is hidden and the pointer is away from it

## 0.2.1 — October 7, 2026

- **Hermes Agent** (Nous Research, open-source): sessions appear in the notch — live tool steps, the final response when done, and the platform (Telegram, Discord…) when running via the gateway. Install from Settings → Agents → Hermes: it writes a small Python plugin to `~/.hermes/plugins/coucou/` and enables it in `~/.hermes/config.yaml`, with the same preview, backup and confirmation flow as other agents *(macOS, GitHub build)* (#288)
- Hermes approval requests show a "⏳ Approval pending in Hermes" step in the notch. Approving directly from the notch isn't supported yet — current Hermes versions (0.15.x) don't expose the transport API. The Approvals toggle in Settings will activate automatically once Hermes adds it (#288)
- Coucou never blocks Hermes: if the app is closed or unreachable, Hermes continues normally and handles approvals itself (#288)

## 0.2.0 — October 6, 2026

- GitHub Copilot CLI and Muse Code sessions show up in the notch: see every step live and approve or deny permissions right from the island. Install from Settings → Agents → Copilot CLI / Muse Code, which shows what will change in your config and backs it up before writing *(GitHub build)* (#263)
- OpenCode sessions appear in the notch via a small JavaScript plugin: install it from Settings → Agents → OpenCode. Same installer flow — preview, backup, confirm. OpenCode never blocks on the plugin (fire-and-forget), so Coucou never slows it down *(macOS, GitHub build)* (#263)
- Amp sessions appear in the notch the same way, via a TypeScript plugin: Settings → Agents → Amp *(macOS, GitHub build)* (#263)
- Weekly recap: on Monday morning, the first time an agent starts working or your Mac wakes, Coucou shows a card for the past week — time spent, sessions, files and lines changed, commands run, permissions and questions, plus your top agent, top project, busiest day and longest session. Open it any time from the menu bar with "Weekly recap" (#264)
- Share your week as a 1080 × 1920 image with Mochi: copy it, save it or share it from the notch. A privacy toggle lets you hide project names before sharing (#264)
- Everything stays on your Mac: the recap reads from a local history file (12-week rolling window) that never leaves your machine. Clear it any time in Settings → General → Weekly recap (#264)
- Coucou now speaks English, 中文, हिन्दी, Español, العربية, Français, বাংলা, Português, Русский and Bahasa Indonesia. Pick your language in Settings → General → Language, independent of your system locale. Translations welcome — open a pull request (#268)

## 0.1.9 — October 6, 2026

- Services up close on the iPhone: tap a service and your Mac fetches live data from its API — Vercel, GitHub, Stripe, Resend, Cal.com, n8n and Notion. The keys never leave the Mac; the detail is written to your iCloud encrypted (#251)
- Act from the iPhone: Vercel (redeploy, promote to production, cancel a build), GitHub (re-run failed jobs, approve, squash and merge), n8n (activate, deactivate, retry a failed run). Each action runs only if it was offered on an item in the last detail the Mac published for that service, is used once, and must be less than 5 minutes old. Nothing that moves money or sends an email (#251)
- The Live Activity starts 20 seconds after the Mac locks, not immediately, so a quick lock and unlock doesn't spend one of iOS's hourly starts. It starts right away when an agent is waiting for a permission or has a question (#251)
- After unlocking, the Live Activity waits 30 seconds before ending, in case the Mac locks again — useful on a laptop that goes to sleep the moment you put it down (#251)
- If the iPhone has no update token yet (iOS held back the start), and an approval or question is waiting, the Mac starts the activity again once for that specific request (#251)
- Cal.com upcoming bookings work again: the API v2 expects `afterStart` / `beforeEnd`, not `start` / `end`, so the bookings page was empty (#251)

## 0.1.8 — October 5, 2026

- Coucou on iPhone: turn on Settings → General → iPhone (off by default) and your agent sessions show up live in the Coucou iPhone app and its widgets, through your own private iCloud. Project names, commands and questions are encrypted with your iCloud keys; turning it off deletes them (#209, #211, #212, #213)
- Allow or deny a permission from the iPhone: a notification with the command, Deny right from it, Allow behind Face ID. Your Mac only applies a decision meant for the exact request it is waiting on, and the request expires after 2 minutes. The iPhone keeps a history of your decisions (#220)
- Lock your Mac while an agent works and Mochi moves to your iPhone's Lock Screen and Dynamic Island, then comes back to the notch when you unlock. Turn it on under Settings → General → iPhone. It goes through a small relay that only sees the agent's name and state (#221)
- Mochi, the pills and the diff engine now live in a shared package used by both apps; nothing changes in the notch (#210)
- The iPhone sees more of what your Mac sees: every service Mochi (GitHub, Stripe, Vercel, Resend, Cal.com, n8n, Notion) with its latest items, and the last turn of each session with its commands and diffs, all encrypted with your iCloud keys. No API key ever leaves the Mac (#224)
- Send the next instruction to Claude Code from the iPhone (GitHub build, off by default): your Mac picks it up within 15 seconds and continues the session in its own folder (#224)
- Answer Claude's questions from the iPhone: your Mac applies an answer only if it matches the question still waiting (#241)
- The Live Activity counts the time since Mochi left, and shows Allow and Deny while a command waits for you (#232, #241)
- A new coucou sound for Mochi's greeting (#241)

## 0.1.7 — October 4, 2026

- Keyboard shortcuts from anywhere: ⌃⌥Space opens the chat, ⌃⌥A jumps to a waiting permission or question, ⌃⌥T brings your terminal forward, ⌃⌥] and ⌃⌥[ switch pills, ⌃⌥M mutes Mochi, ⌃⌥D sends him to the desktop and back, ⌃⌥G opens the wardrobe, and ⌃⌥W attaches the front window to the chat (GitHub build) (#205)
- In the open island: ⌘← ⌘→ and ⌘1–9 switch pills, ⌘↑ ⌘↓ and ⌘O move through a card's list, ⌘E opens the diff, ⌘↩ sends, ⌘K starts a new chat, ⌘P pins the island (#205)
- Every global shortcut can be changed or turned off in Settings → Shortcuts, which also flags combinations another app already uses. They need no Accessibility permission (#205)
- ⌘⇧N now opens and closes the island (#205)

## 0.1.6 — October 4, 2026

- Mochi on the desktop: drag him out of the notch and drop him anywhere on your desktop. He hangs out there, follows your cursor with his eyes, wears his outfit and dances to your music (#198)
- When Claude needs you, he flies back to the notch with the permission or the question, then returns to his spot once you answer. He does a happy jump when a task finishes (#198)
- Click him to poke him, right-click for the wardrobe, drop him on a window to attach it to the chat (GitHub build), and drop him on the notch or double-click him to bring him home (#198)
- He falls asleep when nothing is going on, and remembers his spot between launches (#198)

## 0.1.5 — October 4, 2026

- Dress Mochi up: right-click him to open the wardrobe and pick a party hat, beanie, crown, witch hat, Santa hat, bunny ears, bow, sunglasses, round glasses, scarf or pumpkin, all drawn in code (#195)
- Auto mode dresses Mochi for the seasons on his own (#195)
- Outfits follow his head in 3D, glasses stay on his eyes, soft parts react when you tap or move him, and outfits come and go with a transition. Only the main Mochi wears them (#195)
- A new launch greeting: Mochi drops into the island, bounces, slides to the side and waves hello with a quick little hand, then comes back, with a new soft whisper of a sound (#196)
- Mochi's body is no longer clipped at two corners during the greeting (#196)

## 0.1.4 — October 3, 2026

- See what Claude is editing, live: each file edit shows up in the session ticker with its +N −M lines, and a click opens the diff right in the notch (#177)
- When Claude finishes, the session card shows its final message instead of the last step, without the shimmer (#177, #179)
- GitHub pill: your open pull requests with their CI status, the pull requests waiting for your review, and the CI of the default branch of your recent repos. Click a row for the list, then an item to open it on github.com (#181)
- GitHub alerts: a badge and a sound when the CI of one of your pull requests turns red or green, when a default branch breaks, or when someone requests your review. Fast CI runs are caught too, and the card refreshes when you open it (#181, #185)
- Your GitHub contribution grid: the last 7 days in the GitHub card header, click it for the past 23 weeks, and click a day for its count (#187)
- The GitHub token needs read access to pull requests and CI: a classic token with the repo scope, or a fine-grained token with read access to Pull requests, Commit statuses and Actions (#181)
- The finished view no longer overflows the card (#179)

## 0.1.3 — October 3, 2026

- Answer Claude's questions from the notch: when Claude Code asks a multiple-choice question, pick an option or type your own answer right in the island, and Reply in terminal hands it back. Update your hooks in Settings to turn it on (#165) — thanks @Vega8991 for the idea (#94)
- Claude plan usage (GitHub build): turn on Settings → Agents → Plan usage to see your 5-hour and weekly limits in a small pill in the notch header, and click it for the details and reset times. Pro and Max plans; your current status line keeps working (#159)
- Chat with local models through Ollama or LM Studio, no API key needed: connect them in Settings → Chat → Local models. Answers stream in, and thinking blocks stay hidden (#156)
- Markdown in chat answers: bold, lists, headings, quotes, and code blocks with a copy button. Links open only when they are web links (#156)
- Apple Music (GitHub build): see what is playing in the notch, play, pause and skip on hover, and Mochi dances along (#144, #153)
- Settings are now organized in a sidebar (#153)
- The chat greets you by your own first name (#154)

## 0.1.2 — October 2, 2026

- Codex support (GitHub build): sessions show up live on the Codex pill, and permission requests get Allow and Deny in the notch. Install from Settings → Codex Hooks, then trust the hooks once with /hooks in Codex (#130) — thanks @lacatu5
- Cursor: Claude Code started in Cursor's terminal shows up on the Cursor pill, and you can answer its permission requests from the notch (#120).
- Pick your main coding tool in Settings → Active pills: VS Code, Cursor, Codex or Antigravity (Codex and Antigravity: GitHub build). It stays on and no longer takes one of the 4 slots (#120).
- The permission card stays in the notch until you answer it: the mouse no longer folds it, and reopening the island shows the request again (#117).
- The permission card also shows when the island is already open, and the pill you were on comes back once you answer (#120).

## 0.1.1 — October 2, 2026

- Declare the tools you use in Settings: Gemini CLI, Antigravity, Anthropic, Google AI and OpenAI pills join the existing ones (Cursor and Codex pills are coming soon), and you pick the main pill.
- Chat now supports Google AI (Gemini) and OpenAI in addition to Anthropic; switch provider and model by clicking the model name in the chat view, on macOS.
- Linux version: the Tauri app now builds for Linux too (AppImage, .deb, .rpm), with the island as a layer-shell overlay on Wayland and Claude Code hooks over a private Unix socket (#21) — thanks @Davy133
- Compact island on screens without a notch (#22) — thanks @Kamasoutra
- Only web links (http/https) open from the notch; other kinds of links from Claude or integrations are ignored (#16) — thanks @Cris1670
- Hook socket limited to your own user account, with size and time limits; logs no longer keep commands, n8n data or full URLs, and stay under 1 MB (#16) — thanks @Cris1670 and @Vignesh-Thangamariappan
- The island always reopens after folding, and Settings opens below it, resizable — thanks @rouderz
- Choose the Claude model for the chat in Settings; the list comes from your Anthropic account, and Claude Sonnet 4.6 stays the default — thanks @rouderz
- Windows build artifacts are now downloadable from a manual CI run — thanks @MysJofR
- Any agent can talk to Mochi: tag a hook payload with `coucou_agent` (e.g. `nb-hook --agent my-agent`) and it gets its own pill in the island (#7, #9) — thanks @lacatu5
- Gemini CLI and Antigravity (agy) hook support on macOS: install from Settings and their sessions show up in the island — thanks @corefusiion

## 0.1.0 — September 27, 2026

- First release: Mochi lives in your notch, breathing, blinking, with eyes that follow your cursor
- Claude Code sessions: live steps, approve permissions, answer questions, jump to the terminal
- Chat with Claude from the notch
- Drop a file on the notch to ask a question about it or send it by email
- Drag Mochi onto any window to attach it as context
- Integrations: Stripe, n8n, GitHub, Vercel, Resend, Notion, Cal.com
- 28 handcrafted sounds
- Hides when idle, peeks out when you hover
