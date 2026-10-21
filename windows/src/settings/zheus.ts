// Settings → Zheus One: the switch, the island's look (Liquid Glass or classic), how Zheus prepares the user's arrival
// (the way home, the air conditioning, the vacuum and its rooms, the lights, the TV, the day's summary), the
// Autopilot's goals with their limits, and the TV mode. Rust owns all of it (src-tauri/src/zheus_one): the page only
// asks, and nothing here reaches a device.

import { Bridge } from "../core/bridge";
import type { HomeDevice } from "../core/home";
import {
  DEFAULT_ARRIVAL, DEFAULT_GOALS, clock, minuteOf, modeChoices, type ArrivalPrefs, type Goals, type Mode, type RoomTime,
  type ZheusStatus,
} from "../core/zheus";
import { h } from "../views/dom";
import { t } from "../i18n/i18n";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;

/** The island's look, which Settings keeps with the island's own settings. */
export interface LookChoice {
  get: () => string;
  set: (look: "glass" | "classic") => void;
}

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

function notice(text: string, ok: boolean): HTMLElement {
  return h("div", { class: `notice ${ok ? "ok" : "err"}`, text });
}

function number(value: number, min: number, max: number, width = "72px"): HTMLInputElement {
  const el = h("input", { type: "number", min: String(min), max: String(max), style: `width:${width}` }) as HTMLInputElement;
  el.value = String(value);
  return el;
}

function time(minute: number): HTMLInputElement {
  const el = h("input", { type: "text", placeholder: "18:00", style: "width:72px" }) as HTMLInputElement;
  el.value = clock(minute);
  return el;
}

function read(el: HTMLInputElement, fallback: number): number {
  const n = Number(el.value);
  return Number.isFinite(n) ? Math.round(n) : fallback;
}

function readTime(el: HTMLInputElement, fallback: number): number {
  return minuteOf(el.value) ?? fallback;
}

