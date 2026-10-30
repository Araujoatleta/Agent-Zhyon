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
4. On iOS 16 or later, the first time: **Settings → Privacy & Security → Developer Mode** → on (the iPhone restarts and
   asks again: confirm).

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
- **Memory**: Hermes remembers what you tell it and past conversations, as on the computer.
- **Helpers**: for a long job Hermes hands parts to helpers of its own (delegation), as on the computer.

## Photos and files

The **+** next to the message field sends Hermes **photos** (the library, or the **camera**) and **files** (Files,
iCloud Drive): PDFs, Word, Excel, CSV, text. A brain that sees (ChatGPT, Gemini, Claude) gets the photo itself;
another gets it described first. A PDF is read on the iPhone, its scanned pages with the iPhone's own text recognition,
and its text goes with your message; Hermes can read any file you sent again later. *"What does this receipt say?"*,
*"sum up this statement"*, *"add these trades to my portfolio"*. Files stay in AXYON's own folder, a month at most.

With ChatGPT as the brain (or OpenRouter), Hermes also **makes images** (*"draw…"*): they show under its answer, and a
tap shares or saves them.

## Google: Gmail, Sheets, Drive, Docs and Calendar

On the computer, Hermes' google-workspace skill signs in by running scripts in a terminal, which no iPhone app may
run: here AXYON signs in to Google itself, in the app, with **your own Google Cloud OAuth client** (the one you made
for gog or for Hermes on the computer works too).

1. Once, in Google Cloud (about 5 minutes): create a project, enable the Gmail, Sheets, Drive, Docs and Calendar APIs,
   create an OAuth client of type **Desktop app**, add your account as a test user (Audience) and download its JSON.
2. **Settings → Google → Choose the client's JSON file** (or paste its client ID and secret), or send the JSON in the
   chat and ask *"connect my Google"*.
3. **Sign in with Google**: Google's own page opens inside AXYON; sign in and allow (if Google says the app is not
   verified: Advanced → go to your app). AXYON finishes by itself. If the page after allowing fails to load, copy its
   address and paste it in Settings → Google, or in the chat.

Then: *"what is new in my Gmail?"*, *"find the BTLG11 report in my e-mail"*, *"read my sheet's AXYON tab"*, *"add this
trade to my spreadsheet"*, *"find my 2027 plan in Drive"*, *"put the assembly in my Google Calendar"*. A draft goes to
Gmail's drafts; **an e-mail is sent only when you tap Send** on the card AXYON shows in the chat. Your password never
goes to AXYON: the sign-in is Google's page, and AXYON keeps Google's sign-in in Hermes' folder, as Hermes does on the
computer.

## Scheduled tasks

Hermes' own **cron** works on the iPhone: *"every weekday at 9am, tell me how BTLG11 and LVBI11 are against my
ceilings"*, *"in 2 hours, remind me to pay the DARF with the amount"*. Each task's answer comes as a **notification**;
a task told to stay silent when there is nothing to say ("only tell me if…") stays silent. **Settings → Scheduled
tasks** lists them; to change or remove one, ask in the chat.

On time while AXYON is open. With AXYON closed, iOS wakes it **a few times a day** (Background App Refresh on), and a
task that came due runs then: no iPhone app may wake itself every 5 minutes. For alerts within minutes during the
session, the computer's AXYON watches the prices while it runs and can notify the iPhone (Settings → Phone →
Notifications with Coucou closed, on the computer).

## The superpowers, inside the iPhone

The computer's own superpowers engine runs inside the app, with no computer:

- **The portfolio**: *"how is my portfolio today?"*, *"what is GARE11 at?"*, from the official data of B3, the CVM,
  FundosNet and the Banco Central (read when you ask) and the session's prices from Google Finance (about 15 minutes
  late).
- **What you did**: *"today I bought 10 GGRC11 at 9.20"*, a pasted portfolio (Investidor10 or the broker), *"my
  ceiling for TIMS3 is 21.50"*. Kept on this iPhone, shown in the Wallet tab.
- **Funds and companies**: book value, P/VP, dividends and whether they last, record dates and payments,
  assemblies; a company's statements and your thesis checked point by point.
