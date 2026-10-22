// The setup.exe's "Your assistant" page (src-tauri/nsis/hooks.nsh, declared by Coucou's copy of Tauri's template,
// src-tauri/nsis/installer.nsi): it offers what Coucou knows (hermes_setup.rs, secrets.rs), it hands over exactly the
// setup.json first_run.rs reads, and the template copy is still Tauri's, plus Coucou's two changes. The installer
// itself only runs on Windows; these tests read the sources.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8").replace(/\r\n/g, "\n");
const HOOKS = read("src-tauri/nsis/hooks.nsh");
const TEMPLATE = read("src-tauri/nsis/installer.nsi");
const RUST = read("src-tauri/src/hermes_setup.rs");

// The shared contract's ids (brains with their keystore entry, tones, skill packs) and Tauri's template.
const BRAINS = {
  chatgpt: null,
  openai: "openai-api-key",
  anthropic: "anthropic-api-key",
  google: "google-api-key",
  openrouter: "openrouter-api-key",
  ollama: null,
  lmstudio: null,
};
const PERSONAS = ["friendly", "direct", "professional", "playful", "butler"];
const SKILLS = ["home", "search", "news"];
const CLI = "2.12.0";
// crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi at the tag @tauri-apps/cli-v2.12.0.
const UPSTREAM_SHA256 = "dabed59013b1d78b879a1a85bc7f2eed2993b33a9a90cdabe5946de3d3950597";

/** The rows of one of hooks.nsh's tables (`!macro CoucouBrains EACH` …), each as its list of values. */
function table(name) {
  const body = HOOKS.match(new RegExp(`!macro ${name} EACH\\n([\\s\\S]*?)!macroend`))?.[1];
  assert.ok(body, `hooks.nsh has no ${name} table`);
  return [...body.matchAll(/!insertmacro \$\{EACH\} (.*)/g)].map(([, row]) =>
    [...row.matchAll(/"([^"]*)"|(\S+)/g)].map(([, quoted, bare]) => quoted ?? bare),
  );
}

/** The body of a Function of hooks.nsh. */
function fn(name) {
  const body = HOOKS.match(new RegExp(`\\nFunction ${name}\\n([\\s\\S]*?)\\nFunctionEnd`))?.[1];
  assert.ok(body, `hooks.nsh has no Function ${name}`);
  return body;
}

/** A Rust string constant (`pub const KEY: &str = "…";`) of a module of src-tauri/src. */
function rustConst(module, name) {
  const value = read(`src-tauri/src/${module}.rs`).match(new RegExp(`pub const ${name}: &str = "([^"]*)";`))?.[1];
  assert.ok(value !== undefined, `${module}.rs has no ${name}`);
  return value;
}

/** hermes_setup.rs BRAINS as { id: keystore entry or null }. */
function rustBrains() {
  const list = RUST.match(/pub const BRAINS: &\[Brain\] = &\[([\s\S]*?)\n\];/)?.[1];
  assert.ok(list, "hermes_setup.rs has no BRAINS table");
  const brains = {};
  for (const [, fields] of list.matchAll(/Brain \{([^}]*)\}/g)) {
    const id = fields.match(/\bid: "([^"]+)"/)[1];
    const key = fields.match(/\bkey: (None|Some\(([^)]*)\))/);
    assert.ok(key, `no key for ${id}`);
    let entry = key[2] ?? null;
    if (entry?.startsWith('"')) entry = entry.slice(1, -1);
    else if (entry) {
      const [, module, constant] = entry.match(/^crate::(\w+)::(\w+)$/);
      entry = rustConst(module, constant);
    }
    brains[id] = entry;
  }
  return brains;
}

