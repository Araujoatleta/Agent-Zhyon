# Coucou on iPhone with Windows or Linux

The iPhone app follows a Mac through iCloud ([IPHONE.md](IPHONE.md)). It can also follow **Coucou on a Windows or
Linux computer**, directly over your local network, with the link Coucou for Android uses
([ANDROID_LINK.md](ANDROID_LINK.md), protocol v1). No Mac, no iCloud, no server in between.

## What you get

- Your agent sessions as the island shows them: Claude Code and the other agents, live.
- **Allow** (behind Face ID) and **Deny** for permission requests, and answers to Claude Code's questions.
- **Mochi's chat on the computer**, with the AI providers set up there (the keys never leave the computer), and
  **Mochi's hands**: ask him to look through a folder, change a file or run the tests. Each step shows on the iPhone
  as he works, and every change, command or web request still waits for your Allow.
- **Talk to Mochi**: the microphone in the chat with the computer; a pause sends what you said, and the answer is read
  aloud. With **Mochi's home** on the computer, that is "turn on the living room TV and open YouTube"
  ([SMART_HOME.md](SMART_HOME.md)).
- A notification for a request that arrives in the half minute after you leave Coucou, and after that **through
  ntfy**, if you turn it on (see *Notifications with Coucou closed* below).
- **The assistant and Mochi's home**: change the assistant's name, tone, brain and skills, sign in with ChatGPT, and
  use the devices of Mochi's home, if you allow it on the computer (see below).

## On the computer