- **Ceiling prices** (Bazin, projected Bazin, Graham, Peter Lynch) and the opportunities under them.
- **Plan, taxes and decisions**: the road to your targets by class, the month's DARF and the yearly declaration, the
  decision journal and the *committee* before you decide.
- **Studies, English and training**: the exam plan, review cards, the English lesson from your own companies, today's
  workout, and *"what is my day today?"*.

None of it buys, sells, pays or sends: AXYON only records and works things out. The month's content kit stays on the
computer.

**Alerts with AXYON closed** (Settings → Superpowers, off until you turn them on): a notification when an asset goes
under your ceiling or back above it, on a strong day for an asset or the portfolio, and when an asset is under its
ceiling, each once; and in the morning, once, **your day** (record dates and payments, a DARF, decisions to look at
again, studies, training and the English lesson). iOS decides when AXYON may look (a few times a day, as you use the iPhone, and only with
**Settings → General → Background App Refresh** on for AXYON); **Look now** looks at once. Only B3, the CVM, FundosNet, the Banco Central and Google Finance are read; nothing about you is sent.

iOS asks the first time for each thing it protects; **Settings → What AXYON may use on this iPhone** shows what is
allowed.

## Investing: the committee, the reader, your theses, the week and learning

The **Wallet** tab, the first one AXYON opens on, is its investing screen, in the design of a statistics screen:
charcoal, a ring of thick rounded segments in soft colours with the total in its middle, a legend in two columns, a
pill to choose **Portfolio** or **Dividends**, **Class | Asset**, a carousel of months for the dividends, and
**History** under it (your positions, as before). Every number is computed by AXYON's code from the official sources;
the brain only reads and votes, and says where each point comes from. The reports are always in Brazilian
Portuguese, and the app itself speaks Portuguese (set once; iOS **Settings → AXYON → Language** changes it).

- **The dividends are your sheet's**: when your Google Sheet has a **Proventos pagos** table (months across, the total
  per month, assets row by row), the computer reads it every 3 hours and sends it with the portfolio's copy; the ring
  shows those amounts (R$ 82,27 in October if that is what the sheet says), says **Your sheet · read on …**, and a month
  the funds' announcements disagree on shows both. Without the table, the announcements times what you held.
- **Targets per asset**: your sheet's **Percentual Ideal** (or **Meta**) column, or **… → Targets per asset** on the
  Wallet. The committee weighs every asset against its target before any opinion on buying more.

- **The committee of five minds, audited**: ask *"buy 100 shares of GGRC11?"*. The code first gathers the **facts**,
  each numbered (F1…) with its source, its reference date, the day it was read, the calculation (P/VP names the price's
  day and the book value's month; R$ 0,083 stays R$ 0,083 into the yield; the 12-month yield and the one annualized
  from the last payout are two facts) and a status: confirmed, computed, estimate, missing or divergent (two sources
  that disagree). A valuation analyst, a legal and governance mind, a forensic accountant, a risk manager and a macro
  strategist each vote with how sure they are, why, and points marked fact, calculation, hypothesis, estimate or
  missing data, each citing F3 or a document's page (D1 p.4), and what they lacked. AXYON's code then checks every
  figure against what it cites, turns a vote against that rests on missing data into neutral, keeps a veto only on an
  objective criterion (concentration only past twice your target or a quarter of the portfolio), and an **auditor**
  (the brain again) reads all of it, revises the minds it must and writes the two final opinions: **A, the asset**
  (positive, neutral, negative) and **B, in your portfolio** (increase, keep, reduce, no new contribution) — a good
  asset above its target can be A positive and B no new contribution, and a vote against buying is never a call to
  sell. The verdict shows **Critical alerts** apart from **Material points** and **Notes**, a **To verify** list, each
  mind's points with a seal when the code found them in their source, and the **Sources**: tap F3 for its source,
  dates and calculation, or D1 p.4 for the page's text and the link to the document. It ends with a **question**:
  the decision is yours. The minds are archetypes, never real people.
