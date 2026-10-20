# Mochi's home (Windows and Linux)

*Português: [SMART_HOME.pt-BR.md](SMART_HOME.pt-BR.md)*

"Mochi, turn on the living room TV and open YouTube." With **Mochi's home**, the chat of Coucou for Windows and Linux
(and the chat of a paired phone) can use the devices of your house: turn a TV on or off, set its volume, open one of
its apps, change the channel, dim a light, switch a plug, run a routine such as "Good night". After each action Mochi
asks the device again and tells you what it reports ("the TV is on, YouTube is on screen"), not what he hoped.

Three ways in, all on your local network only:

- **Home Assistant**, through its official REST API, with a long-lived access token: media players (TVs, speakers),
  lights and switches (plugs, relays).
- **Samsung TVs** (2016 and later, Tizen), straight to the TV: the remote control API Samsung's phone apps use, on
  ports 8001 and 8002. The TV asks once on its screen whether to allow Coucou.
- **Philips TVs**, straight to the TV over JointSpace, the API Philips TVs answer on ports 1925 and 1926. Android TVs
  are paired once with a PIN the TV shows.
- **Android TVs, projectors and TV boxes**, straight to the device over ADB, Android's network debugging (port 5555),
  once it is turned on in the device's Developer options. The device asks once on its screen whether to allow this
  computer.

A TV of another kind is not added: Coucou says what it does answer on, so the next step can be chosen from facts.

It is **off by default**. While it is off, nothing is read and nothing is sent.

## What you need

- Coucou for Windows or Linux with **Mochi's hands** (the version after 0.4.0, unreleased yet: take the installer
  from the **Windows installer** workflow's artifacts, or build it from [`windows/`](../windows/README.md)).
- A chat model that can use tools: Anthropic, Google, OpenAI, OpenRouter, or a local model that supports tool calls.
- The computer on the same network as Home Assistant and the TVs.

## Setting it up

