// Settings → Mochi's home: the switch, Home Assistant, the Philips TVs, the devices Mochi may use (with their room,
// whether they ask first, and a way to test them), the routines and the record. Rust owns all of it
// (src-tauri/src/smart_home): the page only asks, and nothing here reaches a device without a click.

import { Bridge } from "../core/bridge";
import {
  ACTIONS, actionsFor, capsLine, cloneRoutine, foundLine, kindLabel, originLabel, roomsOf, stateLine, stepLine, tvLine,
  type DevicePrefs, type Found, type HomeDevice, type HomeStatus, type Routine, type RoutineStep, type TvSummary,
} from "../core/home";
import { h } from "../views/dom";
import { t } from "../i18n/i18n";

type Toggle = (on: boolean, onChange: (v: boolean) => void) => HTMLElement;

const errorText = (err: unknown) => String(err).replace(/^Error:\s*/, "");

/** An input with its starting value (as a property too, which is what is read back). */
function field(attrs: Record<string, string>, value = ""): HTMLInputElement {
  const el = h("input", { ...attrs, value }) as HTMLInputElement;
  el.value = value;
  return el;
}

/** A message line that says how something went. */
function notice(text: string, ok: boolean): HTMLElement {
  return h("div", { class: `notice ${ok ? "ok" : "err"}`, text });
}

/** Runs `work` with `button` disabled, and puts what went wrong (or `done`) in `into`. */
async function busy(button: HTMLButtonElement, into: HTMLElement, work: () => Promise<string | void>) {
  button.disabled = true;
  into.replaceChildren();
  try {
    const done = await work();
    if (done) into.append(notice(done, true));
  } catch (err) {
    into.append(notice(errorText(err), false));
  } finally {
    button.disabled = false;
  }
}