export function zheusSection(makeToggle: Toggle, look: LookChoice): HTMLElement {
  let status: ZheusStatus | null = null;
  let devices: HomeDevice[] = [];
  const note = h("div", {});
  const form = h("div", { class: "pill-group zheus-form" });

  // ── The switch and the look ──
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
  const looks = h("div", { class: "zheus-segmented" });
  const drawLooks = () => {
    looks.replaceChildren(
      ...(["glass", "classic"] as const).map((id) =>
        h("button", {
          class: look.get() === id || (id === "glass" && look.get() !== "classic") ? "on" : "",
          text: id === "glass" ? t("Liquid Glass") : t("Classic"),
          onclick: () => {
            look.set(id);
            drawLooks();
          },
        }),
      ),
    );
  };
  drawLooks();

  // ── The form, drawn from the status ──
  const draw = () => {
    onSwitch.classList.toggle("on", status?.enabled === true);
    onSwitch.setAttribute("aria-pressed", String(status?.enabled === true));
    const a: ArrivalPrefs = { ...DEFAULT_ARRIVAL, ...(status?.arrival ?? {}) };
    const g: Goals = { ...DEFAULT_GOALS, ...(status?.goals ?? {}) };

    const eta = number(a.eta, 1, 240);
    const comfort = number(a.comfort, 0, 30);
    const lead = number(a.acLead, 0, 120);
    const vacuum = makeToggle(a.vacuum, () => {});
    const whole = number(a.whole, 5, 240);
    const battery = number(a.minBattery, 0, 100);
    const darkFrom = time(a.darkFrom);
    const darkUntil = time(a.darkUntil);
    const summary = makeToggle(a.summary, () => {});

    // The rooms the vacuum cleans, in the order they matter.
    const rooms: RoomTime[] = a.rooms.map((r) => ({ ...r }));
    const roomList = h("div", { class: "zheus-rooms" });
    const drawRooms = () => {
      roomList.replaceChildren(
        ...rooms.map((r, i) => {
          const name = h("input", { type: "text", placeholder: t("Room"), style: "width:160px" }) as HTMLInputElement;
          name.value = r.room;
          name.addEventListener("input", () => (r.room = name.value));
          const minutes = number(r.minutes, 1, 240, "64px");
          minutes.addEventListener("input", () => (r.minutes = read(minutes, r.minutes)));
          const remove = h("button", { class: "danger", text: "×", title: t("Remove") });
          remove.addEventListener("click", () => {
            rooms.splice(i, 1);
            drawRooms();
          });
          return h("div", { class: "row" }, name, minutes, h("span", { class: "hint", text: t("min") }), remove);
        }),
      );
    };
    drawRooms();
    const addRoom = h("button", { text: t("Add a room") });
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
        return { box, el: h("label", { class: "zheus-check" }, box, h("span", { text: d.room ? `${d.name} (${d.room})` : d.name })) };
      });
    const tvSelect = h("select", {}) as HTMLSelectElement;
    tvSelect.append(h("option", { value: "", text: t("None") }));
    for (const d of devices.filter((x) => x.kind === "tv" && x.exposed)) {
      tvSelect.append(h("option", { value: d.id, text: d.room ? `${d.name} (${d.room})` : d.name }));
    }
    tvSelect.value = a.tv;
    const tvApp = h("input", { type: "text", placeholder: "Netflix", style: "width:140px" }) as HTMLInputElement;
    tvApp.value = a.tvApp;

    // The Autopilot's goals.
    const goalRows: [keyof Pick<Goals, "energy" | "tidy" | "agenda" | "business">, string, string][] = [
      ["energy", t("Less energy wasted"), t("Turns off the lights, the TV and the air conditioning left on while nobody is home (never a plug).")],
      ["tidy", t("A tidy house"), t("The vacuum cleans while the house is empty, once a day, in its hours, and goes back when someone comes home.")],
      ["agenda", t("Your day prepared"), t("In the morning, the summary of your calendar, tasks and messages, by the assistant. Read only.")],
      ["business", t("Your business moving on"), t("Looks at your campaigns and sales and prepares proposals. Only ever suggests: nothing is published, spent or sent without you.")],
    ];
    const selects = new Map<string, HTMLSelectElement>();
    const goalEls = goalRows.map(([id, label, hint]) => {
      const select = h("select", {}) as HTMLSelectElement;
      for (const m of modeChoices(id)) select.append(h("option", { value: m.id, text: m.label }));
      select.value = g[id] === "act" && id === "business" ? "suggest" : g[id];
      selects.set(id, select);
      return h("div", {}, h("div", { class: "row" }, h("label", { text: label }), select), h("div", { class: "hint", text: hint }));
    });
    const tidyFrom = time(g.tidyFrom);
    const tidyUntil = time(g.tidyUntil);
    const agendaAt = time(g.agendaAt);
    const away = number(g.awayMinutes, 5, 240);

    const saveNote = h("div", {});
    const saveButton = h("button", { class: "primary", text: t("Save") }) as HTMLButtonElement;
    saveButton.addEventListener("click", () =>
      void (async () => {
        saveButton.disabled = true;
        saveNote.replaceChildren();
        const arrival: ArrivalPrefs = {
          eta: read(eta, a.eta),
          comfort: read(comfort, a.comfort),
          acLead: read(lead, a.acLead),
          vacuum: vacuum.classList.contains("on"),
          rooms: rooms.filter((r) => r.room.trim()).map((r) => ({ room: r.room.trim(), minutes: r.minutes })),
          whole: read(whole, a.whole),
          minBattery: read(battery, a.minBattery),
          lights: lightBoxes.filter((l) => l.box.checked).map((l) => l.box.dataset.id ?? "").filter(Boolean),
          darkFrom: readTime(darkFrom, a.darkFrom),
          darkUntil: readTime(darkUntil, a.darkUntil),
          tv: tvSelect.value,
          tvApp: tvApp.value.trim(),
          summary: summary.classList.contains("on"),
        };
        const goals: Goals = {
          energy: (selects.get("energy")?.value ?? "off") as Mode,
          tidy: (selects.get("tidy")?.value ?? "off") as Mode,
          agenda: (selects.get("agenda")?.value ?? "off") as Mode,
          business: (selects.get("business")?.value ?? "off") as Mode,
          tidyFrom: readTime(tidyFrom, g.tidyFrom),
          tidyUntil: readTime(tidyUntil, g.tidyUntil),
          agendaAt: readTime(agendaAt, g.agendaAt),
          awayMinutes: read(away, g.awayMinutes),
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

    const tvButton = h("button", { text: t("Open on the TV") }) as HTMLButtonElement;
    tvButton.addEventListener("click", () => void Bridge.zheusTv(true).catch((err) => saveNote.append(notice(errorText(err), false))));

    form.replaceChildren(
      h("h3", { text: t("Your arrival") }),
      h("div", { class: "hint", text: t("Say \"I'm on my way home\" (in the chat, on your phone or aloud) and Zheus plans the whole house for your arrival. It shows you the plan, with what clashes, and starts it only when you say yes.") }),
      h("div", { class: "row" }, h("label", { text: t("Usual way home") }), eta, h("span", { class: "hint", text: t("min") })),
      h("div", { class: "row" }, h("label", { text: t("Air conditioning at") }), comfort, h("span", { class: "hint", text: t("°C (0 leaves it alone)") })),
      h("div", { class: "row" }, h("label", { text: t("Turned on before you arrive") }), lead, h("span", { class: "hint", text: t("min") })),
      h("div", { class: "row" }, h("label", { text: t("Vacuum before you arrive") }), vacuum),
      h("div", { class: "row" }, h("label", { text: t("Whole house") }), whole, h("span", { class: "hint", text: t("min") }), h("label", { text: t("Lowest battery") }), battery, h("span", { class: "hint", text: "%" })),
      h("div", { class: "hint", text: t("The rooms the vacuum cleans, in the order they matter, with the minutes each takes: when the way home is short, Zheus offers to clean only what fits. Their names are your Home Assistant areas (room cleaning needs Home Assistant 2026.3 or later and a vacuum that supports it). Empty: the whole house.") }),
      roomList,
      h("div", { class: "row" }, addRoom),
      h("div", { class: "row" }, h("label", { text: t("Dark from") }), darkFrom, h("label", { text: t("until") }), darkUntil),
      h("div", { class: "hint", text: lightBoxes.length ? t("The lights turned on for an arrival after dark (none ticked: the living room's).") : t("No light is shared with Zheus yet (Settings → Mochi's home).") }),
      h("div", { class: "zheus-checks" }, ...lightBoxes.map((l) => l.el)),
      h("div", { class: "row" }, h("label", { text: t("TV at the arrival") }), tvSelect, h("label", { text: t("App") }), tvApp),
      h("div", { class: "row" }, h("label", { text: t("How the day went, at the arrival") }), summary),
      h("h3", { text: t("Autopilot") }),
      h("div", { class: "hint", text: t("Standing goals instead of repeated orders: Zheus looks at the house every few minutes and suggests, or acts within the limits you set. Spending, publishing, writing to people and the house's safety are never done alone, and a device set to ask always waits for your click.") }),
      ...goalEls,
      h("div", { class: "row" }, h("label", { text: t("Vacuum hours") }), tidyFrom, h("label", { text: t("until") }), tidyUntil),
      h("div", { class: "row" }, h("label", { text: t("Morning summary at") }), agendaAt),
      h("div", { class: "row" }, h("label", { text: t("Empty house for") }), away, h("span", { class: "hint", text: t("min, before turning things off") })),
      h("div", { class: "row" }, saveButton),
      saveNote,
      h("h3", { text: t("TV mode") }),
      h("div", { class: "hint", text: t("\"Show our situation\": the house, the plan and the decisions waiting, full screen on the TV or projector plugged into this computer (or cast to). Esc closes it.") }),
      h("div", { class: "row" }, tvButton),
    );
  };

  const refresh = async () => {
    status = (await Bridge.zheusStatus()) ?? null;
    devices = (await Bridge.homeDevices().catch(() => [])) ?? [];
    draw();
  };
  void refresh();

  return h(
    "section",
    { id: "zheus" },
    h("h2", {}, h("span", { text: "Zheus One" })),
    h("div", { class: "hint", text: t("Your assistant as one intelligence for the house and the day: missions instead of fifteen orders, a picture of the whole house, a plan shown before anything runs, and goals it keeps for you. It uses only the devices you share in Mochi's home. Off by default.") }),
    h("div", { class: "row" }, h("label", { text: "Zheus One" }), onSwitch),
    h("div", { class: "row" }, h("label", { text: t("Look of the island") }), looks),
    note,
    form,
  );
}
