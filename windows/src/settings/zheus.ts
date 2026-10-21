// Settings → Zheus One, in Liquid Glass like the island: the switch, the island's look (Liquid Glass or classic),
// how Zheus prepares the user's arrival (the way home and the air conditioning, the vacuum and its rooms, the lights
// and the TV, the day's summary), the Autopilot's goals with their limits, and the TV mode. Rust owns all of it
// (src-tauri/src/zheus_one): the page only asks, and nothing here reaches a device.

import { Bridge } from "../core/bridge";
import { kindLabel, type HomeDevice } from "../core/home";
import {
  DEFAULT_ARRIVAL, DEFAULT_GOALS, clock, minuteOf, modeChoices, type ArrivalPrefs, type Goals, type Mode, type RoomTime,
  type ZheusStatus,
} from "../core/zheus";
import { h, svg } from "../views/dom";
import { ICONS } from "../views/icons";
import { t } from "../i18n/i18n";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;
type GoalId = "energy" | "tidy" | "agenda" | "business";

/** The island's look, which Settings keeps with the island's own settings. */
export interface LookChoice {
  get: () => string;
  set: (look: "glass" | "classic") => void;
}

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

export function notice(text: string, ok: boolean): HTMLElement {
  return h("div", { class: `notice ${ok ? "ok" : "err"}`, text });
}

/** A number with its unit inside the field: "30 min", "23 °C". */
export function number(value: number, min: number, max: number, unit: string): { el: HTMLElement; input: HTMLInputElement } {
  const input = h("input", { type: "number", min: String(min), max: String(max) }) as HTMLInputElement;
  input.value = String(value);
  return { el: h("label", { class: "zheus-input" }, input, h("span", { class: "zheus-unit", text: unit })), input };
}

/** A time of day, with the system's own picker. */
function time(minute: number): HTMLInputElement {
  const el = h("input", { type: "time", class: "zheus-time" }) as HTMLInputElement;
  el.value = clock(minute);
  return el;
}

function read(el: HTMLInputElement, fallback: number): number {
  const n = Number(el.value);
  return el.value.trim() !== "" && Number.isFinite(n) ? Math.round(n) : fallback;
}

function readTime(el: HTMLInputElement, fallback: number): number {
  return minuteOf(el.value) ?? fallback;
}

/** One setting as a small tile of glass: its name above, its field below, and a word on it if it needs one. */
export function field(label: string, control: (Node | null)[], hint?: string): HTMLElement {
  return h(
    "div",
    { class: "zheus-field" },
    h("span", { class: "zheus-field-label", text: label }),
    h("div", { class: "zheus-field-control" }, ...control),
    hint ? h("span", { class: "zheus-field-hint", text: hint }) : null,
  );
}

/** A card of glass with its icon and title. */
export function glassCard(icon: SVGSVGElement, title: string, ...children: (Node | null)[]): HTMLElement {
  return h(
    "div",
    { class: "zheus-box" },
    h("div", { class: "zheus-box-head" }, h("span", { class: "zheus-box-icon" }, icon), h("h3", { text: title })),
    ...children,
  );
}

const named = (d: HomeDevice) => (d.room ? `${d.name} (${d.room})` : d.name);