export function homeSection(makeToggle: Toggle): HTMLElement {
  let status: HomeStatus | null = null;
  let devices: HomeDevice[] = [];
  const note = h("div", {});

  // ── The switches ──
  const apply = async (enabled: boolean, askAll: boolean) => {
    note.replaceChildren();
    try {
      show(await Bridge.homeSet(enabled, askAll));
    } catch (err) {
      note.append(notice(errorText(err), false));
    }
  };
  const onSwitch = makeToggle(false, (next) => void apply(next, status?.askAll ?? false));
  const askSwitch = makeToggle(false, (next) => void apply(status?.enabled ?? false, next));

  // ── The blocks, redrawn from the status ──
  const searchBox = h("div", { class: "pill-group" });
  const haBox = h("div", { class: "pill-group" });
  const tvBox = h("div", { class: "pill-group" });
  const deviceBox = h("div", { class: "pill-group" });
  const routineBox = h("div", { class: "pill-group" });
  const logBox = h("div", { class: "pill-group" });

  const show = (next: HomeStatus | null) => {
    if (!next) return;
    status = next;
    for (const [el, on] of [[onSwitch, next.enabled], [askSwitch, next.askAll]] as const) {
      el.classList.toggle("on", on);
      el.setAttribute("aria-pressed", String(on));
    }
    drawHa();
    drawTvs();
    drawRoutines();
  };
  const refresh = async () => show(await Bridge.homeStatus());

  // ── Search ──
  const searchNote = h("div", {});
  const results = h("div", { class: "home-list" });
  const searchButton = h("button", { text: t("Search the network") }) as HTMLButtonElement;
  searchButton.addEventListener("click", () =>
    void busy(searchButton, searchNote, async () => {
      results.replaceChildren();
      const found = await Bridge.homeScan();
      for (const f of found) results.append(foundRow(f));
      if (!found.length) return t("Nothing found. Check that this computer and the devices are on the same network, or add them by their address below.");
    }),
  );
  const foundRow = (f: Found): HTMLElement => {
    const row = h("div", { class: "pill-row" }, h("span", { class: "name", text: f.name }), h("span", { class: "state", text: foundLine(f) }));
    if (f.kind === "homeAssistant" && f.url) {
      const use = h("button", { text: t("Use") });
      use.addEventListener("click", () => {
        haUrl.value = f.url ?? "";
        haToken.focus();
      });
      row.append(use);
    } else if (f.kind === "philipsTv") {
      const add = h("button", { text: t("Add") }) as HTMLButtonElement;
      add.addEventListener("click", () => void addTv(f.host, add));
      row.append(add);
    } else {
      row.append(h("span", { class: "hint", text: t("Not controlled directly: add it to Home Assistant.") }));
    }
    return row;
  };
  searchBox.append(
    h("h3", { text: t("Search the network") }),
    h("div", { class: "hint", text: t("Looks for Home Assistant and TVs on your local network (mDNS and UPnP), for a few seconds. Nothing is added until you choose.") }),
    h("div", { class: "row" }, searchButton),
    results,
    searchNote,
  );

  // ── Home Assistant ──
  const haUrl = h("input", { type: "text", placeholder: "http://homeassistant.local:8123", style: "flex:1 1 auto;min-width:0", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
  const haToken = h("input", { type: "password", style: "flex:1 1 auto;min-width:0", autocomplete: "off", spellcheck: "false" }) as HTMLInputElement;
  const haNote = h("div", {});
  const haConnect = h("button", { class: "primary", text: t("Connect") }) as HTMLButtonElement;
  const haForget = h("button", { text: t("Disconnect") }) as HTMLButtonElement;
  haConnect.addEventListener("click", () =>
    void busy(haConnect, haNote, async () => {
      const info = await Bridge.homeHaConnect(haUrl.value, haToken.value.trim() ? haToken.value : null);
      haToken.value = "";
      await refresh();
      return t("Connected: Home Assistant {version} · {home}", { version: info.version, home: info.location });
    }),
  );
  haForget.addEventListener("click", () =>
    void busy(haForget, haNote, async () => {
      await Bridge.homeHaConnect("", null);
      haUrl.value = "";
      await refresh();
    }),
  );
  const drawHa = () => {
    if (!status) return;
    if (document.activeElement !== haUrl) haUrl.value = status.haUrl;
    haToken.placeholder = status.haToken ? "••••••••" : t("Long-lived access token");
    haForget.hidden = !status.haUrl;
  };
  haBox.append(
    h("h3", { text: "Home Assistant" }),
    h("div", { class: "row" }, h("label", { text: t("Address") }), haUrl),
    h("div", { class: "row" }, h("label", { text: t("Access token") }), haToken),
    h("div", { class: "row" }, haConnect, haForget),
    haNote,
    h("div", { class: "hint", text: t("Create the token in Home Assistant: your profile → Security → Long-lived access tokens. It is kept in the system's credential store, never in a file. Only an address on your local network is accepted. Lights, switches, TVs and speakers become devices; Mochi uses only those you tick below.") }),
  );

  // ── Philips TVs ──
  const tvHost = h("input", { type: "text", placeholder: "192.168.1.20", style: "flex:1 1 auto;min-width:0", spellcheck: "false", autocomplete: "off" }) as HTMLInputElement;
  const tvAdd = h("button", { text: t("Add") }) as HTMLButtonElement;
  const tvNote = h("div", {});
  const tvList = h("div", { class: "home-list" });
  const addTv = (host: string, button: HTMLButtonElement) =>
    busy(button, tvNote, async () => {
      const tv = await Bridge.homeTvAdd(host);
      tvHost.value = "";
      await refresh();
      return tv.needsPairing && !tv.paired
        ? t("Found: {name} ({details}). Now press Pair: the TV shows a PIN.", { name: tv.name, details: tvLine(tv) })
        : t("Found: {name} ({details}).", { name: tv.name, details: tvLine(tv) });
    });
  tvAdd.addEventListener("click", () => void addTv(tvHost.value, tvAdd));

  const tvRow = (tv: TvSummary): HTMLElement => {
    const out = h("div", {});
    const line = h(
      "div",
      { class: "pill-row" },
      h("span", { class: "name", text: tv.name }),
      h("span", { class: "state", text: `${tv.host} · ${tvLine(tv)}` }),
    );
    const pairing = h("div", {});
    // The list is drawn again after a change: what happened is said below it, where it stays.
    const result = tvNote;
    const pair = h("button", { text: tv.paired ? t("Pair again") : t("Pair") }) as HTMLButtonElement;
    pair.addEventListener("click", () =>
      void busy(pair, result, async () => {
        await Bridge.homeTvPair(tv.id);
        const pin = h("input", { type: "text", style: "width:110px", placeholder: "PIN", autocomplete: "off", inputmode: "numeric" }) as HTMLInputElement;
        const confirm = h("button", { class: "primary", text: t("Confirm") }) as HTMLButtonElement;
        confirm.addEventListener("click", () =>
          void busy(confirm, result, async () => {
            await Bridge.homeTvPin(tv.id, pin.value);
            pairing.replaceChildren();
            await refresh();
            return t("Paired. Mochi can now use this TV.");
          }),
        );
        pairing.replaceChildren(h("div", { class: "row" }, h("label", { text: t("PIN on the TV") }), pin, confirm));
        pin.focus();
      }),
    );
    const mac = field({ type: "text", style: "width:170px", placeholder: "A4:B1:C2:D3:E4:F5", spellcheck: "false", autocomplete: "off" }, tv.mac);
    const saveMac = h("button", { text: t("Save") }) as HTMLButtonElement;
    saveMac.addEventListener("click", () =>
      void busy(saveMac, result, async () => {
        await Bridge.homeTvSetMac(tv.id, mac.value);
        await refresh();
      }),
    );
    const remove = h("button", { class: "danger", text: t("Remove") }) as HTMLButtonElement;
    remove.addEventListener("click", () =>
      void busy(remove, result, async () => {
        await Bridge.homeTvRemove(tv.id);
        await refresh();
      }),
    );
    const pairState = tv.needsPairing
      ? h("span", { class: "hint", text: tv.paired ? t("Paired") : t("Needs pairing: press Pair and type the PIN the TV shows.") })
      : h("span", { class: "hint", text: t("No pairing needed") });
    out.append(
      line,
      h("div", { class: "row" }, pairState, tv.needsPairing ? pair : null, remove),
      pairing,
      h("div", { class: "row" }, h("label", { text: t("MAC address") }), mac, saveMac),
    );
    return out;
  };
  const drawTvs = () => {
    tvList.replaceChildren(...(status?.tvs ?? []).map(tvRow));
  };
  tvBox.append(
    h("h3", { text: t("Philips TVs") }),
    h("div", { class: "row" }, h("label", { text: t("TV address") }), tvHost, tvAdd),
    tvList,
    tvNote,
    h("div", { class: "hint", text: t("Coucou asks the TV which JointSpace version it speaks and what it can do, and offers only that. Android TVs (2016 and later) are paired once with a PIN. The TV's address is in its network settings.") }),
    h("div", { class: "hint", text: t("To turn a TV on from standby, turn on \"Switch on with Wi-Fi\" (or \"with Chromecast\") on the TV. The MAC address, found by itself when possible, lets Coucou wake it from deep standby.") }),
  );

  // ── Devices ──
  const deviceNote = h("div", {});
  const deviceList = h("div", { class: "home-list" });
  const rooms = h("datalist", { id: "home-rooms" });
  const readButton = h("button", { text: t("Read the devices") }) as HTMLButtonElement;
  const readDevices = () =>
    busy(readButton, deviceNote, async () => {
      devices = await Bridge.homeDevices();
      drawDevices();
      drawRoutines();
      if (!devices.length) return t("No device yet: connect Home Assistant or add a TV above.");
    });
  readButton.addEventListener("click", () => void readDevices());

  const prefsOf = (d: HomeDevice): DevicePrefs => status?.prefs[d.id] ?? { name: "", room: "", exposed: null, ask: null };

  const deviceRow = (d: HomeDevice): HTMLElement => {
    const prefs = prefsOf(d);
    const result = h("div", {});
    const dot = h("span", { class: "dot", style: `background:${d.state.online ? "var(--green)" : "var(--dim-2)"}` });
    const name = field({ type: "text", style: "flex:1 1 140px;min-width:0" }, prefs.name || d.name);
    const room = field({ type: "text", placeholder: t("Room"), list: "home-rooms", style: "width:130px" }, prefs.room || d.room);
    const exposed = h("input", { type: "checkbox" }) as HTMLInputElement;
    exposed.checked = d.exposed;
    const ask = h("input", { type: "checkbox" }) as HTMLInputElement;
    ask.checked = d.ask;
    const store = () =>
      void (async () => {
        result.replaceChildren();
        try {
          const next: DevicePrefs = {
            name: name.value.trim() === d.name && !prefs.name ? "" : name.value.trim(),
            room: room.value.trim(),
            exposed: exposed.checked,
            ask: ask.checked,
          };
          await Bridge.homeDeviceSet(d.id, next);
          if (status) status.prefs[d.id] = next;
        } catch (err) {
          result.append(notice(errorText(err), false));
        }
      })();
    for (const el of [name, room]) el.addEventListener("change", store);
    for (const el of [exposed, ask]) el.addEventListener("change", store);

    // Test: the actions it reported, the user's own clicks.
    const actions = actionsFor(d);
    const pick = h("select", {}) as HTMLSelectElement;
    for (const a of actions) pick.append(h("option", { value: a.id, text: t(a.label) }));
    const value = h("input", { type: "text", style: "width:150px", list: `home-values-${d.id}` }) as HTMLInputElement;
    const values = h("datalist", { id: `home-values-${d.id}` });
    const fillValues = () => {
      const kind = ACTIONS.find((a) => a.id === pick.value)?.value ?? null;
      value.hidden = kind === null;
      value.placeholder = kind === "percent" ? "0–100" : kind === "app" ? "YouTube" : kind === "channel" ? "5" : "";
      const list = kind === "app" ? d.apps : kind === "channel" ? d.channels.map((c) => `${c.number}`) : [];
      values.replaceChildren(...list.slice(0, 400).map((v) => h("option", { value: v })));
    };
    pick.addEventListener("change", fillValues);
    fillValues();
    const test = h("button", { text: t("Test") }) as HTMLButtonElement;
    test.addEventListener("click", () =>
      void busy(test, result, async () => {
        const r = await Bridge.homeTest(d.id, pick.value, value.hidden ? null : value.value);
        if (!r.ok) throw new Error(r.detail);
        return r.detail || t("Done.");
      }),
    );

    return h(
      "div",
      { class: "home-device" },
      h("div", { class: "row" }, dot, name, room, h("span", { class: "hint", text: `${kindLabel(d.kind)} · ${d.source}${d.model ? ` · ${d.model}` : ""}` })),
      h("div", { class: "hint", text: `${stateLine(d.state)} — ${capsLine(d)}` }),
      h(
        "div",
        { class: "row" },
        h("label", { class: "check" }, exposed, h("span", { text: t("Mochi may use it") })),
        h("label", { class: "check" }, ask, h("span", { text: t("Ask before acting") })),
      ),
      actions.length ? h("div", { class: "row" }, pick, value, values, test) : null,
      result,
    );
  };
  const drawDevices = () => {
    rooms.replaceChildren(...roomsOf(devices, status?.prefs ?? {}).map((r) => h("option", { value: r })));
    deviceList.replaceChildren(...devices.map(deviceRow));
  };
  deviceBox.append(
    h("h3", { text: t("Devices") }),
    h("div", { class: "hint", text: t("Tick the devices Mochi may use, and give each its room: then \"turn on the living room TV\" finds it. Plugs and switches ask before acting unless you untick it; with \"Ask before every action\" on, every device asks.") }),
    h("div", { class: "row" }, readButton),
    deviceNote,
    rooms,
    deviceList,
  );

  // ── Routines ──
  const routineNote = h("div", {});
  const routineList = h("div", { class: "home-list" });
  const editor = h("div", {});
  const suggest = h("button", { text: t("Suggestions") }) as HTMLButtonElement;
  const create = h("button", { text: t("New routine") }) as HTMLButtonElement;
  const suggestions = h("div", { class: "home-list" });
  suggest.addEventListener("click", () =>
    void busy(suggest, routineNote, async () => {
      if (!devices.length) devices = await Bridge.homeDevices();
      const templates = await Bridge.homeTemplates();
      suggestions.replaceChildren(
        ...templates.map((r) => {
          const use = h("button", { text: t("Edit and save") });
          use.addEventListener("click", () => {
            suggestions.replaceChildren();
            edit({ ...cloneRoutine(r), id: "" });
          });
          return h("div", { class: "pill-row" }, h("span", { class: "name", text: r.name }), h("span", { class: "state", text: r.steps.map((s) => stepLine(s, devices)).join(" → ") }), use);
        }),
      );
      if (!templates.length) return t("Tick some devices for Mochi first: suggestions are made of them.");
    }),
  );
  create.addEventListener("click", () => edit({ id: "", name: "", steps: [] }));

  /** The routine editor: a name and its steps, saved only with Save. */
  const edit = (routine: Routine) => {
    const r = cloneRoutine(routine);
    const name = field({ type: "text", placeholder: t("Name, like Good night"), style: "flex:1 1 auto;min-width:0" }, r.name);
    const steps = h("div", { class: "home-list" });
    const usable = devices.filter((d) => d.exposed);
    const drawSteps = () => {
      steps.replaceChildren(
        ...r.steps.map((step, i) => {
          const device = h("select", {}) as HTMLSelectElement;
          for (const d of usable) device.append(h("option", { value: d.id, text: d.room ? `${d.name} (${d.room})` : d.name }));
          if (!usable.some((d) => d.id === step.device)) device.append(h("option", { value: step.device, text: step.device }));
          device.value = step.device;
          const action = h("select", {}) as HTMLSelectElement;
          const fillActions = () => {
            const d = usable.find((x) => x.id === device.value);
            const list = d ? actionsFor(d) : ACTIONS;
            action.replaceChildren(...list.map((a) => h("option", { value: a.id, text: t(a.label) })));
            action.value = list.some((a) => a.id === step.action) ? step.action : (list[0]?.id ?? "");
            step.action = action.value;
          };
          fillActions();
          const value = field({ type: "text", style: "width:120px" }, step.value);
          const wait = field({ type: "number", min: "0", max: "120", style: "width:64px", title: t("Seconds to wait before this step") }, String(step.wait));
          const showValue = () => {
            const kind = ACTIONS.find((a) => a.id === action.value)?.value ?? null;
            value.hidden = kind === null;
            value.placeholder = kind === "percent" ? "0–100" : kind === "app" ? "YouTube" : kind === "channel" ? "5" : "";
          };
          showValue();
          device.addEventListener("change", () => {
            step.device = device.value;
            fillActions();
            showValue();
          });
          action.addEventListener("change", () => {
            step.action = action.value;
            showValue();
          });
          value.addEventListener("input", () => (step.value = value.value));
          wait.addEventListener("input", () => (step.wait = Math.max(0, Math.min(120, Number(wait.value) || 0))));
          const remove = h("button", { text: "×", title: t("Remove") });
          remove.addEventListener("click", () => {
            r.steps.splice(i, 1);
            drawSteps();
          });
          return h("div", { class: "row" }, h("span", { class: "hint", text: `${i + 1}.` }), device, action, value, wait, remove);
        }),
      );
    };
    drawSteps();
    const addStep = h("button", { text: t("Add a step") });
    addStep.addEventListener("click", () => {
      const first = usable[0];
      r.steps.push({ device: first?.id ?? "", action: first ? (actionsFor(first)[0]?.id ?? "turn_on") : "turn_on", value: "", wait: 0 } as RoutineStep);
      drawSteps();
    });
    const result = h("div", {});
    const save = h("button", { class: "primary", text: t("Save") }) as HTMLButtonElement;
    save.addEventListener("click", () =>
      void busy(save, result, async () => {
        r.name = name.value;
        await Bridge.homeRoutineSave(r);
        editor.replaceChildren();
        await refresh();
      }),
    );
    const cancel = h("button", { text: t("Cancel") });
    cancel.addEventListener("click", () => editor.replaceChildren());
    editor.replaceChildren(
      h(
        "div",
        { class: "home-device" },
        h("div", { class: "row" }, h("label", { text: t("Routine") }), name),
        usable.length ? null : h("div", { class: "hint", text: t("Read the devices and tick some for Mochi first.") }),
        steps,
        h("div", { class: "row" }, addStep, h("span", { class: "spacer" }), cancel, save),
        result,
      ),
    );
    name.focus();
  };

  const routineRow = (r: Routine): HTMLElement => {
    const result = h("div", {});
    const run = h("button", { text: t("Run") }) as HTMLButtonElement;
    run.addEventListener("click", () =>
      void busy(run, result, async () => {
        const out = await Bridge.homeRoutineRun(r.id);
        if (!out.ok) throw new Error(out.detail);
        return out.detail;
      }),
    );
    const change = h("button", { text: t("Edit") });
    change.addEventListener("click", () => edit(r));
    const remove = h("button", { class: "danger", text: t("Remove") }) as HTMLButtonElement;
    remove.addEventListener("click", () =>
      void busy(remove, result, async () => {
        await Bridge.homeRoutineDelete(r.id);
        await refresh();
      }),
    );
    return h(
      "div",
      {},
      h("div", { class: "pill-row" }, h("span", { class: "name", text: r.name }), h("span", { class: "state", text: r.steps.map((s) => stepLine(s, devices)).join(" → ") })),
      h("div", { class: "row" }, run, change, remove),
      result,
    );
  };
  const drawRoutines = () => {
    routineList.replaceChildren(...(status?.routines ?? []).map(routineRow));
  };
  routineBox.append(
    h("h3", { text: t("Routines") }),
    h("div", { class: "hint", text: t("A routine is a name and the actions it stands for, in order: say \"Mochi, good night\" and he runs it. Each step is checked like an action asked alone.") }),
    h("div", { class: "row" }, suggest, create),
    suggestions,
    routineNote,
    editor,
    routineList,
  );

  // ── The record ──
  const logList = h("div", { class: "home-list" });
  const logButton = h("button", { text: t("Show the latest actions") }) as HTMLButtonElement;
  logButton.addEventListener("click", () =>
    void (async () => {
      const entries = (await Bridge.homeLog()) ?? [];
      logList.replaceChildren(
        ...entries.map((e) =>
          h(
            "div",
            { class: "pill-row" },
            h("span", { class: "dot", style: `background:${e.ok ? "var(--green)" : "var(--red)"}` }),
            h("span", { class: "name", text: `${e.device} · ${e.action}` }),
            h("span", { class: "state", text: `${e.detail ? `${e.detail} · ` : ""}${originLabel(e.origin)} · ${e.at.slice(5, 16)}` }),
          ),
        ),
      );
      if (!entries.length) logList.append(h("div", { class: "hint", text: t("Nothing yet.") }));
    })(),
  );
  logBox.append(
    h("h3", { text: t("Record") }),
    h("div", { class: "hint", text: t("Every action on the house, who asked and how it went, kept on this computer only.") }),
    h("div", { class: "row" }, logButton),
    logList,
  );

  void refresh();
  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: t("Mochi's home") })),
    h("div", { class: "hint", text: t("Let Mochi use the devices of your house from the chat, on this computer or on your phone: turn the TV on, open an app, dim the lights, run a routine. Through Home Assistant, or straight to Philips TVs on your network. Off by default.") }),
    h("div", { class: "hint", text: t("Mochi needs his hands on (above) and uses only the devices you tick. After each action he asks the device again and tells you what it reports.") }),
    h("div", { class: "row" }, h("label", { text: t("Mochi's home") }), onSwitch),
    h("div", { class: "row" }, h("label", { text: t("Ask before every action") }), askSwitch),
    note,
    searchBox,
    haBox,
    tvBox,
    deviceBox,
    routineBox,
    logBox,
  );
}