/** A Rust list of ids (`pub const PERSONAS: &[&str] = &[…];`). */
function rustList(name) {
  const list = RUST.match(new RegExp(`pub const ${name}: &\\[&str\\] = &\\[([^\\]]*)\\];`))?.[1];
  assert.ok(list, `hermes_setup.rs has no ${name}`);
  return [...list.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

/** Every text the page and its hooks show, as [English, French, Portuguese]. */
function texts() {
  const calls = [...HOOKS.matchAll(/!insertmacro CoucouText \S+ "([^"]*)" "([^"]*)" "([^"]*)"/g)].map((m) => m.slice(1));
  // The macro's own `!insertmacro CoucouText $0 "${EN}" "${FR}" "${PT}"` lines pass a table row's labels along.
  const own = calls.filter(([en]) => !en.startsWith("${"));
  const rows = [...table("CoucouBrains"), ...table("CoucouPersonas"), ...table("CoucouSkills")].map((row) => row.slice(-3));
  return [...own, ...rows];
}

// ── The template copy ─────────────────────────────────────────────────────────

test("the template copy is Tauri's 2.12.0 template plus Coucou's six changes, and the CLI is pinned to it", () => {
  const pkg = JSON.parse(read("package.json"));
  const lock = JSON.parse(read("package-lock.json"));
  assert.equal(pkg.devDependencies["@tauri-apps/cli"], CLI, "exactly, not a range: the template copy belongs to it");
  assert.equal(lock.packages[""].devDependencies["@tauri-apps/cli"], CLI);
  assert.equal(lock.packages["node_modules/@tauri-apps/cli"].version, CLI);
  assert.match(TEMPLATE.slice(0, TEMPLATE.indexOf("Unicode true")), new RegExp(`@tauri-apps/cli-v${CLI.replace(/\./g, "\\.")}`));

  // Without its header and without each "; Coucou:" line and the line after it, the copy is the template itself.
  const lines = TEMPLATE.slice(TEMPLATE.indexOf("Unicode true")).split("\n");
  const upstream = lines.filter((line, i) => !line.startsWith("; Coucou:") && !lines[i - 1]?.startsWith("; Coucou:"));
  assert.equal(lines.length - upstream.length, 12, "six changes of one line each, each with its comment");
  assert.equal(createHash("sha256").update(upstream.join("\n")).digest("hex"), UPSTREAM_SHA256);
});

test("the template includes the hooks and declares the page right after the install-folder page", () => {
  assert.match(
    TEMPLATE,
    /\{\{#if installer_hooks\}\}\n; Coucou:.*\n!define COUCOU_PAGE_IN_TEMPLATE\n!include "\{\{installer_hooks\}\}"\n\{\{\/if\}\}/,
  );
  const page = "Page custom CoucouPageCreate CoucouPageLeave";
  const after = TEMPLATE.split("!insertmacro MUI_PAGE_DIRECTORY\n")[1].split("\n").filter((l) => l && !l.startsWith(";"));
  assert.equal(after[0], page);
  // Welcome, reinstall, install folder, the assistant, the files, finish.
  const order = [
    "!insertmacro MUI_PAGE_WELCOME",
    "Page custom PageReinstall PageLeaveReinstall",
    "!insertmacro MUI_PAGE_DIRECTORY",
    page,
    "!insertmacro MUI_PAGE_INSTFILES",
    "!insertmacro MUI_PAGE_FINISH",
  ].map((line) => TEMPLATE.indexOf(`\n${line}\n`));
  assert.ok(order.every((at, i) => at > 0 && (i === 0 || at > order[i - 1])), `pages out of order: ${order}`);
  // With Tauri's own template, hooks.nsh declares the page itself (it then comes first).
  assert.match(HOOKS, /!ifndef COUCOU_PAGE_IN_TEMPLATE\n\s*Page custom CoucouPageCreate CoucouPageLeave\n!endif/);
});

test("the app is AXYON, and an update still finds and replaces the one installed as Coucou", () => {
  const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
  assert.equal(conf.productName, "AXYON");
  assert.equal(conf.identifier, "fr.louisraille.coucou", "the keystore, the settings and the single instance hang on it");
  // Tauri's default upgrade code is derived from the product name: the one derived from Coucou, kept.
  assert.equal(conf.bundle.windows.wix.upgradeCode, "664695e2-338f-541b-8d41-6266d176533c");
  assert.deepEqual(conf.bundle.linux.deb.replaces, ["coucou"]);
  // The uninstall entry and the key that remembers the folder keep Coucou's names; what shows is AXYON.
  assert.match(TEMPLATE, /\n!undef UNINSTKEY\n; Coucou:.*\n!define UNINSTKEY "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Coucou"\n/);
  assert.match(TEMPLATE, /\n!undef MANUPRODUCTKEY\n; Coucou:.*\n!define MANUPRODUCTKEY "\$\{MANUKEY\}\\Coucou"\n/);
  assert.match(TEMPLATE, /WriteRegStr SHCTX "\$\{UNINSTKEY\}" "DisplayName" "\$\{PRODUCTNAME\}"/);
  // Coucou's shortcuts become AXYON's; "start with Windows" keeps its entry (lib.rs), removed with the app only.
  assert.match(HOOKS, /!insertmacro CoucouOldShortcut "\$SMPROGRAMS"\n\s*!insertmacro CoucouOldShortcut "\$DESKTOP"/);
  assert.match(HOOKS, /\$\{If\} \$UpdateMode <> 1\n\s*DeleteRegValue HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Run" "Coucou"/);
  assert.match(read("src-tauri/src/lib.rs"), /tauri_plugin_autostart::Builder::new\(\)\.app_name\("Coucou"\)/);
});

test("tauri.conf.json builds the setup.exe from the copy, with the hooks, in English, French and Portuguese", () => {
  const nsis = JSON.parse(read("src-tauri/tauri.conf.json")).bundle.windows.nsis;
  assert.equal(nsis.template, "nsis/installer.nsi");
  assert.equal(nsis.installerHooks, "nsis/hooks.nsh");
  assert.equal(nsis.installMode, "currentUser", "$APPDATA and Credential Manager are the user's");
  assert.deepEqual(nsis.languages, ["English", "French", "PortugueseBR"]);
});

// ── What the page offers ──────────────────────────────────────────────────────

test("the brains are hermes_setup.rs's, ChatGPT first, each with the keystore entry Coucou reads", () => {
  const rows = table("CoucouBrains");
  assert.deepEqual(rows.map((r) => r[0]), rows.map((_, i) => String(i)), "indexes in the dropdown's order");
  const offered = Object.fromEntries(rows.map(([, id, key]) => [id, key || null]));
  assert.deepEqual(Object.keys(offered), Object.keys(BRAINS), "the contract's order: ChatGPT first, selected by default");
  assert.deepEqual(offered, BRAINS);
  const rust = rustBrains();
  assert.deepEqual(Object.keys(rust).sort(), Object.keys(offered).sort(), "every brain of BRAINS, and only those");
  for (const [id, key] of Object.entries(offered)) assert.equal(key, rust[id], `${id}'s keystore entry`);

  // Keys go where secrets.rs looks: a known entry, under its service.
  const secrets = read("src-tauri/src/secrets.rs");
  for (const key of Object.values(offered).filter(Boolean)) assert.ok(secrets.includes(`"${key}",`), `${key} in KNOWN_KEYS`);
  assert.match(secrets, /const SERVICE: &str = "fr\.louisraille\.coucou";/);
  assert.match(fn("CoucouStoreKey"), /StrCpy \$3 "\$CoucouKeyEntry\.fr\.louisraille\.coucou"/);

  // The servers on this computer are shown at the address Coucou uses when it has none.
  for (const [, id, , address] of rows.filter(([, id]) => id === "ollama" || id === "lmstudio")) {
    assert.match(RUST, new RegExp(`"${id}" => Some\\("${address.replace(/\./g, "\\.")}"\\)`));
  }
});

test("the tones and the skill packs are the contract's, with its defaults", () => {
  const tones = table("CoucouPersonas");
  assert.deepEqual(tones.map((r) => r[1]), PERSONAS, "the five presets, friendly first (the default)");
  assert.deepEqual(PERSONAS, rustList("PERSONAS").filter((p) => p !== "custom"));
  const skills = table("CoucouSkills");
  assert.deepEqual(skills.map((r) => r[1]).sort(), [...SKILLS].sort());
  assert.deepEqual([...rustList("SKILL_PACKS")].sort(), [...SKILLS].sort());
  assert.deepEqual(Object.fromEntries(skills.map(([, id, on]) => [id, on])), { home: "1", search: "1", news: "0" });
  for (const [index] of skills) assert.match(HOOKS, new RegExp(`!define COUCOU_SKILL_AT_${index} "`), `skill ${index} placed`);
});

test("every text is in English, French and Portuguese, the contract's own words where it gives them", () => {
  const all = texts();
  assert.ok(all.length > 30);
  for (const [en, fr, pt] of all) {
    assert.ok(en && fr && pt, `a missing translation for "${en}"`);
    if (en.length > 12) assert.ok(fr !== en && pt !== en, `"${en}" is not translated`);
  }
  // $LANGUAGE picks them: 1036 French, 1046 Portuguese (Brazil), anything else English.
  assert.match(HOOKS, /!macro CoucouText VAR EN FR PT\n\s*\$\{If\} \$LANGUAGE = 1036\n\s*StrCpy \$\{VAR\} "\$\{FR\}"\n\s*\$\{ElseIf\} \$LANGUAGE = 1046\n\s*StrCpy \$\{VAR\} "\$\{PT\}"\n\s*\$\{Else\}\n\s*StrCpy \$\{VAR\} "\$\{EN\}"/);

  const english = new Map(all.map(([en, fr, pt]) => [en, { fr, pt }]));
  for (const en of [
    "Set up the assistant now (recommended)",
    "Stored in Windows Credential Manager, never in a file.",
    "After installing, AXYON opens OpenAI's page and shows a code: sign in with your ChatGPT Plus or Pro account. No key needed.",
    "Let my iPhone change the assistant and use AXYON's home",
    "When AXYON first starts it will: download AXYON Core's official installer from Nous Research (a fixed version, checked by SHA-256), install AXYON Core and its Python in your user folder, set the brain, write the personality to its SOUL.md (an existing one is backed up), and install the ticked skills.",
  ]) {
    assert.ok(english.has(en), `"${en}"`);
  }
  assert.deepEqual(english.get("Your assistant"), { fr: "Votre assistant", pt: "Seu assistente" });
  const pt = {
    "ChatGPT (your subscription)": "ChatGPT (sua assinatura)",
    "OpenAI (API key)": "OpenAI (chave de API)",
    "Anthropic Claude (API key)": "Anthropic Claude (chave de API)",
    "Google Gemini (API key)": "Google Gemini (chave de API)",
    "OpenRouter (API key)": "OpenRouter (chave de API)",
    "Ollama (on this computer)": "Ollama (neste computador)",
    "LM Studio (on this computer)": "LM Studio (neste computador)",
    Friendly: "Amigável",
    Direct: "Direto",
    Professional: "Profissional",
    Playful: "Divertido",
    "Butler (formal, witty)": "Mordomo (formal e espirituoso)",
    "AXYON's home (TVs, lights…)": "Casa do AXYON (TVs, luzes…)",
    "Free web search (DuckDuckGo)": "Busca na web grátis (DuckDuckGo)",
    "News and feeds (RSS)": "Notícias e feeds (RSS)",
  };
  for (const [en, label] of Object.entries(pt)) assert.equal(english.get(en)?.pt, label, en);
});

test("the page skips passive and update runs, and starts unticked on a reinstall", () => {
  const create = fn("CoucouPageCreate");
  assert.match(create, /\$\{GetOptions\} \$CMDLINE "\/P" \$0\n\s*\$\{IfNot\} \$\{Errors\}\n\s*Abort/);
  assert.match(create, /\$\{GetOptions\} \$CMDLINE "\/UPDATE" \$0\n\s*\$\{IfNot\} \$\{Errors\}\n\s*Abort/);
  assert.match(create, /\$\{If\} \$\{FileExists\} "\$APPDATA\\Coucou\\settings\.json"\n\s*StrCpy \$CoucouEnable 0\n\s*\$\{Else\}\n\s*StrCpy \$CoucouEnable 1/);
  assert.match(create, /StrCpy \$CoucouName "Zheus"/);
  assert.match(create, /\$\{NSD_Edit_SetTextLimit\} \$CoucouHwName 40/);
  assert.match(create, /\$\{NSD_CreatePassword\} .* \$CoucouKey\n\s*Pop \$CoucouHwKey\n\s*\$\{NSD_Edit_SetTextLimit\} \$CoucouHwKey 512/);
  // A brain with a key entry and no key typed: a message, and the page stays.
  assert.match(fn("CoucouPageLeave"), /\$\{AndIf\} \$CoucouKeyEntry != ""\n\s*\$\{AndIf\} \$CoucouKey == ""\n[\s\S]*MessageBox[^\n]*\n\s*Abort/);
});

// ── What it hands over ────────────────────────────────────────────────────────

test("setup.json has exactly the contract's fields, UTF-16LE after a BOM, and nothing secret", () => {
  const save = fn("CoucouSaveChoices");
  const literals = [...save.matchAll(/StrCpy \$1 '(\{"version":1[^']*\})'/g)].map((m) => m[1]);
  assert.equal(literals.length, 2);
  const [ticked, unticked] = literals;
  const sample = ticked
    .replace("$CoucouBrain", "chatgpt")
    .replace("$1", "Zheus")
    .replace("$CoucouPersona", "friendly")
    .replace("$2", "home,search")
    .replace("$3", "true");
  const json = JSON.parse(sample);
  assert.deepEqual(Object.keys(json), ["version", "configure", "brain", "name", "persona", "skills", "phone"]);
  assert.deepEqual(json, { version: 1, configure: true, brain: "chatgpt", name: "Zheus", persona: "friendly", skills: "home,search", phone: true });
  assert.deepEqual(JSON.parse(unticked), { version: 1, configure: false });

  assert.match(save, /Push \$CoucouName\n\s*Call CoucouJson\n\s*Pop \$1/, "the name is escaped for JSON");
  assert.match(save, /FileOpen \$0 "\$APPDATA\\Coucou\\setup\.json" w/);
  assert.deepEqual([...save.matchAll(/^\s*(FileWrite\w* .*)/gm)].map((m) => m[1]), ["FileWriteUTF16LE /BOM $0 $1"]);
  // The key is never in the file, the details list or a message; only Credential Manager gets it.
  for (const line of HOOKS.split("\n").filter((l) => /FileWrite|DetailPrint|MessageBox|CoucouText/.test(l))) {
    assert.ok(!/\$CoucouKey(?!Entry)/.test(line), line);
  }
  assert.ok(!/\$CoucouKey(?!Entry)/.test(ticked));
  assert.match(save, /Call CoucouStoreKey[\s\S]*!insertmacro CoucouForget \$CoucouKey/);
});

test("the key is written the way keyring 3.6.3 reads it, and the JSON escape looks at code units", () => {
  const lock = read("Cargo.lock");
  assert.match(lock, /name = "keyring"\nversion = "3\.6\.3"/, "another keyring: check its Windows credential layout");
  const store = fn("CoucouStoreKey");
  // CREDENTIALW, 32-bit: Flags, Type = CRED_TYPE_GENERIC, TargetName, Comment, LastWritten, BlobSize, Blob,
  // Persist = CRED_PERSIST_ENTERPRISE, AttributeCount, Attributes, TargetAlias, UserName.
  assert.ok(store.includes('System::Call "*(i 0, i 1, p r5, p 0, i 0, i 0, i r2, p r4, i 3, i 0, p 0, p 0, p r6)p.r7"'));
  assert.ok(store.includes('System::Call "advapi32::CredWriteW(p r7, i 0)i.r0 ?e"'));
  assert.match(store, /StrLen \$2 \$9\n\s*IntOp \$2 \$2 \* 2/, "the blob is the UTF-16 code units, no terminator");
  assert.match(store, /StrCpy \$8 \$CoucouKeyEntry[\s\S]*\*\(&w\$\{NSIS_MAX_STRLEN\} r8\)p\.r6/, "UserName is the entry");
  assert.match(store, /RtlZeroMemory\(p r4, i r2\)/);
  assert.match(HOOKS, /!if \$\{NSIS_PTR_SIZE\} > 4\n\s*!error/, "the layout is the 32-bit one");

  const json = fn("CoucouJson");
  assert.match(json, /\$\{If\} \$5 = 0x5C\n\s*StrCpy \$1 "\$1\\\\"/);
  assert.match(json, /\$\{ElseIf\} \$5 = 0x22\n\s*StrCpy \$1 '\$1\\"'/);
  assert.match(json, /\$\{ElseIf\} \$5 < 0x20\n\s*; a control character: dropped\n\s*\$\{ElseIf\}/);
  assert.match(json, /\$\{ElseIf\} \$5 >= 0xDC00\n\s*\$\{AndIf\} \$5 <= 0xDFFF\n\s*; the second half of a pair, alone: dropped/);
});

test("the uninstaller removes edge identity, configuration, task, and staged app files", () => {
  const uninstall = HOOKS.split("!macro NSIS_HOOK_PREUNINSTALL\n")[1].split("!macroend")[0];
  assert.match(uninstall, /schtasks\.exe[^']*\/End \/TN "Coucou Edge Worker"/);
  assert.doesNotMatch(uninstall, /taskkill\.exe[^']*\/IM/);
  assert.match(uninstall, /schtasks\.exe[^']*\/Delete \/TN "Coucou Edge Worker" \/F/);
  assert.match(uninstall, /coucou-edge-worker\.exe" --remove-identity/);
  assert.match(uninstall, /DeleteRegValue HKCU "Environment" "COUCOU_VPS_URL"/);
  assert.match(uninstall, /DeleteRegValue HKCU "Environment" "COUCOU_DEVICE_NAME"/);
  for (const path of ["edge-worker", "bin", "inbox"]) {
    assert.ok(uninstall.includes(`RMDir /r "$LOCALAPPDATA\\Coucou\\${path}"`));
  }
  assert.match(uninstall, /Delete "\$LOCALAPPDATA\\Coucou\\coucou\.log"/);
});
