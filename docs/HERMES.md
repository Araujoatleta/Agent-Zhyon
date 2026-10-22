# Hermes Agent in Coucou (Windows and Linux)

*Português: [HERMES.pt-BR.md](HERMES.pt-BR.md)*

"What's on my calendar today?", "Play my road trip playlist on Spotify", "I'll be home in ten minutes: turn on the
speaker and the bedroom lights". With **Hermes Agent** connected, the chat of Coucou for Windows and Linux (the island,
and the iPhone or Android phone paired with the computer) is answered by [Hermes Agent](https://github.com/NousResearch/hermes-agent),
the open-source agent by Nous Research (MIT), with its own memory, skills and tools: Google Calendar and Gmail,
Spotify, Home Assistant, scheduled jobs, the web, the terminal, and the hundreds of integrations it can add.

Coucou is the face and the hands around it:

- **Mochi shows what Hermes does**, step by step, in the chat on the computer and on the phone.
- **Hermes asks you through Coucou.** An action Hermes wants a person to approve comes up as Mochi's own requests do:
  on the island, or on the paired phone behind Face ID or the fingerprint. Allow or Deny goes back to Hermes.
- **Your voice**: the phone's microphone and the answers read aloud (see [IPHONE_PC.md](IPHONE_PC.md)) work the same
  with Hermes.
- **Mochi's home for Hermes**: Hermes can use the devices of Mochi's home (Samsung and Philips TVs, Home Assistant
  devices, your routines) through MCP, with the same rules as Mochi ([SMART_HOME.md](SMART_HOME.md)).

Hermes is **connected, not copied** into Coucou: it is a large Python agent that keeps itself up to date, and Coucou
talks to it through its official API server. Whatever Hermes learns or installs is there at once, and Coucou stays
small.

## The simplest way: Coucou installs and runs it all (Zheus)

No terminal, no file to edit:

1. In Coucou: **Settings → Hermes Agent**. Under **Your assistant's name**, type what you will call it, **Zheus** for
   instance; pick **Its tone** (below) and click **Save**.
2. Under **Its brain**, pick **ChatGPT (your subscription)** to use your ChatGPT Plus or Pro plan (below), or one of the
   AIs Coucou already has (the Anthropic, OpenAI, Google or OpenRouter key you saved in Settings, or the computer's
   Ollama or LM Studio). Leave **Let it use Mochi's home** ticked to let it use the devices you ticked, and tick the
   **skill packs** you want (below).
3. Read **What Coucou will do** and click **Install Hermes**. Coucou:
   - downloads Hermes' official installer from GitHub, at a pinned version, and **checks its SHA-256** before running
     anything (if it does not match, nothing runs);
   - installs Hermes in `%LOCALAPPDATA%\hermes` (`~/.hermes` on Linux) with everything it needs, asking nothing, showing
     each step (about 1 GB and 5 to 15 minutes). The browser and computer-use tools are left out (`hermes pm install`
     adds them later);
   - sets the brain and Mochi's home with Hermes' own `hermes config set`. **The AI key never goes into a file**: it
     stays in the system's keystore and is handed to Hermes in memory only, when Coucou starts it. With ChatGPT, there
     is no key: Coucou signs you in (below);
   - writes the name and tone to Hermes' `SOUL.md` (an existing one is backed up first), and installs the skill packs
     you ticked;
   - makes a private key between Coucou and Hermes and runs Hermes' server **on this computer only**
     (`127.0.0.1:8642`), whenever Coucou runs. If it stops, Coucou starts it again (a few times), and quitting Coucou
     stops Hermes too;
   - connects the chat, the paired phone and Mochi's home to Hermes.
4. That is all: talk to it on the island's chat or on the iPhone: *"Zheus, what's on my calendar today?"*.

**Run Hermes with Coucou** turns the server on and off (Hermes stays installed). Hermes' own commands still work:
`hermes setup` for more providers, `hermes plugins install spotify` and the rest of section 5.

If **Hermes is already installed** on this computer, the button becomes **Let Coucou run Hermes**: Coucou only runs your
Hermes, without changing its settings (unless you tick **Also give it this brain**). If your Hermes' server already
runs by itself, Coucou starts no second one (Hermes runs one per computer): it uses the key of Hermes' `.env`, or you
connect with the form below.

**Change its brain** gives the Hermes Coucou runs another brain, after the same kind of plan: Coucou stops its server,
sets the new brain and starts it again with the new key (or signs you in to ChatGPT).

To remove Hermes later: turn **Run Hermes with Coucou** off and run `hermes uninstall`.

### ChatGPT: your subscription, no key

Pick **ChatGPT (your subscription)** as its brain to use your **ChatGPT Plus or Pro** plan instead of an API key. This is
Hermes' own sign-in to OpenAI (`hermes auth add openai-codex`):

- during the setup, once Hermes is installed, Coucou opens OpenAI's page (`https://auth.openai.com/codex/device`) and
  shows a code, large, in Settings, with **Copy code**, **Open the page** and **Cancel**. Sign in there with your
  ChatGPT account and type the code: it works for 15 minutes. Only OpenAI's own page is ever opened;
- Hermes keeps the sign-in in its own `auth.json`, in Hermes' folder. **Coucou never reads it** and never sees your
  password: nothing of it goes into Coucou's settings, its logs or the phone link;
- **Its model**: the models of the subscription, best first (`gpt-6-sol`, `gpt-6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`,
  `gpt-5.6-luna`, `gpt-5.5`, `gpt-5.4-mini`, `gpt-5.4`, `gpt-5.3-codex-spark`, Hermes' own list), or **Another model…** to
  type one. Hermes starts again with it. Which ones your plan reaches is OpenAI's call; `gpt-5.6-sol` is the usual one;
- afterwards, **Sign in with ChatGPT** and **Sign out** stay under its brain. Cancel ends a sign-in, and Hermes keeps
  nothing of it. The limits are your plan's (a 5-hour and a weekly window): `hermes usage` shows them.

### Using the computer (screen, mouse, keyboard)

**Let it use this computer** lets Hermes see the screen and click, scroll and type in your apps and websites, in the
background, without taking your mouse ("open YouTube and play the trailer"): Hermes' own `computer_use` tool, through
[cua-driver](https://github.com/trycua/cua) (Windows, macOS and Linux). Turning it on runs Hermes' own commands
(`hermes tools post-setup cua_driver`, then `hermes tools enable computer_use --platform api_server`: the tool is off by
default for the API server Coucou talks to) and starts Hermes again; off turns the tool off and leaves cua-driver
installed. What Hermes does on the screen waits for your OK, as its other actions do: Allow on the island or on the
phone. Off by default.

With it on, Hermes' own browser (its `browser_*` tools, which run hidden from you) is off for the API server too
(`hermes tools disable browser --platform api_server`; turning the switch off gives it back), and Coucou tells Hermes the
short way with each message: find the exact link (one web search), open it in your own browser with one terminal
command (`cmd.exe /c start "" "<link>"` on Windows, `xdg-open` on Linux; a YouTube video link starts playing on its
own), and use the mouse and keyboard only for what a link cannot do. Each step is a round trip to its model, so fewer
steps is what makes it quick. A Coucou updated with the switch already on turns the hidden browser off once, at its next
start.

### Its speed

**Its speed** says how long Hermes thinks before each step: **Fast** (less: it answers and acts on the screen sooner,
best for opening sites, playing videos and clicking), **Usual** (Hermes' own) or **Thorough** (longer). Coucou gives it
with each message (Hermes' `model_options.reasoning_effort`: `low`, nothing, `high`), so it applies from your next
message, without starting Hermes again, and Hermes' own settings stay as they are.

### Keeping it running, and the diagnosis

When Coucou runs Hermes, it keeps the gateway up by itself, on `127.0.0.1` only, with no terminal window to leave open:

- It tells a **closed port** from **another program** on the port, from Hermes **refusing the key**, from Hermes
  **ready** — a real health check, not just "something answers".
- A gateway **already on the port** (one Coucou or Hermes left running) is **adopted**, so Coucou never starts a second
  one beside it and the two never fight for the port.
- A gateway that **falls** is brought back after a growing wait (2, 4, 8, 16, 32 s); after five quick stops in ten
  minutes it is left **Stopped** with the reason, never restarted in a tight loop. An adopted gateway is checked every
  minute and before each message.
- When Coucou runs Hermes it turns **Launch at startup** on, so the gateway comes back after a reboot without you
  opening Coucou.

**Settings → Hermes Agent → Diagnosis** → **Check the integration** shows, in your language: whether Hermes is
installed, whether the API answers, and whether the key is valid. From there:

- **Repair the key** makes a new key, writes it to Hermes' `.env` (your old file kept as `.env.backup-…`) and to
  Coucou's keystore for this address, and restarts the gateway. The authentication is never turned off.
- **Let only Coucou run Hermes** (Windows) removes a gateway service of Hermes' own — a scheduled task or a Startup
  entry — when it runs the gateway beside Coucou's. It removes only that service, never Hermes itself.
- **Copy the report** copies the lines above as text (no key or secret in it), to send to whoever helps you.

**Connect** (for a Hermes you run yourself) now fills the key in by itself from Hermes' `.env` when you leave it blank
and the address is on this computer.

### Its name and tone (SOUL.md)

Under **Its tone**, pick **Friendly**, **Direct**, **Professional**, **Playful**, **Butler (formal, witty)**, or
**Custom** to say it in your own words (up to 1,500 characters), then click **Save**. When Coucou runs Hermes, it writes
the name and tone to Hermes' personality file, `SOUL.md` in Hermes' folder (*"You are Zheus, the user's personal
assistant…"*), and also tells Hermes the tone with each message, so the change applies from your next message.

A `SOUL.md` that Coucou did not write (Hermes' default one, or yours) is **copied first** to
`SOUL.backup-YYYYMMDD-HHMMSS.md`, beside it. Coucou's own ends with the line *"Set up in Coucou (Settings → Hermes
Agent)…"*, and Coucou writes it again whenever the name or tone changes there. If you edit it yourself, remove that line:
Coucou then copies your version aside before it writes the next one. What you type as a tone stays on this computer, in
Coucou's settings and Hermes' `SOUL.md`, and never goes in Coucou's log.

### Skill packs

- **Mochi's home (TVs, lights…)**: the devices of Mochi's home, through MCP (section 6).
- **Free web search (DuckDuckGo)**: Hermes' official `duckduckgo-search` skill, no key.
- **News and feeds (RSS)**: Hermes' official `rss-feeds` skill, no key.

Coucou installs a pack with Hermes' own command, `hermes skills install official/research/… --yes` (official skills
that come with Hermes), and, when you untick it, removes it with `hermes skills uninstall … --yes`, **only if Coucou
installed it**: a skill you installed yourself is never removed. While Coucou runs Hermes, a pack you tick is installed
at once; otherwise during the setup. A pack that does not install is said in Settings, and Hermes runs without it.

Already included with Hermes, with nothing to install: Google Workspace, Notion, maps, Obsidian, e-mail triage and
reminders (its scheduled jobs).

### Straight from the installer (setup.exe)

The Windows installer (`setup.exe`) asks the same questions on a page of its own, **Your assistant**, right after the
install folder: **Set up the assistant now (recommended)**, its brain (ChatGPT first), the API key for a brain that needs
one (kept in Windows Credential Manager, never in a file), the name, the tone, the skill packs, and **Let my iPhone change
the assistant and use Mochi's home**. The page says what will happen before you click: download Hermes' official
installer (a fixed version, checked by SHA-256), install Hermes and its Python in your user folder, set the brain, write
the personality to `SOUL.md` (an existing one is backed up), and install the ticked skills.

The installer leaves your choices, never the key, in `%APPDATA%\Coucou\setup.json`. At its first start Coucou reads
that file once, **deletes it**, and starts the same setup as the Install button, with Settings open on Hermes Agent: with
ChatGPT, that is where the code shows. Then there is nothing left to do: just talk to it. On a reinstall or an update
the box comes unticked, so a Hermes already set up is left as it is.

The MSI installer and Linux have no such page: use Settings → Hermes Agent as above. Whatever you picked in `setup.exe`
can be changed there later, and from the paired iPhone when you let it.

## By hand

The rest of this guide is for installing and running Hermes yourself.

## What you need

- Coucou for Windows or Linux, the version after 0.4.0 (unreleased yet: take the installer from the **Windows
  installer** workflow's artifacts, or build it from [`windows/`](../windows/README.md)).
- Hermes Agent on the **same computer** (Windows 10/11 natively, Linux, macOS or WSL2), with a model provider set up
  (`hermes setup`).

## 1. Install Hermes

On Windows, in PowerShell (no admin rights needed):

```powershell
iex (irm https://hermes-agent.nousresearch.com/install.ps1)
```

On Linux, macOS or WSL2: `curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash`. Open a new terminal,
run `hermes setup` and pick the AI that answers (Nous Portal, Anthropic, OpenAI, OpenRouter, a local model…).

## 2. Turn on its API server

Hermes keeps its files in `%LOCALAPPDATA%\hermes\` on Windows and in `~/.hermes/` elsewhere. Add to its `.env` file:

```bash
API_SERVER_ENABLED=true
API_SERVER_KEY=a-long-secret-of-your-choice
```

Then start the gateway: `hermes gateway` (or `hermes gateway install` to have it start when you log in). It listens on
`http://127.0.0.1:8642`, on this computer only.

## 3. Connect it in Coucou

1. **Settings → Hermes Agent**: leave the address empty (it is `http://127.0.0.1:8642`), type the same secret in **API
   key**, **Connect**. Coucou checks that it is Hermes Agent and that it can run, stream, stop and wait for an approval,
   and shows the name it answers as.
2. **Use in chat**, or pick **Hermes** above the chat box.
3. For the phone: **Settings → Phone**, turn on **Let the phone chat with my AI providers**, then **Choose models**
   next to **Hermes** and tick the name it answers as.

## 4. Talk to it

- "Como está minha agenda hoje?" — Hermes reads your calendar (once you set it up in Hermes, below).
- "Coloca Legião Urbana no Spotify." — Hermes plays it on the Spotify device that is on.
- "Me lembra amanhã às 9h de pagar a conta de luz." — a scheduled job in Hermes.
- From the iPhone, on the way home: "Chego em 10 minutos: liga o som, põe minha playlist e acende a luz do quarto em
  vermelho." — with the speaker and lights in Home Assistant.

Each tool Hermes uses shows in the chat ("Hermes spotify_playback · play…"), and **Stop** stops Hermes.

### Your conversations are kept

Turn the computer off in the middle of a conversation and, the next time, the chat is where you left it: its messages
are back on the island and Hermes goes on in the same session, so it remembers what you were talking about (Hermes
keeps its sessions itself; another provider is sent the conversation again, its last 120 messages at most).

- **+** in the chat (or `Ctrl+K`) starts a new chat. The one before is kept.
- The **Conversations** button, left of the model's name, lists the kept ones, latest first: click one to go back to
  it, **×** twice to erase it, **Reset this conversation** twice to erase the one on screen and start over.

Up to 50 conversations are kept, only on this computer, in `%LOCALAPPDATA%\Coucou\chats.bin` (`~/.local/share/coucou`
on Linux), sealed with a key of their own kept in Windows Credential Manager (the Secret Service on Linux): the file
alone reads as nothing. The phone keeps its own conversation.

## 5. Give Hermes its powers (in Hermes)

These are set up in Hermes, once; Coucou never sees your Google or Spotify credentials.

- **Its name and personality**: edit `SOUL.md` in Hermes' folder ("You are Zheus, …").
- **Google Calendar and Gmail**: ask Hermes "set up Google Workspace": its `google-workspace` skill walks you through
  Google's sign-in (a Google Cloud "Desktop app" OAuth client of your own).
- **Spotify**: `hermes plugins install spotify`, `hermes tools enable spotify`, `hermes spotify login`. Playing music
  needs Spotify Premium and a Spotify app open on some device (the computer, the phone, a Connect speaker).
- **Home Assistant** (lights, air conditioning, speakers, plugs): `hermes plugins install homeassistant`, then
  `HASS_URL` and `HASS_TOKEN` in Hermes' `.env`.
- **A wake word on the computer**: Hermes' own desktop app listens for one (Porcupine has a built-in "Jarvis"; the
  `sherpa` engine takes a phrase you type). See Hermes' "Wake word" guide.
- **A morning summary on the phone**: a Hermes scheduled job (`hermes cron add …`) can deliver to ntfy, the same
  notification app Coucou uses for the iPhone.

## 6. Mochi's home for Hermes (MCP)

**Settings → Hermes Agent → Mochi's home for Hermes** (off by default) lets Hermes, and any other agent on this
computer that speaks MCP, use Mochi's home. Add what Settings shows to Hermes' `config.yaml`, then restart
`hermes gateway`:

```yaml
mcp_servers:
  coucou:
    command: 'C:\Users\you\AppData\Local\Coucou\bin\coucou-hook.exe'
    args: ["--mcp"]
```

Hermes then has `home_devices`, `home_control` and `home_routine`. `coucou-hook --mcp` runs nothing itself: it hands
each call to Coucou, which applies Mochi's rules — only the devices ticked for Mochi, only the actions each reported, a
click (Face ID on the phone) for plugs, switches and the devices set to ask, every action in the home's record under
"Hermes Agent". Mochi's hands (files, commands) are never reachable this way. When Coucou is closed, Hermes is told so.

## Safety

- **The key** is kept in the system's keystore (Windows Credential Manager, Secret Service on Linux) with the one
  address it was typed for, and only sent there: to this computer, or over https. Never in a file, never in the logs.
- **What goes to Hermes**: your message, the text of a file you dropped in the chat, and a short note on where the
  message comes from. No other key, no setting.
- **Approvals**: Coucou shows Hermes' question with its reason ("recursive delete"); with no answer within about four
  and a half minutes it says no (Hermes itself waits five minutes by default). Stop asks Hermes to stop.
- **What Hermes may do on its own** is Hermes' own policy (its `approvals` settings): Coucou shows every tool it uses
  and asks you about everything Hermes asks about. Read Hermes' security guide before giving it access to things that
  matter.
- **One at a time**: a Hermes turn and a turn of Mochi's hands share the same place; the second waits for the first.

## Limits, honestly

- **Tested on a simulated Hermes**: the API server's runs, events, approvals and stop, and an MCP client, not yet
  against a live Hermes. Hermes' API is documented and versioned (`/v1/capabilities`); Coucou refuses a server that is
  not Hermes or is too old for it.
- **Hermes on the same computer** (or reachable over https). A key never goes in clear text to another machine.
- **The quality of the answers** is the model's you picked in Hermes.
- The Hermes plugin of **Settings → Agents** (Hermes sessions on their own pill) still works, and can stay on.

## For developers

- `windows/src-tauri/src/hermes_setup.rs`: Zheus — the pinned installer (`COMMIT`, its SHA-256 per platform), its stage
  protocol (`-Manifest`, `-Stage NAME -Json -NonInteractive`), `hermes config set` for the brain and Mochi's home, and
  the gateway Coucou runs (`hermes gateway run --external-supervisor`, a Windows job object so it ends with Coucou,
  restarts, the key from Hermes' own `.env` when it has one). To move to a newer Hermes: change `COMMIT` and both
  hashes (`git cat-file blob <commit>:scripts/install.ps1 | sha256sum`). Also the ChatGPT sign-in
  (`hermes auth add|status|logout openai-codex`, its output read for the code; `PYTHONUNBUFFERED=1` or Python holds
  the code back in the pipe), `soul_text` and the `SOUL.md` backup, and `skill_commands`. The window-free functions
  (`status_of`, `begin_setup`, `set_assistant`, `set_skills`, `login_start`, `login_cancel`, `logout`, `gateway_set`)
  are what Settings, the installer's page and the phone link call.
- `windows/src-tauri/src/first_run.rs`: `setup.json` from `setup.exe` (UTF-16LE with a BOM, or UTF-8), read once and
  deleted.

- `windows/src-tauri/src/hermes.rs`: the API server as Coucou uses it (capabilities, runs, their events, approvals,
  stop), and the key bound to its address. `windows/src-tauri/src/agent/hermes.rs`: a turn with Hermes on the island
  and the phones (steps, cards, Stop). `windows/src-tauri/src/agent/mcp.rs`: Mochi's home for other agents.
  `windows/hook/src/mcp.rs` and `house_tools.json`: `coucou-hook --mcp`, the MCP server over stdio.
  `windows/src/settings/hermes.ts`: Settings → Hermes Agent.
- Tests: `cargo test --workspace` (a simulated Hermes API server with its event stream, approvals answered or settled
  without the card, Stop, a broken stream reopened where it broke; the MCP server's messages) and `npm test`
  (`tests/hermes.test.mjs`).
