// Small pieces every settings section draws with.

import { h } from "../views/dom";

export function statusDot(ok: boolean): HTMLElement {
  const color = ok ? "#34d399" : "#fb7185";
  return h("i", { class: "dot", style: `background:${color};color:${color}` });
}

export function renderDiff(text: string): HTMLElement {
  const box = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
    box.append(h("div", { class: cls, text: line }));
  }
  return box;
}

/** Whether a key is kept: a lit drop when it is, an empty one when not (not having a key is not an error). */
export function keyDot(present: boolean): HTMLElement {
  return h("i", { class: present ? "dot key-dot set" : "dot key-dot" });
}

/** A failure to keep or remove a key, on its drop. */
export function keyDotFailed(dot: HTMLElement) {
  dot.className = "dot key-dot failed";
}

export function keyDotSet(dot: HTMLElement, present: boolean) {
  dot.className = present ? "dot key-dot set" : "dot key-dot";
}

/** A key's field: a well holding what is typed and the button that keeps it, as a search field holds its button. */
export function keyWell(input: HTMLInputElement, ...buttons: HTMLElement[]): HTMLElement {
  input.style.removeProperty("flex");
  input.style.removeProperty("min-width");
  return h("div", { class: "key-well" }, input, ...buttons);
}
