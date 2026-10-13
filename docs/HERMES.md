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

- `windows/src-tauri/src/hermes.rs`: the API server as Coucou uses it (capabilities, runs, their events, approvals,
  stop), and the key bound to its address. `windows/src-tauri/src/agent/hermes.rs`: a turn with Hermes on the island
  and the phones (steps, cards, Stop). `windows/src-tauri/src/agent/mcp.rs`: Mochi's home for other agents.
  `windows/hook/src/mcp.rs` and `house_tools.json`: `coucou-hook --mcp`, the MCP server over stdio.
  `windows/src/settings/hermes.ts`: Settings → Hermes Agent.
- Tests: `cargo test --workspace` (a simulated Hermes API server with its event stream, approvals answered or settled
  without the card, Stop, a broken stream reopened where it broke; the MCP server's messages) and `npm test`
  (`tests/hermes.test.mjs`).