- **The intern who reads everything**: the monthly reports, material facts, assemblies and offerings of your funds
  (FundosNet) and the material facts, notices and releases of your companies (the CVM's list, once a week) are
  downloaded and read on the iPhone (PDFKit, scanned pages by the iPhone's own text recognition); the code finds the
  figures with their pages (vacancy, default, dividend and result per share, LTV, cap rate, holders) and what changed
  since the previous report; AXYON Core sums up only what matters, page by page.
- **Living theses**: write why you hold an asset in your words (*"P/VP below 1.05; yield above 9%; vacancy up to
  10%"*); each condition with a number is checked against the price and every new report. When a document that
  matters comes or a thesis breaks, the committee **meets by itself** when AXYON Core is running, and you get a
  notification (with **Settings → Superpowers** alerts on).
- **Your week in three minutes**: a script AXYON's code makes from your records (the portfolio against a week ago,
  the best and worst moves, the dividends that came and those ahead, what the reader read, the theses, the
  committee), read aloud in the iPhone's voice; **Make the audio** saves it as a sound file to send (WhatsApp takes
  it as audio), **WhatsApp** opens it with the text written, and a reminder every Sunday if you want one.
- **Learn with your own portfolio**: apprentice mode (the committee, the reader and the week explain each term the
  first time), a glossary with each term's value in your own assets, flashcards of your assets graded as you go, a
  training portfolio with pretend money at the real prices (against your real one and the CDI), and your personal
  score (when you say you are sure, how often you are right, from the decision journal).

Your portfolio stays on the iPhone. The documents are public and read once on the iPhone; the brain you chose is
asked only for a document's digest and the committee's meetings (two answers each: the minds, then the auditor).
Nothing is bought or sold: this is not an investment recommendation.

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
there, so Hermes on the iPhone still answers about it with the computer off (each newer copy replaces the iPhone's
positions: with a computer, it and its sheet stay the record); the computer's home shows in the Home tab
and Hermes can act on it; a command of the computer's agents waiting for your OK shows above every tab with Allow
(Face ID) and Deny, and the sessions are listed in **Settings → Agents on the computer**. Away from home, use
Tailscale on both ([IPHONE_PC.md](IPHONE_PC.md)).

## What iOS does not allow

iOS suspends an app soon after it leaves the screen: Hermes thinks while AXYON is open, and an answer under way gets
the half minute iOS grants to end, then a notification brings it. iOS lets no app run other programs: Hermes'
terminal, code execution, its own browser and file tools that go through a shell are not on the iPhone.

**Computer use** (the mouse and keyboard Hermes drives through cua-driver on a Mac or a PC) cannot exist on an
iPhone: iOS lets no app see or touch another app's screen, sideloaded or not. What comes closest: **Shortcuts**
(*"run my shortcut…"*: Shortcuts reach almost every setting and many apps), links that open an app filled in, and the
paired computer's AXYON, which does use its own screen.

## When the app closes by itself or Hermes does not start: REA

[REA](https://github.com/morluto/rea) reads the IPA and the iPhone's crash reports without running anything. On a
computer with [Node.js](https://nodejs.org) 22 or 24:

1. **What the IPA carries** (Info.plist, frameworks, Python, permissions):
   `npx -y rea-agents@latest inspect-artifact AXYON-iPhone-Hermes.ipa`
2. **An iPhone crash report**: on the iPhone, **Settings → Privacy & Security → Analytics & Improvements → Analytics
   Data**, find a file starting with **Coucou** (AXYON's program name), tap it and share it (AirDrop, or an e-mail to
   yourself). On the computer: `npx -y rea-agents@latest inspect-apple-ips Coucou-2026-10-28-123456.ips --format md`
   shows the kind of crash, the thread that stopped and the functions it was in.
3. **An unreadable Swift name** (`$s5Coucou…`): `npx -y rea-agents@latest demangle-swift '$s5Coucou…'`.
4. **The libraries the app loads**: unzip the IPA (it is a .zip) and run
   `npx -y rea-agents@latest trace-dylib-resolution Payload/Coucou.app --format md`.

CI does step 1 on every build (the **REA-evidence** artifact has all of it). What Hermes says as it starts is also in a
sysdiagnose and, with the iPhone on a cable, in the Mac's Console app, filtered on **Coucou**.
