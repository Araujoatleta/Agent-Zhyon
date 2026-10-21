# Zheus One (Windows and Linux)

*Português: [ZHEUS_ONE.pt-BR.md](ZHEUS_ONE.pt-BR.md)*

"Zheus, I'm on my way home. Get everything ready and tell me how the day went." With **Zheus One**, that sentence is
one mission, not fifteen orders: your assistant looks at the whole house, works out a plan — the air conditioning on
in time, the vacuum done before you arrive, the lights after dark, your day's summary ready — shows it to you with
what would clash, and starts it when you say yes.

Zheus One sits on top of two things you already have in Coucou: **Mochi's home** ([SMART_HOME.md](SMART_HOME.md)), the
devices of your house, and **your assistant** ([HERMES.md](HERMES.md)), Hermes or the chat's provider. It uses only the
devices you share in Mochi's home, through the same checks and the same record. Off by default.

## Turning it on

1. Turn **Mochi's home** on and tick the devices Zheus may use (Settings → Mochi's home). The vacuum, the air
   conditioning and the sensors come from Home Assistant (2026.3 or later for cleaning room by room).
2. Settings → **Zheus One** (in glass, like the island): turn it on, then set its cards. **Your arrival**: how long
   the way home usually takes, the temperature for the air conditioning and how many minutes before you arrive it
   starts, and whether the day's summary is told at the arrival. **Vacuum**: whether it cleans before you arrive, how
   long the whole house takes, the lowest battery, and the rooms it cleans with the minutes each takes (in the order
   they matter). **Lights and TV**: when it is dark, the lights for an arrival after dark, the TV and an app.
3. A **✦** tab appears on the island: Zheus One's view.

## The arrival mission

Say it however you like, in the island's chat, on your phone (the chat with the computer, by voice or text), or let
Hermes start it: "I'm on my way home", "estou indo para casa, chego em 25 minutos", "voy a casa", "je rentre dans
une heure". Coucou reads it itself — not a model — with the minutes when you say them, else your usual way home.

**Foresight**: before anything runs, Zheus shows the plan, step by step, with its times:

- **The air conditioning** starts as many minutes before you arrive as you set, not earlier: no cooling an empty house.
- **The vacuum** cleans so it is done before you arrive. When the way home is shorter than the cleaning, Zheus says so
  and offers ways around it: *clean only the bedroom now (about 15 min) and the living room another time*, *start now
  and send it back to its dock just before you arrive*, or *no cleaning today*. It stays on its dock when someone is
  home, when its battery is low, or when it needs help, and the plan says why.
- **The lights** you chose (or the living room's) turn on at your arrival after dark; **the TV** and its app, if you
  asked for it.
- **The day's summary** is prepared by your assistant a few minutes before you arrive, with the tools and connectors
  it has (calendar, tasks, messages, business), read only.
- **The welcome**: at your arrival Zheus says what was done and how your day went, on the island and aloud when you
  turned Voice on: "Welcome home! Done: … Shall we start with business, or would you rather relax first?"

Nothing runs until you say yes. Answer **"yes"** ("sim", "pode", "ok") in the same chat, on the phone or the computer,
or click **Approve** on the island. "Yes, but no cleaning today", "send it back to the dock", "only the bedroom" pick a
way around the conflict as you approve; "no" or "cancel" drops the plan, or stops it while it runs. Only a short answer
counts, and only while a plan is waiting: words that also mean something else ("para", "deixa", "no") never cancel a
plan. A step on a device set to ask still waits for its own click, on the island or the phone (Face ID).

Your message still goes to your assistant, with the plan added, so the rest of it ("…and tell me how the day went",
"remind me to buy bread") is answered too, and the assistant knows not to act on those devices itself.

## The World State

The ✦ view shows the house as glass tiles: the vacuum (cleaning, on its dock, its battery), the air conditioning (its
temperature, the room's, − and +), the lights (how many are on) and the TV, each with its switch (the vacuum's sends it
cleaning, or back to its dock); a kind you have not shared yet keeps its place as an empty tile, which opens Settings;
where you are (on your way, home, away); the facts that tie things together ("the TV is on where the vacuum is
cleaning", "nobody is home, and these are on"); and the missions: **Arrival 15 · 30 · 45 · 60 min** (prepare my
arrival in that many minutes) and **Situation** for the TV. Who is home comes from Home Assistant's people (its
companion app on your phone, your router), or from what you said ("I'm home").

## The Autopilot

Standing goals instead of repeated orders, in Settings → Zheus One → Autopilot. Each goal is **Off**, **Suggest** or
**Act** (within the limits under it); Zheus looks at the house every five minutes while one is on, and nothing runs
while they are all off.

- **Less energy wasted**: what was left on (lights, TV, air conditioning — never a plug) while nobody has been home for
  the minutes you set.
- **A tidy house**: the vacuum cleans the empty house once a day, in its hours, and goes back to its dock when someone
  comes home.
- **Your day prepared**: in the morning, the summary of your calendar, tasks and messages, by your assistant, read only.
- **Your business moving on**: your assistant looks at your campaigns and sales (with the connectors and skills you
  gave it: Instagram, Meta Ads, your shop) and prepares proposals. **Suggestions only, ever.**

What may never be done alone, whatever a goal says: spending, publishing, writing to people (customers included), and
anything about the house's safety; and an action on a device set to ask always waits for your click. A suggestion
shows on the island with **Yes** and **No**, once a day each.

## The TV mode

"Zheus, show our situation" (or **Situation** on the island, or **Open on the TV** in Settings): the house, the plan
under way, the decisions waiting for you and your assistant's spoken overview, full screen in glass — on the TV or
projector plugged into the computer (or cast to it) when there is one. Esc closes it. It reads the house every half
minute while it is on screen, and nothing once it is closed.

## The look: Liquid Glass

Settings → Zheus One → **Look of the island**: **Liquid Glass** (light along the rims, the cards frosted over a soft
field of color) or **Classic** (the black island). The glass is drawn by the page, not the window (the island lives
in a fixed, transparent window, and blurring the desktop would frost the whole rectangle), so its tint is dark enough
that the page behind the island never reads through. Nothing in it moves: it costs nothing while the island is hidden.

## For Hermes and other agents

With Mochi's home shared with Hermes (Settings → Hermes Agent), `coucou-hook --mcp` also offers **zheus_world** (the
World State in words) and **zheus_mission** (`arrival`, with the minutes, or `situation`). A mission from an agent only
shows the plan: the user's yes runs it, in Coucou.

## What to know

- Tested so far with simulated devices and a simulated Home Assistant, not with a real vacuum or air conditioner. What
  happens on a real Windows PC (the TV mode on a second screen, the voice) is for you to try.
- A plan waiting or running lives in Coucou: if Coucou closes, it is dropped.
- Zheus has no location of its own: "on my way" is what you say, "home" is Home Assistant's people or what you say.
- Temperatures are taken in °C.
- Coucou itself does not connect to Instagram, Meta Ads, Google Calendar or Drive: those go through your assistant's
  own connectors and skills.
