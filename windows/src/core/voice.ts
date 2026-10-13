// Mochi's answers read aloud in the chat on this computer, with the voices the system already has (Windows' own, such
// as "Microsoft Maria" for Brazilian Portuguese; on Linux, the webview's when it has any). Only voices that run on this
// computer are used, so nothing is sent anywhere. Speaking *to* Mochi needs no code: Windows' voice typing
// (Windows + H) writes in the chat's field like a keyboard. Turned on in Settings → Mochi's home → Voice (`readAloud`).

import { language, type Language } from "../i18n/i18n";

/** The longest text read aloud: a long answer is cut after a sentence, the rest stays on screen. */
const MAX_SPOKEN = 600;

/** An answer as it is read aloud: code blocks, Markdown marks and link addresses left out, each line a sentence, at
 *  most `max` characters (cut after a sentence when one ends in the second half). Same as the iPhone's
 *  PCText.spoken. */
export function spokenText(markdown: string, max = MAX_SPOKEN): string {
  const kept: string[] = [];
  let code = false;
  for (const raw of markdown.split(/\r?\n/)) {
    let line = raw.trim();
    if (line.startsWith("```")) {
      code = !code;
      continue;
    }
    if (code || !line) continue;
    while (line && "#>-*•".includes(line[0])) line = line.slice(1).trim();
    if (!line) continue;
    // A list item or a title is a sentence too: the voice pauses after it.
    if (!/[.!?…:;,]$/.test(line.replace(/[*_`]+$/, ""))) line += ".";
    kept.push(line);
  }
  const text = kept
    .join(" ")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/\*\*|__|`|\*/g, "")
    .replace(/\s+/g, " ")
    .trim();
  const chars = [...text];
  if (chars.length <= max) return text;
  const cut = chars.slice(0, max).join("");
  const end = cut.lastIndexOf(". ");
  if (end > max / 2) return `${cut.slice(0, end)}.`;
  return `${cut.trim()}…`;
}

/** The language voices are listed under, for the app's language. */
export function speechLanguage(lang: Language = language()): string {
  switch (lang) {
    case "zh-Hans":
      return "zh-CN";
    case "hi":
      return "hi-IN";
    case "es":
      return "es-ES";
    case "ar":
      return "ar-SA";
    case "fr":
      return "fr-FR";
    case "bn":
      return "bn-IN";
    case "pt-BR":
      return "pt-BR";
    case "ru":
      return "ru-RU";
    case "id":
      return "id-ID";
    default:
      return "en-US";
  }
}

/** What a voice says about itself (SpeechSynthesisVoice). */
export interface VoiceLike {
  name: string;
  lang: string;
  localService?: boolean;
}

/** The voice to read `lang` ("pt-BR") with: one of that very language first, then one of the same language
 *  ("pt-PT"). Only voices that run on this computer: an online voice would send the text away. Null when none. */
export function pickVoice<V extends VoiceLike>(voices: readonly V[], lang: string): V | null {
  const norm = (s: string) => s.replace(/_/g, "-").toLowerCase();
  const want = norm(lang);
  const base = want.split("-")[0];
  const local = voices.filter((v) => v.localService !== false);
  return local.find((v) => norm(v.lang) === want) ?? local.find((v) => norm(v.lang).split("-")[0] === base) ?? null;
}

/** The system's voices. Chromium lists them only once they are loaded: the first time, wait a moment for them. */
export function systemVoices(wait = 1500): Promise<SpeechSynthesisVoice[]> {
  const synth = globalThis.speechSynthesis;
  if (!synth) return Promise.resolve([]);
  const now = synth.getVoices();
  if (now.length) return Promise.resolve(now);
  return new Promise((resolve) => {
    const done = () => {
      clearTimeout(timer);
      synth.removeEventListener?.("voiceschanged", done);
      resolve(synth.getVoices());
    };
    const timer = setTimeout(done, wait);
    synth.addEventListener?.("voiceschanged", done);
  });
}

/** The voice answers are read with, in the app's language; null when this computer has none for it. */
export async function answerVoice(lang = speechLanguage()): Promise<SpeechSynthesisVoice | null> {
  return pickVoice(await systemVoices(), lang);
}

/** Reads `markdown` aloud in the app's language (anything being read stops first). False, and silent, when there is
 *  nothing to say or no voice of this computer for that language. */
export async function speak(markdown: string, lang = speechLanguage()): Promise<boolean> {
  const synth = globalThis.speechSynthesis;
  const text = spokenText(markdown);
  if (!synth || typeof SpeechSynthesisUtterance === "undefined" || !text) return false;
  const voice = await answerVoice(lang);
  if (!voice) return false;
  synth.cancel();
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.voice = voice;
  utterance.lang = voice.lang;
  synth.speak(utterance);
  return true;
}

/** Stops what is being read (a new question was sent). */
export function stopSpeaking() {
  globalThis.speechSynthesis?.cancel();
}
