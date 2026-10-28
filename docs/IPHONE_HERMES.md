# AXYON on the iPhone with Hermes inside

The sideloaded iPhone app (Sideloadly or AltStore) can carry **Hermes Agent inside it**. Hermes is the brain and the
iPhone itself is the body: AXYON thinks and acts **with no computer at all**. A Windows or Linux computer stays an
option: paired, it adds its home, its agents (Claude Code, Codex…) and the portfolio. Portuguese:
[IPHONE_HERMES.pt-BR.md](IPHONE_HERMES.pt-BR.md). How it is built: [NotchBuddy/Hermes/README.md](../NotchBuddy/Hermes/README.md).

## Install

1. On GitHub, **Actions** → **iPhone (sideload)** → the latest green run → download the **AXYON-iPhone-Hermes**
   artifact (a .zip with `AXYON-iPhone-Hermes.ipa` in it).
2. On a computer, open **Sideloadly** (or AltStore), drop the .ipa, sign in with your Apple Account and install. The
   icon reads **AXYON**.
3. On the iPhone: **Settings → General → VPN & Device Management** → trust your Apple Account. With a free account the
   app must be installed again every 7 days (Sideloadly does it again with the same file).

## First use

1. Open AXYON. The orb wakes up: Hermes takes a few seconds to start inside the iPhone the first time.
2. Tap **Give Hermes a brain** (or **Settings → Brain**) and choose:
   - **ChatGPT** (your subscription, no key): **Sign in with ChatGPT**, copy the code, **Open OpenAI**, paste the code
     and confirm. That sign-in belongs to this iPhone, apart from the computer's.
   - **Google Gemini** (free key): **Get a free key from Google AI Studio**, paste it, **Use this brain**.
   - **OpenRouter**, **Anthropic** or **another OpenAI-compatible** server: paste the key (and the address).
   Keys stay in this iPhone's Keychain, out of backups.
3. Write or speak in **Chat**.

## What it does with the iPhone

- **Reminders and notifications**, which come even with the app closed; **the calendar**; **contacts**.
- **Messages, e-mails and calls**: it opens WhatsApp, Messages, Mail or Phone with everything filled in, and **you**
  send. It never sends or calls by itself.
- **Shortcuts**: *"run my Good night shortcut"*. Shortcuts can do almost anything on an iPhone; make one once and ask
  AXYON for it by name.
- **Apps and links**, **the iPhone's state** (battery, storage, where it is), the **flashlight**, **music**, reading
  aloud, the clipboard.
- **The web**: a search and reading a page, no key, no account.
- **The home**: the TVs, boxes and Home Assistant the iPhone reaches itself (below).
- **The portfolio**: B3 prices from Google Finance (about 15 minutes late), with the positions a paired computer gave.
- **Memory**: Hermes remembers what you tell it and past conversations, as on the computer.

iOS asks the first time for each thing it protects; **Settings → What AXYON may use on this iPhone** shows what is
allowed.

## The home straight from the iPhone (no computer)

**Home → +** adds the devices the iPhone drives itself on the home's Wi-Fi: a **Samsung TV** (Tizen, 2016 on; it asks
once on its screen to allow AXYON), an **Android TV or box** (Android 4.4 too: turn on USB debugging and network
debugging on port 5555 in its Developer options; it asks once "Allow USB debugging?"; the key stays in the
iPhone's Keychain), and **Home Assistant** (its address and a long-lived access token; lights, switches, players,
covers, scenes and scripts, never locks or alarms). Then ask in the chat: *"turn on the living room TV"*. Plugs and
switches ask for Face ID first. The first scan makes iOS ask for the local network: allow it.

## With a computer (optional)

**Settings → Computer (optional) → Windows or Linux computer** pairs as before. Once paired, the computer's portfolio
(positions, your ceilings, last prices, the notes of AXYON's tab) reaches the iPhone each time it connects and stays
there, so Hermes on the iPhone still answers about it with the computer off; the computer's home shows in the Home tab
and Hermes can act on it; the computer's agents show on the start screen with Allow and Deny. Away from home, use
Tailscale on both ([IPHONE_PC.md](IPHONE_PC.md)).

## What iOS does not allow

iOS suspends an app soon after it leaves the screen: Hermes thinks while AXYON is open, and an answer under way gets
the half minute iOS grants to end, then a notification brings it. iOS lets no app run other programs: Hermes'
terminal and code tools are not on the iPhone.