1. **Settings → Mochi's hands**: turn them on and pick a working folder. The house is reached through his hands.
2. **Settings → Mochi's home**: turn on **Mochi's home**.
3. **Search the network** looks for a few seconds (mDNS and UPnP). Home Assistant and the TVs it finds come with
   **Use** or **Add**: a TV of another maker (an Android projector's screen-mirroring app, for one) is tried over ADB
   when you add it, and otherwise says what it answers on. Nothing is added until you click.
4. **Home Assistant**: its address (for example `http://homeassistant.local:8123`) and a **long-lived access token**
   (in Home Assistant: your profile → Security → Long-lived access tokens), then **Connect**. Coucou says which
   version answered. Rooms come from Home Assistant's areas when the token belongs to an administrator; otherwise
   set them in Coucou.
5. **TVs**: the TV's address (in the TV's network settings), then **Add**, or **Add** next to a TV the search found.
   Coucou first asks it whether it is a Samsung TV, then a Philips TV:
   - **Samsung**: its API names it ("UN43AU7700GXZD · Tizen · API 2.0.25 · encrypted (port 8002)") and gives its
     Wi-Fi MAC address; its UPnP renderer, when it has one, gives the volume as a number and mute ("volume by UPnP").
     Press **Pair**: the TV asks on its screen whether to allow Coucou, choose **Allow** with the remote (you have
     half a minute). The TV then gives Coucou a token, kept in the keystore, and does not ask again.
   - **Philips**: Coucou asks the TV which JointSpace version it speaks (6, 5 or 1), over HTTPS on port 1926 or HTTP
     on 1925, and what it can do, and shows it:
     "55PUS8007/12 · JointSpace 6.4.0 · Android TV · encrypted (port 1926)". An Android TV needs **Pair**: it shows a
     PIN, type it in **PIN on the TV**, **Confirm**.
   - **Android over ADB** (when it is neither): see *Android TVs, projectors and TV boxes* below.
   - **None of them**: the device is not added, and the answer lists what it does answer on ("DIAL, UPnP
     MediaRenderer…"), with how to turn on an Android device's network debugging. Send that line to whoever helps you
     with Coucou: it says which way, if any, could control it.

   The **MAC address** (for switching the TV on from deep standby with Wake-on-LAN) comes from the TV or from the
   computer's neighbour table; you can also type it.
6. **Devices → Read the devices**: every device with what it reported it can do and its state. Tick **Mochi may use
   it** for those he may use (devices from Home Assistant start unticked; a TV you added starts ticked), give it a
   name and a room ("TV", "Sala"), and tick **Ask before acting** if it should always wait for your click. **Test**
   runs any action it reported, with a value when it takes one (a percent, an app, a channel).
7. **Routines**: **Suggestions** offers "Good night" (TVs, speakers and lights off), "Movie mode" (the living room TV
   on, its lights dimmed) and "Turn on …" for each TV with a room, made of your own devices. **Edit and save** to keep
   one; **New routine** to write your own (up to 20 steps, each with an optional wait of up to 120 s, for a TV that
   needs a moment before it opens an app).
8. **Voice** (optional): **Read Mochi's answers aloud**, and **Try** to hear the voice. See *Voice* below.

### Android TVs, projectors and TV boxes (ADB)

Many Android devices without Google's TV certification (projectors such as the HY300, TV boxes) can only be reached
through ADB, Android's debugging bridge. Coucou speaks it itself, on port 5555 (nothing to install):

1. On the device, open **Settings → About** (on some projectors, "About" or "Sobre nós") and tap **Build number**
   seven times: **Developer options** appear. Turn on **USB debugging** there, and **network debugging** (or "ADB over
   network") when it is listed.
2. In Coucou, type the device's address (in its network settings) under **TVs and projectors**, then **Add**. Coucou
   asks Samsung's and Philips' ways first, then knocks on port 5555 without offering anything.
3. Press **Pair**. The device shows **Allow USB debugging?** with this computer's key fingerprint: tick **Always allow
   from this computer** and choose **Allow** (you have a minute). Coucou then reads what the device is (its model,
   Android version and name), what it reports (volume, mute) and the apps its launcher shows.

The key is an RSA-2048 one, as adb makes it, created by Coucou the first time and kept in the system's keystore
(never in a file). ADB gives full control of a device, so Coucou only ever sends its own fixed commands: keys (wake,
sleep, volume, mute), opening an app of the device's own list, and reading its state (`dumpsys`). Nothing the model
writes ever becomes a command. Android 11's **wireless debugging** (with a pairing code) is a different protocol, with
TLS, which Coucou does not speak: it says so, and the classic network debugging is the way in.

What then works: turning it on from sleep and off (to sleep), the volume up and down and as a number, mute, opening an
app, and reading whether it is awake, its volume and the app on screen. A device that turns its network off when it
is switched off (most projectors and TV boxes) cannot be switched on from Coucou: turn it on with its remote.

## Talking to Mochi

Ask in the chat as you would ask someone at home, in your language:

- "Mochi, liga a TV da sala e abre o YouTube" — the TV is switched on, Mochi waits until it reports being on, then
  opens YouTube from the TV's own app list, and tells you what is on screen.
- "Abaixa o volume da TV do quarto para 15", "Põe no canal 5", "Apaga a luz da cozinha".
- "Boa noite" runs the routine of that name.

Devices are found by their name and room ("TV da sala", "living room TV"); apps and channels by the device's own
lists. When something is ambiguous or not possible, Mochi says so instead of guessing.

## Voice

- **On the computer** (Windows 10 and 11): open the chat and press **Windows + H**. Windows' voice typing writes what
  you say in the chat's field, in the language of the keyboard in use (Windows + Space switches it), then press Enter.
  Linux has no voice typing of its own: speak from the phone.
- **Reading answers aloud** (Settings → Mochi's home → Voice, off by default): answers to what you asked in the
  island's chat are read with the computer's own voices, in the app's language. Only voices that run on the computer
  are used, so nothing is sent anywhere. For Brazilian Portuguese, Windows has "Microsoft Maria"; add voices in
  Windows Settings → Time & language → Speech. Muting the island's sounds silences it too.
- **On the iPhone** (sideload build, chat with the computer): the microphone next to the text field. Speak, pause, and
  the message is sent; the answer is read aloud with the iPhone's voices (the chat's ⋯ menu turns that off and picks
  the dictation language). Recognition runs on the iPhone when it can.

## Safety

- **Nothing the model writes is run as is.** Mochi has three tools: list the devices, control one, run a routine.
  Every action is parsed into a fixed set (turn on/off, volume, mute, open app, channel, brightness), checked against
  what that device reported, and becomes one call of a fixed list: in Home Assistant only `media_player`, `light` and
  `switch` services such as `turn_on`, `volume_set` or `select_source`. Scripts, automations, shell commands, locks,
  alarms or restarts cannot be reached.
- **Only the devices you ticked.** Mochi does not even see the others.
- **Clicks for what matters.** Plugs and switches always wait for a click (Allow on the island, or on the phone
  behind Face ID), and so do the devices set to **Ask before acting**; **Ask before every action** makes every device
  wait. With Mochi's hands set to read only, house actions are refused.
- **Credentials in the system's keystore** (Windows Credential Manager, or the Secret Service on Linux): the Home
  Assistant token and the TVs' pairings. Never in a file, never in the logs, never sent to the AI provider.
- **Local network only.** Every address is checked before each request: an IP address must be private, link-local or
  loopback, and a name must be a local one (`.local`, `.lan`, `.home.arpa`, a single word) or resolve only to such
  addresses. A public address is refused, and these requests never go through a proxy.
- **The TV's certificate is pinned.** Samsung and Philips TVs use a self-signed certificate: Coucou keeps its SHA-256
  at the first contact and refuses a different one afterwards (remove the TV and add it again if you know why it
  changed).
- **A record.** Every action, who asked (chat, phone, routine, Settings) and how it went is written in `home-log.jsonl`
  (`%LOCALAPPDATA%\Coucou` on Windows, `~/.local/share/coucou` on Linux), at most 1,000 lines, on this computer only.
  **Show the latest actions** in Settings reads it.
- Home Assistant over plain `http://` sends the token in clear on your network: use `https://` if you can.
- **Other agents** (Hermes Agent, or another MCP client on this computer) reach the house only once you turn on
  **Settings → Hermes Agent → Mochi's home for Hermes**, with the same rules, and the record names them. See
  [HERMES.md](HERMES.md).

## Limits, honestly

- **Tested on simulated devices.** The code is checked against a simulated Home Assistant, a simulated Samsung TV
  (its API, its remote over TLS with the Allow question and the token, its apps, its UPnP volume) and a simulated
  Philips TV (requests, pairing, digest authentication, errors), not yet against real TVs. Neither Samsung's remote
  API nor JointSpace is a published API, and both differ between years and firmwares. Coucou only offers what your TV
  answers; if something fails, Settings and Mochi say what the TV said.
- **Switching a TV on.** A TV in standby may still answer on the network (it is then switched on by its API or its
  power key), or not at all: then only Wake-on-LAN can wake it, which needs its MAC address, the computer on the same
  network segment, and the TV set to allow being switched on over the network (Samsung: "Power On with Mobile", in
  Settings → General → Network → Expert Settings; Philips: "Switch on with Wi-Fi (WoWLAN)" or "Switch on with
  Chromecast"; on the models that have them). A TV on Wi-Fi in deep standby may not wake. Coucou waits up to 25 s and
  says "did not confirm" rather than claiming it worked.
- **Samsung TVs**: the app list comes from the TV, which not every model gives (then no app is offered); whether an
  app came up is confirmed only on the TVs that say which app is on screen. Channels: next and previous only (the TV
  gives no channel list). The volume as a number and mute need its UPnP renderer; without it, volume up and down only.
  A Samsung TV from before 2016 (another protocol) is not supported.
- **Apps** can be opened on Android TVs that list them; **channels** only on TVs that list their channels. The app or
  channel must be in the TV's own list.
- **Home Assistant**: media players, lights and switches only. Rooms from its areas need an administrator's token (the
  template API); otherwise set rooms in Coucou. Channels are not offered through Home Assistant.
- **Other brands** (LG, Sony, TCL…) are found by the search but not controlled directly: add them to Home Assistant,
  whose integrations know many of them, and they show up as its media players. Adding one by its address says what it
  answers on, for a later driver.
- **Android over ADB** is tested against a simulated device (the handshake, the key's signature checked as Android
  checks it, the question on its screen, commands and their output) and the outputs of Android 4.4, 7 to 9 and 13,
  not yet against a real projector. Apps are named from their package (a list of known ones, else the package's own
  word: "com.waxrain.airplaydmr" is "Waxrain"); the volume as a number moves by the device's steps (15 on most).
  Whether a device keeps network debugging on after a restart depends on its firmware. A device that turns its Wi-Fi
  off with it cannot be switched on over the network.
- **Discovery** relies on mDNS and SSDP, which guest networks, VPNs or some routers block: add devices by address.
- **Voice typing on the computer** is Windows' own; Linux has none. Reading aloud needs a voice for the app's
  language installed on the computer.

## For developers

- `windows/src-tauri/src/smart_home/`: `model.rs` (devices, capabilities, actions, the checks), `home_assistant.rs`,
  `samsung.rs` (the Samsung remote: its API, the WebSocket with the Allow question and the token, apps),
  `upnp.rs` (a renderer's volume and mute), `philips.rs` (JointSpace, pairing, Wake-on-LAN, the pinned TLS used by
  both TVs), `digest.rs` (HTTP Digest, MD5 and SHA-256, with RFC test vectors),
  `discovery.rs` (mDNS and SSDP), `android.rs` (ADB: the protocol, the RSA key Android asks for, its public key
  format, the fixed commands and the parsing of what they print), `routines.rs`, `journal.rs`, `config.rs` (settings
  and keystore), `mod.rs` (reading the house, carrying out an action, Settings' commands). The crates are those the
  app already uses (reqwest with rustls, ring, mdns-sd, tokio, tokio-tungstenite), and `num-bigint` for ADB's RSA key,
  already in the lock file through the Linux keyring.
- `windows/src-tauri/src/agent/tools.rs`: the tools `home_devices`, `home_control` and `home_routine`; `policy.rs`
  decides which action waits for a click.
- `windows/src/settings/home.ts` and `windows/src/core/home.ts`: Settings → Mochi's home. `windows/src/core/voice.ts`:
  reading answers aloud.
- Tests: `cargo test --workspace` (simulated Home Assistant, Samsung and Philips TVs, an end-to-end turn "turn the TV
  on, then open YouTube", a denied plug never touched) and `npm test` (`tests/home.test.mjs`, `tests/voice.test.mjs`).