export function zheusSection(makeToggle: Toggle, look: LookChoice): HTMLElement {
  let status: ZheusStatus | null = null;
  let devices: HomeDevice[] = [];
  const note = h("div", {});
  const body = h("div", { class: "zheus-panel-body" });

  // ── The switch ──
  const onSwitch = makeToggle(false, (next) =>
    void (async () => {
      note.replaceChildren();
      try {
        status = await Bridge.zheusSet(next);
        draw();
      } catch (err) {
        note.append(notice(errorText(err), false));
      }
    })(),
  );
  onSwitch.setAttribute("aria-label", "Zheus One");

  // ── The look: two small islands to pick from ──
  const looks = h("div", { class: "zheus-looks" });
  const drawLooks = () => {
    const current = look.get() === "classic" ? "classic" : "glass";
    looks.replaceChildren(
      ...(["glass", "classic"] as const).map((id) =>
        h(
          "button",
          {
            class: `zheus-look ${id}${current === id ? " on" : ""}`,
            "aria-pressed": String(current === id),
            onclick: () => {
              look.set(id);
              drawLooks();
            },
          },
          h("span", { class: "zheus-look-preview" }, h("span", { class: "zheus-look-island" }, h("i"), h("i"), h("i"))),
          h("span", { class: "zheus-look-name", text: id === "glass" ? t("Liquid Glass") : t("Classic") }),
        ),
      ),
    );
  };
  drawLooks();

  // ── The cards, drawn from the status ──
  const draw = () => {
    const enabled = status?.enabled === true;
    onSwitch.classList.toggle("on", enabled);
    onSwitch.setAttribute("aria-pressed", String(enabled));
    const a: ArrivalPrefs = { ...DEFAULT_ARRIVAL, ...(status?.arrival ?? {}) };
    const g: Goals = { ...DEFAULT_GOALS, ...(status?.goals ?? {}) };

    const eta = number(a.eta, 1, 240, t("min"));
    const comfort = number(a.comfort, 0, 30, "°C");
    const lead = number(a.acLead, 0, 120, t("min"));
    const summary = makeToggle(a.summary, () => {});
    const vacuum = makeToggle(a.vacuum, () => {});
    const whole = number(a.whole, 5, 240, t("min"));
    const battery = number(a.minBattery, 0, 100, "%");
    const darkFrom = time(a.darkFrom);
    const darkUntil = time(a.darkUntil);

    // The rooms the vacuum cleans, in the order they matter.
    const rooms: RoomTime[] = a.rooms.map((r) => ({ ...r }));
    const roomList = h("div", { class: "zheus-rooms" });
    const drawRooms = () => {
      roomList.replaceChildren(
        ...rooms.map((r, i) => {
          const name = h("input", { type: "text", placeholder: t("Room"), class: "zheus-room-name" }) as HTMLInputElement;
          name.value = r.room;
          name.addEventListener("input", () => (r.room = name.value));
          const minutes = number(r.minutes, 1, 240, t("min"));
          minutes.input.addEventListener("input", () => (r.minutes = read(minutes.input, r.minutes)));
          const remove = h("button", { class: "zheus-remove", text: "×", title: t("Remove"), "aria-label": t("Remove") });
          remove.addEventListener("click", () => {
            rooms.splice(i, 1);
            drawRooms();
          });
          return h("div", { class: "zheus-room" }, h("span", { class: "zheus-room-rank", text: String(i + 1) }), name, minutes.el, remove);
        }),
      );
    };
    drawRooms();
    const addRoom = h("button", { class: "zheus-add", text: `+ ${t("Add a room")}` });
    addRoom.addEventListener("click", () => {
      rooms.push({ room: "", minutes: 15 });
      drawRooms();
    });

    // The lights and the TV for the arrival, from the devices shared with Zheus.
    const lightBoxes = devices
      .filter((d) => d.kind === "light" && d.exposed)
      .map((d) => {
        const box = h("input", { type: "checkbox" }) as HTMLInputElement;
        box.checked = a.lights.includes(d.id);
        box.dataset.id = d.id;
        return { box, el: h("label", { class: "zheus-check" }, box, h("span", { text: named(d) })) };
      });
    const tvSelect = h("select", {}) as HTMLSelectElement;
    tvSelect.append(h("option", { value: "", text: t("None") }));
    for (const d of devices.filter((x) => x.kind === "tv" && x.exposed)) tvSelect.append(h("option", { value: d.id, text: named(d) }));
    tvSelect.value = a.tv;
    const tvApp = h("input", { type: "text", placeholder: "Netflix", title: t("App"), "aria-label": t("App"), class: "zheus-tv-app" }) as HTMLInputElement;
    tvApp.value = a.tvApp;

    // The Autopilot's goals: each a choice of Off, Suggest or Act (business only ever suggests), with its own limit.
    const tidyFrom = time(g.tidyFrom);
    const tidyUntil = time(g.tidyUntil);
    const agendaAt = time(g.agendaAt);
    const away = number(g.awayMinutes, 5, 240, t("min"));
    const goalRows: [GoalId, string, string, HTMLElement | null][] = [
      [
        "energy",
        t("Less energy wasted"),
        t("Turns off the lights, the TV and the air conditioning left on while nobody is home (never a plug)."),
        h("div", { class: "zheus-goal-extra" }, h("span", { text: t("Empty house for") }), away.el, h("span", { text: t("before turning things off") })),
      ],
      [
        "tidy",
        t("A tidy house"),
        t("The vacuum cleans while the house is empty, once a day, in its hours, and goes back when someone comes home."),
        h("div", { class: "zheus-goal-extra" }, h("span", { text: t("Vacuum hours") }), tidyFrom, h("span", { text: t("until") }), tidyUntil),
      ],
      [
        "agenda",
        t("Your day prepared"),
        t("In the morning, the summary of your calendar, tasks and messages, by the assistant. Read only."),
        h("div", { class: "zheus-goal-extra" }, h("span", { text: t("Morning summary at") }), agendaAt),
      ],
      [
        "business",
        t("Your business moving on"),
        t("Looks at your campaigns and sales and prepares proposals. Only ever suggests: nothing is published, spent or sent without you."),
        null,
      ],
    ];
    const modes = new Map<GoalId, Mode>();
    const goalEls = goalRows.map(([id, label, hint, extra]) => {
      modes.set(id, id === "business" && g[id] === "act" ? "suggest" : g[id]);
      const seg = h("div", { class: "zheus-seg", role: "radiogroup", "aria-label": label });
      const drawSeg = () =>
        seg.replaceChildren(
          ...modeChoices(id).map((m) =>
            h("button", {
              class: `${m.id}${modes.get(id) === m.id ? " on" : ""}`,
              role: "radio",
              "aria-checked": String(modes.get(id) === m.id),
              title: m.title,
              text: m.label,
              onclick: () => {
                modes.set(id, m.id);
                drawSeg();
              },
            }),
          ),
        );
      drawSeg();
      return h(
        "div",
        { class: "zheus-goal" },
        h(
          "div",
          { class: "zheus-goal-main" },
          h("div", { class: "zheus-goal-text" }, h("span", { class: "zheus-goal-name", text: label }), h("span", { class: "hint", text: hint })),
          seg,
        ),
        extra,
      );
    });

    const saveNote = h("div", {});
    const saveButton = h("button", { class: "primary zheus-save", text: t("Save") }) as HTMLButtonElement;
    saveButton.addEventListener("click", () =>
      void (async () => {
        saveButton.disabled = true;
        saveNote.replaceChildren();
        const arrival: ArrivalPrefs = {
          eta: read(eta.input, a.eta),
          comfort: read(comfort.input, a.comfort),
          acLead: read(lead.input, a.acLead),
          vacuum: vacuum.classList.contains("on"),
          rooms: rooms.filter((r) => r.room.trim()).map((r) => ({ room: r.room.trim(), minutes: r.minutes })),
          whole: read(whole.input, a.whole),
          minBattery: read(battery.input, a.minBattery),
          lights: lightBoxes.filter((l) => l.box.checked).map((l) => l.box.dataset.id ?? "").filter(Boolean),
          darkFrom: readTime(darkFrom, a.darkFrom),
          darkUntil: readTime(darkUntil, a.darkUntil),
          tv: tvSelect.value,
          tvApp: tvApp.value.trim(),
          summary: summary.classList.contains("on"),
        };
        const goals: Goals = {
          energy: modes.get("energy") ?? "off",
          tidy: modes.get("tidy") ?? "off",
          agenda: modes.get("agenda") ?? "off",
          business: modes.get("business") ?? "off",
          tidyFrom: readTime(tidyFrom, g.tidyFrom),
          tidyUntil: readTime(tidyUntil, g.tidyUntil),
          agendaAt: readTime(agendaAt, g.agendaAt),
          awayMinutes: read(away.input, g.awayMinutes),
        };
        try {
          status = await Bridge.zheusPrefsSet(arrival, goals);
          draw();
          note.replaceChildren(notice(t("Saved."), true));
        } catch (err) {
          saveNote.append(notice(errorText(err), false));
        } finally {
          saveButton.disabled = false;
        }
      })(),
    );

    const tvButton = h("button", { class: "zheus-add" }, svg(ICONS.tv, 13, { stroke: 1.8 }), h("span", { text: t("Open on the TV") })) as HTMLButtonElement;
    tvButton.addEventListener("click", () => void Bridge.zheusTv(true).catch((err) => saveNote.append(notice(errorText(err), false))));

    body.replaceChildren(
      glassCard(
        svg(ICONS.house, 15),
        t("Your arrival"),
        h("p", { class: "hint", text: t("Say \"I'm on my way home\" (in the chat, on your phone or aloud) and Zheus plans the whole house for your arrival. It shows you the plan, with what clashes, and starts it only when you say yes.") }),
        h(
          "div",
          { class: "zheus-grid" },
          field(t("Usual way home"), [eta.el]),
          field(t("Air conditioning at"), [comfort.el], t("0 leaves it alone")),
          field(t("Turned on before you arrive"), [lead.el]),
          field(t("How the day went, at the arrival"), [summary]),
        ),
      ),
      glassCard(
        svg(ICONS.vacuum, 15, { stroke: 1.7 }),
        kindLabel("vacuum"),
        h(
          "div",
          { class: "zheus-grid three" },
          field(t("Vacuum before you arrive"), [vacuum]),
          field(t("Whole house"), [whole.el]),
          field(t("Lowest battery"), [battery.el]),
        ),
        h("div", { class: "zheus-sub" }, h("span", { class: "zheus-sub-title", text: t("Rooms") })),
        h("p", { class: "hint", text: t("The rooms the vacuum cleans, in the order they matter, with the minutes each takes: when the way home is short, Zheus offers to clean only what fits. Their names are your Home Assistant areas (room cleaning needs Home Assistant 2026.3 or later and a vacuum that supports it). Empty: the whole house.") }),
        roomList,
        h("div", { class: "row" }, addRoom),
      ),
      glassCard(
        svg(ICONS.bulb, 15, { stroke: 1.7 }),
        t("Lights and TV"),
        h(
          "div",
          { class: "zheus-grid" },
          field(t("Dark from"), [darkFrom, h("span", { class: "zheus-until", text: t("until") }), darkUntil]),
          field(t("TV at the arrival"), [tvSelect, tvApp]),
        ),
        h("p", { class: "hint", text: lightBoxes.length ? t("The lights turned on for an arrival after dark (none ticked: the living room's).") : t("No light is shared with Zheus yet (Settings → Mochi's home).") }),
        lightBoxes.length ? h("div", { class: "zheus-checks" }, ...lightBoxes.map((l) => l.el)) : null,
      ),
      glassCard(
        svg(ICONS.sparkles, 15),
        t("Autopilot"),
        h("p", { class: "hint", text: t("Standing goals instead of repeated orders: Zheus looks at the house every few minutes and suggests, or acts within the limits you set. Spending, publishing, writing to people and the house's safety are never done alone, and a device set to ask always waits for your click.") }),
        h("div", { class: "zheus-goals" }, ...goalEls),
      ),
      h("div", { class: "zheus-actions" }, saveNote, saveButton),
      glassCard(
        svg(ICONS.tv, 15, { stroke: 1.7 }),
        t("TV mode"),
        h("p", { class: "hint", text: t("\"Show our situation\": the house, the plan and the decisions waiting, full screen on the TV or projector plugged into this computer (or cast to). Esc closes it.") }),
        h("div", { class: "row" }, tvButton),
      ),
    );
    body.classList.toggle("off", !enabled);
  };

  const refresh = async () => {
    status = (await Bridge.zheusStatus()) ?? null;
    devices = (await Bridge.homeDevices().catch(() => [])) ?? [];
    draw();
  };
  void refresh();

  return h(
    "section",
    { id: "zheus", class: "zheus-panel" },
    h(
      "div",
      { class: "zheus-hero" },
      h("span", { class: "zheus-hero-icon" }, svg(ICONS.sparkles, 22)),
      h(
        "div",
        { class: "zheus-hero-text" },
        h("h2", {}, h("span", { text: "Zheus One" })),
        h("p", { class: "hint", text: t("Your assistant as one intelligence for the house and the day: missions instead of fifteen orders, a picture of the whole house, a plan shown before anything runs, and goals it keeps for you. It uses only the devices you share in Mochi's home. Off by default.") }),
      ),
      onSwitch,
    ),
    glassCard(svg(ICONS.sparkles, 13), t("Look of the island"), looks),
    note,
    body,
  );
}