Coucou for Windows or Linux 0.4.0 or later (Mochi's hands, his home and notifications through ntfy need the next
version, unreleased yet: download the installer from the **Windows installer** workflow's artifacts, or build it from
[`windows/`](../windows/README.md#build-it-yourself) meanwhile):

1. **Settings → Phone (Android and iPhone)**: turn on **Phone link**. The iPhone must be on the same Wi-Fi. If Windows
   asks about the firewall, allow Coucou on private networks.
2. Optional, each off until you turn it on: **Show session details on the phone**, **Let the phone answer Claude
   Code's questions**, **Let the phone chat with my AI providers** (then tick the models the phone may use),
   **Let the phone use Mochi's hands** (it needs the chat, and **Settings → Mochi's hands** turned on with a working
   folder), **Let the phone change the assistant (name, tone, brain, skills)** and **Let the phone use Mochi's home**
   (see *The assistant and Mochi's home from the iPhone* below).
3. **Show pairing code**.

## On the iPhone

1. In **Coucou → ⚙︎ → Windows or Linux computer**, tap **Scan the code** (**Scan the pairing code** on first launch)
   and point at the computer's code. The iPhone's own Camera app cannot open Coucou codes ("No usable data found").
   Or press **Copy link** on the computer, send it to the iPhone, and tap it or paste it in the same screen.
2. Coucou names the computer it found: tap **Pair**. When iOS asks, allow Coucou on the **local network**.
3. The computer's sessions show up in **Agents**. The 🖥 button at the top left opens the chat with Mochi on the
   computer; the ⋯ menu there picks the model and turns his hands on or off for the chat.

## Notifications with Coucou closed (ntfy)

iOS pauses Coucou about half a minute after you leave it, and only Apple's push could wake it, which the sideload
build (a free account) cannot have. So the computer can announce a waiting request through **ntfy**, a free
notification app:

1. On the iPhone, install the **ntfy** app (App Store) and allow its notifications.
2. On the computer: **Settings → Phone (Android and iPhone) → Notifications with Coucou closed** → on. A **Topic**
   shows up (a random name, hard to guess): **Copy**.
3. In ntfy on the iPhone: tap **+**, paste the topic, keep the default server (ntfy.sh) and subscribe.
4. On the computer: **Send a test notification**. It should show on the iPhone within seconds.

A request (a permission or a question) that still waits **8 seconds** after it came, while **no phone is
connected**, is announced: "Claude Code is waiting for your OK". A tap opens Coucou, which connects and shows the
request. At most one notification every 20 seconds; a request answered at the desk first is not announced.

The notification goes through the ntfy server and only says **who** is waiting: never the command, a path, a
question's text or the computer's name. Whoever knows the topic can read these notifications: **New topic** changes it
(subscribe again in the app). Your own ntfy server goes in **Server** (https, or plain http on the local network
only), with an **Access token (optional)** kept in the system's keystore. Off by default.

## Talking to Mochi

In the chat with the computer (🖥), tap the **microphone** next to the text field and speak: "turn on the living room
TV and open YouTube". After a short pause the message is sent, and the answer is **read aloud** with the iPhone's
voices. The chat's ⋯ menu has **Read answers to spoken messages aloud** and **Dictation language**. Recognition runs
on the iPhone when it can. On the computer, press **Windows + H** in the island's chat to dictate
([SMART_HOME.md](SMART_HOME.md#voice)).

## The assistant and Mochi's home from the iPhone

When Coucou runs Hermes Agent on the computer ([HERMES.md](HERMES.md)), the iPhone can change the assistant the way
**Settings → Hermes Agent** does, and use the devices of Mochi's home ([SMART_HOME.md](SMART_HOME.md)). Each needs its
own switch in **Settings → Phone (Android and iPhone)** on the computer, off until you turn it on:

- **Let the phone change the assistant (name, tone, brain, skills)**. On the iPhone, **Settings → Assistant** shows the
  assistant's name, its tone (or your own words for it), its brain, its skill packs and whether Hermes runs. A new
  brain asks for Face ID first, and so does **Sign in with ChatGPT**: the code shows on the iPhone in large digits, and
  **Open sign-in page** opens OpenAI's page in Safari (the computer's browser stays closed). Hermes is never installed
  from the iPhone: set it up on the computer first. The keys stay on the computer.
- **Let the phone use Mochi's home**, with **Settings → Mochi's home** turned on too. The **Home** tab shows the devices
  you ticked for Mochi, room by room, with what each reported it can do (power, volume, brightness, mute, apps,
  channels), the routines and the latest actions. A device or a routine that asks first asks for Face ID. The TVs' and
  Home Assistant's addresses, MAC addresses and tokens never reach the iPhone, and each action is written in the
  home's record on the computer, as coming from the phone.

Turning one of these switches on or off reconnects the iPhone once, so it learns about it; while one is off, the
iPhone says which switch to turn on.

## Installing the iPhone app without a Mac

TestFlight and the App Store come from the author's account. Without a Mac and without a paid developer account, you
can install the **sideload build**, which GitHub Actions makes for you:

1. On GitHub: **Actions → iPhone (sideload)**. In a fork, enable Actions once, then **Run workflow**. When the run is
   green, download the artifact **Coucou-iPhone-sideload** (a zip with `Coucou-iPhone-sideload.ipa` inside).
2. Install the `.ipa` with [Sideloadly](https://sideloadly.io) (Windows or macOS) or [AltStore](https://altstore.io)
   (AltServer on Windows), signed in with your Apple Account. A free account works.
3. On the iPhone: **Settings → General → VPN & Device Management** → trust your Apple Account's developer app. On
   iOS 16 or later also **Settings → Privacy & Security → Developer Mode** → on (the iPhone restarts).
4. Apps signed with a free account stop opening after 7 days: install again with Sideloadly, or let AltStore refresh
   it (AltServer running on the computer, same Wi-Fi).

The sideload build is compiled with `SIDELOAD`: it has **no iCloud and no Apple push**, which a free account cannot
have, so it talks only to a Windows or Linux computer. Its widgets stay empty (they need an App Group), and the iPhone
starts Mochi's Live Activity itself (next section). Everything else on this page works.

## Mochi in the Dynamic Island

The Dynamic Island and the Lock Screen show Mochi with the computer's agents (Claude Code, Codex, Gemini CLI…): the one
that needs you most, its step, how long it has been at it, and **Allow** and **Deny** when it waits for your OK (touch
and hold the island; Allow asks for Face ID). The answer goes to the computer through the link, as in the app.

1. Install the sideload build of this branch (an older build has no Dynamic Island for a computer) and pair it
   (above).
2. Allow Live Activities for Coucou: **Settings → Coucou → Live Activities** → on.
3. Open Coucou while an agent on the computer is working or waiting for you: Mochi comes into the island, and stays
   there when you leave the app. Nothing working, nothing in the island.

What to expect, since the sideload build has no Apple push:

- Only Coucou on screen can start the activity: an agent that starts while Coucou is closed shows up the next time you
  open it.
- iOS suspends Coucou about half a minute after you leave it. The island keeps the last state, then, about 40 seconds
  after you left, says **Open Coucou to see the latest** and hides Allow and Deny. Opening Coucou brings it up to date
  at once.
- While Coucou is closed, a request reaches you through **ntfy** (above).
- When nothing is working or waiting any more, Mochi shows the work done and leaves the island half a minute later.

## Away from home, directly (no one's server)

The iPhone can reach the computer from anywhere (cellular, the Wi-Fi at work) with **no server in between**: no
Cloudflare, no VPS, no third-party service. The door is opened by **your own router**:

1. On the computer: **Settings → Phone (Android and iPhone) → Away from home, directly** → on.
2. Coucou asks the router (with **UPnP**, the way game consoles and video calls do it) to forward the link's port to
   this computer, and asks it the home's internet address. A few seconds later Settings shows what the router said,
   for instance **Over IPv4: 198.51.100.7:47821** and **Over IPv6: [2804:…]:47821**.
3. Open Coucou on the iPhone **once at home**, on the Wi-Fi: it learns these addresses. In **⚙︎ → Windows or Linux
   computer**, the **Away from home** row then says *Ready*.
4. That is all. Away from home, the iPhone tries the local network and, if it has not answered within a second and a
   half, goes straight to the home's addresses; on cellular it goes there at once. Linked that way, it says *Over the
   internet, directly to your computer*.

To try it: turn the iPhone's Wi-Fi off and open Coucou. On the computer, **Last connection from outside** shows when.

What has to work (Settings says which one did not, in words):

- **UPnP on in the router.** *"No router answered on the local network"* means it is off: it is usually under
  "Network", "NAT" or "Advanced" in the router's settings.
- **A real internet address.** Many providers share one IPv4 address between many homes (**CGNAT**): then nothing can
  come in over IPv4, and Settings says so. **IPv6** usually works anyway, since every device has its own address; if
  the router keeps IPv6 closed to the outside, look for its option to allow incoming IPv6 connections. A router behind
  the provider's box (two routers) needs that box in **bridge mode**, or a public IP from the provider.
- **The home's address may change.** Coucou asks the router again every 10 minutes and tells a connected iPhone at
  once. If it changes while the iPhone is away and not connected, it learns the new one the next time it connects at
  home.
- **The computer on, with Coucou running**, as always.

Security: it is **the same link** as at home, TLS with the certificate pinned by the QR code and the same pairing
code. A caller from the internet without the code gets no further than the first line; such callers share **2
places** of their own, so they never take the iPhone's place at home, and with the switch off the computer does not
answer the internet at all. Only the router is asked: no "what is my IP" service, no one's DNS. The home's address
never goes to the log. It is **off by default**; turning it off closes the port on the router at once, and so does
quitting Coucou. If the computer goes down suddenly, the router closes the port by itself within the hour (some
routers only take permanent forwards: then it stays open, to a computer that does not answer, until you turn the
switch off).

## Limits, honestly

- **The local network, or away from home directly** (above). Same Wi-Fi, or the computer's own hotspot. Guest
  networks and hotspots with client isolation, a VPN on either side, or a firewall that blocks the port keep the iPhone
  from the computer. If the
  computer's address changes, the iPhone finds it again through its announcement (`_coucou._tcp`) and checks the
  certificate before trusting it.
- **No wake-up.** iOS pauses an app shortly after it leaves the screen, and only Apple's push can wake it, which needs
  a server and a paid developer account. So requests reach the iPhone while Coucou is open, and in the half minute
  after you leave it (as notifications you can answer from the Lock Screen); after that, only with **notifications
  through ntfy** turned on (above), which announce them and open Coucou with a tap. Open Coucou and it connects again
  at once. The computer's own card always works, and never waits on the phone.
- **One code, one phone.** **Pair again** on the computer makes a new code and disconnects the phone that had the old
  one.

## Security

- The iPhone accepts exactly one certificate, the one whose SHA-256 was in the QR code, over TLS 1.2 or later. The
  announcement on the network is only a hint to find the computer; the certificate decides.
- The pairing token is kept in the iPhone's Keychain, on this device only (not in backups, not in iCloud Keychain).
- **Allow asks for Face ID** (or the passcode); so does sending an answer. A command longer than what reached the
  iPhone (over 500 characters) can only be denied from it. Notification buttons ask iOS to unlock first.
- What crosses the link: session states and steps (steps, the final line and the folder's name only with details
  on), the tool and command of a request, questions and your picks, your chat messages and Mochi's answers. Never
  the computer's API keys, never a file. In Mochi's steps and requests a path is written from the working folder, or
  from `~` elsewhere in your home folder, so none names you.
- Mochi's hands from the phone follow the computer's rules (Settings → Mochi's hands): looking inside the working
  folder is free; every change, command and web request waits for Allow; files that look like keys or passwords
  always ask.

## For developers

- `NotchBuddy/Sources/Phone/PCLinkWire.swift`: the pairing link, the lines both ways and the screens' state, and
  `PCRemote`, which takes only IP addresses with a port from the `remote` line (never a name: nothing is looked up).
  Foundation only, tested by `tests/PCLinkWireTests.swift` (`scripts/test-pc-link.sh` runs on macOS and Linux; the
  Mac CI runs it).
- `PCLink.swift`: Network.framework (TLS with the pinned certificate, Bonjour), the Keychain, notifications, and the
  computer's sessions as `SessionItem`s merged into `PhoneLink.sessions`. Decisions, answers and the chat are routed
  there by fingerprint.
- `PCLinkViews.swift`: Settings → Windows or Linux computer, the pairing sheet, the chat with Mochi, and the
  first-launch guide of the sideload build.
- The sideload variant: `xcodebuild -scheme CoucouPhone -configuration Release -destination 'generic/platform=iOS'
  CODE_SIGNING_ALLOWED=NO 'SWIFT_ACTIVE_COMPILATION_CONDITIONS=$(inherited) SIDELOAD'`, as
  `.github/workflows/iphone-sideload.yml` does. With a paid account you can sign the regular build instead: it keeps
  iCloud for a Mac and gains the computer link too.
