// The user's Google Sheet, through gog (github.com/openclaw/gogcli), the Google Workspace command the user's assistant
// already uses with its gog skill: gog's own sign-in, kept by gog in the system's keystore, never seen here; nothing is
// ever asked (`--no-input`). AXYON reads the tab it was given, and writes only into its own tab (powers/mod.rs,
// workspace_sync): its values, raw, from stdin.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use coucou_powers::sheet::{self, SheetLink};
use serde_json::Value;

use crate::i18n::{t, tf};

/// The longest gog may take to answer.
const WAIT: Duration = Duration::from_secs(45);

/// How long a found (or missing) gog is trusted before it is looked for again: the user may install it while AXYON runs.
const LOOK_AGAIN: Duration = Duration::from_secs(30);

/// Where gog runs: a program on this computer, or inside WSL (where the user's assistant, Hermes in WSL2, keeps it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Program {
    Native(PathBuf),
    Wsl,
}

/// The last look for gog: when, and what it found.
static FOUND: Mutex<Option<(Instant, Option<Program>)>> = Mutex::new(None);

/// gog's program, looked for at most every `LOOK_AGAIN` (a look may take seconds when it asks WSL: off the async
/// threads only).
pub fn program() -> Option<Program> {
    if let Some((at, program)) = FOUND.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if at.elapsed() < LOOK_AGAIN {
            return program.clone();
        }
    }
    let program = look_for_gog();
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), program.clone()));
    program
}

/// Whether gog was found, from the last look, for a view that must not wait: a look is begun on its own thread
/// when none was made yet.
pub fn known() -> bool {
    let last = FOUND.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match last {
        Some((_, program)) => program.is_some(),
        None => {
            std::thread::spawn(|| {
                let _ = program();
            });
            false
        }
    }
}

/// The names gog's program goes by: gog.exe from its Windows ZIP or `go install`, a .cmd or .bat shim from a
/// package manager.
fn names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["gog.exe", "gog.cmd", "gog.bat"]
    } else {
        &["gog"]
    }
}

/// gog on the PATH (the one the system has now, not the one AXYON started with), where its installs put it, in a
/// folder its ZIP was extracted to, and last inside WSL.
fn look_for_gog() -> Option<Program> {
    let in_dir = |dir: &Path| names().iter().map(|n| dir.join(n)).find(|p| p.is_file());
    let dirs = search_dirs();
    if let Some(found) = dirs.iter().find_map(|d| in_dir(d)) {
        return Some(Program::Native(found));
    }
    // The ZIP (gogcli_<version>_windows_amd64.zip) extracted where it was downloaded, one or two folders down.
    for root in extracted_roots() {
        for dir in gog_folders(&root) {
            if let Some(found) = in_dir(&dir).or_else(|| gog_folders(&dir).iter().find_map(|d| in_dir(d))) {
                return Some(Program::Native(found));
            }
        }
    }
    wsl_has_gog().then_some(Program::Wsl)
}

fn search_dirs() -> Vec<PathBuf> {
    let home = crate::platform::home_dir();
    let mut out = current_path();
    out.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    out.extend([
        home.join("go").join("bin"),
        home.join(".local").join("bin"),
        home.join("bin"),
        home.join("AppData").join("Local").join("Programs").join("gog"),
        home.join("AppData").join("Local").join("Programs").join("gogcli"),
        home.join("AppData").join("Local").join("gogcli"),
        home.join("AppData").join("Local").join("Microsoft").join("WinGet").join("Links"),
        home.join("AppData").join("Roaming").join("npm"),
        home.join("scoop").join("shims"),
        home.join("scoop").join("apps").join("gogcli").join("current"),
        PathBuf::from(r"C:\Program Files\gog"),
        PathBuf::from(r"C:\Program Files\gogcli"),
        PathBuf::from(r"C:\tools\gog"),
        PathBuf::from(r"C:\tools\gogcli"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
    ]);
    out
}

/// Where a downloaded ZIP is usually extracted.
fn extracted_roots() -> Vec<PathBuf> {
    let home = crate::platform::home_dir();
    let mut out = vec![home.join("Downloads"), home.join("Desktop"), home.join("Documents"), home.clone()];
    out.push(home.join("AppData").join("Local").join("Microsoft").join("WinGet").join("Packages"));
    out.push(home.join("AppData").join("Local").join("Programs"));
    out.push(PathBuf::from(r"C:\"));
    out.push(PathBuf::from(r"C:\Program Files"));
    out.push(PathBuf::from(r"C:\tools"));
    out
}

/// The folders in `root` named after gog ("gogcli_0.9.0_windows_amd64", "gog", "openclaw.gogcli_…").
fn gog_folders(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| is_gog_folder(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect()
}

fn is_gog_folder(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "gog" || name.starts_with("gogcli") || name.contains(".gogcli") || name.starts_with("gog_") || name.starts_with("gog-")
}

/// The PATH the system has now: on Windows the user's and the machine's, as the registry holds them, so a gog put on
/// the PATH after AXYON started is found without restarting it.
#[cfg(windows)]
fn current_path() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for key in [r"HKCU\Environment", r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment"] {
        let mut cmd = Command::new("reg.exe");
        cmd.args(["query", key, "/v", "Path"]).stdin(Stdio::null()).stderr(Stdio::null());
        crate::platform::no_console(&mut cmd);
        if let Ok(done) = cmd.output() {
            if let Some(value) = reg_value(&String::from_utf8_lossy(&done.stdout)) {
                out.extend(std::env::split_paths(&expand(&value)));
            }
        }
    }
    out
}

#[cfg(not(windows))]
fn current_path() -> Vec<PathBuf> {
    Vec::new()
}

/// The value of `reg query … /v Path`: "    Path    REG_EXPAND_SZ    C:\\a;%USERPROFILE%\\b".
#[cfg_attr(not(windows), allow(dead_code))]
fn reg_value(printed: &str) -> Option<String> {
    printed.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("Path").or_else(|| l.strip_prefix("PATH"))?;
        let rest = rest.trim_start();
        let rest = rest.strip_prefix("REG_EXPAND_SZ").or_else(|| rest.strip_prefix("REG_SZ"))?;
        Some(rest.trim().to_string())
    })
}

/// "%USERPROFILE%\\bin" with the variables this process knows put in.
#[cfg_attr(not(windows), allow(dead_code))]
fn expand(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The shell line gog runs with inside WSL: the usual install folders on its PATH, the arguments passed as they are.
const WSL_LINE: &str = "PATH=\"$HOME/.local/bin:$HOME/go/bin:$HOME/bin:/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:$PATH\"; exec gog \"$@\"";

/// WSL's own program, when Windows has it.
#[cfg(windows)]
fn wsl_exe() -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    Some(root.join("System32").join("wsl.exe")).filter(|p| p.is_file())
}

/// gog inside WSL's default distribution, asked once a look (WSL may take a few seconds to wake).
#[cfg(windows)]
fn wsl_has_gog() -> bool {
    let Some(wsl) = wsl_exe() else { return false };
    let probe = WSL_LINE.replace("exec gog \"$@\"", "command -v gog");
    let args: Vec<String> = ["-e", "sh", "-c", &probe].iter().map(|s| s.to_string()).collect();
    run_program(&wsl, &args, None, Duration::from_secs(10)).is_ok_and(|printed| !printed.trim().is_empty())
}

#[cfg(not(windows))]
fn wsl_has_gog() -> bool {
    false
}

/// The program and arguments that run gog with `args`.
fn command_line(program: &Program, args: &[String]) -> (PathBuf, Vec<String>) {
    match program {
        Program::Native(path) => (path.clone(), args.to_vec()),
        Program::Wsl => {
            #[cfg(windows)]
            let wsl = wsl_exe().unwrap_or_else(|| PathBuf::from("wsl.exe"));
            #[cfg(not(windows))]
            let wsl = PathBuf::from("wsl.exe");
            let mut all: Vec<String> = ["-e", "sh", "-c", WSL_LINE, "gog"].iter().map(|s| s.to_string()).collect();
            all.extend(args.iter().cloned());
            (wsl, all)
        }
    }
}

/// The tab: the one the user named, else AXYON's own.
pub fn tab_of(link: &SheetLink) -> String {
    let tab = link.tab.trim();
    if tab.is_empty() { coucou_powers::workspace::TAB.to_string() } else { tab.to_string() }
}

/// "'AXYON'!F2:P9": a range of the tab.
pub fn in_tab(tab: &str, cells: &str) -> String {
    format!("'{}'!{cells}", tab.replace('\'', "''"))
}

fn with_account(mut out: Vec<String>, link: &SheetLink) -> Vec<String> {
    let account = link.account.trim();
    if !account.is_empty() {
        out.push("--account".into());
        out.push(account.into());
    }
    out
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// gog's arguments to read the tab: its values as numbers, read only.
pub fn args(link: &SheetLink, id: &str) -> Vec<String> {
    with_account(strings(&["sheets", "get", id, &in_tab(&tab_of(link), "A1:Z400"), "--json", "--render", "UNFORMATTED_VALUE", "--no-input", "--readonly"]), link)
}

/// gog's arguments to write `range` with the values given on stdin, raw (a text is never taken for a formula).
pub fn update_args(link: &SheetLink, id: &str, range: &str) -> Vec<String> {
    with_account(strings(&["sheets", "update", id, range, "--values-json", "@-", "--input", "RAW", "--no-input", "--force"]), link)
}

/// gog's arguments to add the tab.
pub fn add_tab_args(link: &SheetLink, id: &str, tab: &str) -> Vec<String> {
    with_account(strings(&["sheets", "add-tab", id, tab, "--no-input"]), link)
}

/// What gog said when it failed: its last line that says something.
fn last_words(stderr: &str, stdout: &str) -> String {
    let said = stderr.lines().chain(stdout.lines()).map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("");
    coucou_powers::text::clip(said, 300)
}

/// Runs gog to its end (within `WAIT`), `input` on its stdin: what it printed, or what it said when it failed.
fn run(program: &Program, args: &[String], input: Option<String>) -> Result<String, String> {
    let (path, all) = command_line(program, args);
    run_program(&path, &all, input, WAIT)
}

fn run_program(program: &Path, args: &[String], input: Option<String>, wait: Duration) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1");
    crate::platform::no_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| tf("gog could not start: {error}", &[("error", &e.to_string())]))?;
    // Written beside the reading, so neither waits on the other; closed when written (gog reads to its end).
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = stdin.write_all(text.as_bytes());
        });
    }
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let stdout = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let stderr = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() > wait {
            let _ = child.kill();
            let _ = child.wait();
            return Err(t("gog did not answer in time."));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let (stdout, stderr) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    if status.success() {
        Ok(stdout)
    } else {
        Err(last_words(&stderr, &stdout))
    }
}

/// gog with `args` (and `input`), off the async threads.
async fn gog(args: Vec<String>, input: Option<String>) -> Result<String, String> {
    let program = tauri::async_runtime::spawn_blocking(program)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| t("gog was not found on this computer, on its PATH, in Downloads or in WSL: put gog.exe (from gogcli's Windows ZIP, github.com/openclaw/gogcli) on the PATH and sign in with gog auth add, as your assistant's gog skill does."))?;
    tauri::async_runtime::spawn_blocking(move || run(&program, &args, input)).await.map_err(|e| e.to_string())?
}

/// The spreadsheet's ID, from the link the user gave.
pub fn id_of(link: &SheetLink) -> Result<String, String> {
    sheet::spreadsheet_id(&link.id).ok_or_else(|| t("That is not a Google Sheets link or ID."))
}

/// What reading the tab found.
pub enum Got {
    Rows(Vec<Vec<Value>>),
    /// The spreadsheet has no tab of that name.
    NoTab,
}

/// The tab's cells (its first 400 rows and 26 columns), as numbers where they are numbers.
pub async fn get(link: &SheetLink) -> Result<Got, String> {
    let id = id_of(link)?;
    match gog(args(link, &id), None).await {
        Ok(printed) => {
            let json: Value = serde_json::from_str(printed.trim()).map_err(|_| t("gog's answer could not be read."))?;
            Ok(Got::Rows(sheet::values_of(&json)))
        }
        // Google says so of a range in a tab that is not there.
        Err(e) if e.to_lowercase().contains("unable to parse range") => Ok(Got::NoTab),
        Err(e) => Err(e),
    }
}

/// Adds the tab to the spreadsheet.
pub async fn add_tab(link: &SheetLink, tab: &str) -> Result<(), String> {
    let id = id_of(link)?;
    gog(add_tab_args(link, &id, tab), None).await.map(|_| ())
}

/// Writes `values` into `range` of the spreadsheet, raw.
pub async fn update(link: &SheetLink, range: &str, values: &[Vec<Value>]) -> Result<(), String> {
    let id = id_of(link)?;
    let body = serde_json::to_string(values).map_err(|e| e.to_string())?;
    gog(update_args(link, &id, range), Some(body)).await.map(|_| ())
}

/// The sheet's positions from its rows, the reader's own sentence in the user's language.
pub fn positions(rows: &[Vec<Value>]) -> Result<sheet::Read, String> {
    sheet::positions(rows).map_err(|e| if e == sheet::NO_HEADER { no_header() } else { e })
}

/// The reader's own sentence, in the user's language.
fn no_header() -> String {
    t("No header with the asset and its quantity: name the columns \"Ativo\" and \"Quantidade\" (and \"Preço médio\", \"Classe\" if you like).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gog_reads_the_tab_writes_raw_from_stdin_and_never_asks() {
        let id = "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz";
        let link = SheetLink { id: "x".into(), tab: "Carteira".into(), account: " eu@gmail.com ".into() };
        assert_eq!(
            args(&link, id),
            ["sheets", "get", id, "'Carteira'!A1:Z400", "--json", "--render", "UNFORMATTED_VALUE", "--no-input", "--readonly", "--account", "eu@gmail.com"]
        );
        let ours = SheetLink { id: "x".into(), ..SheetLink::default() };
        assert_eq!(tab_of(&ours), "AXYON", "no tab named: AXYON's own");
        assert_eq!(args(&ours, id)[3], "'AXYON'!A1:Z400");
        assert!(!args(&ours, id).contains(&"--account".to_string()), "gog's own default account");
        assert_eq!(update_args(&ours, id, "'AXYON'!F2:P9"), ["sheets", "update", id, "'AXYON'!F2:P9", "--values-json", "@-", "--input", "RAW", "--no-input", "--force"]);
        assert_eq!(add_tab_args(&ours, id, "AXYON"), ["sheets", "add-tab", id, "AXYON", "--no-input"]);
        assert_eq!(in_tab("Ana's", "A1"), "'Ana''s'!A1");
        assert_eq!(last_words("warning\n\nerror: token expired, run gog auth add\n", ""), "error: token expired, run gog auth add");
        assert_eq!(no_header(), sheet::NO_HEADER, "the same sentence, so it is translated");
    }

    #[test]
    fn gog_is_found_where_windows_users_put_it() {
        assert_eq!(reg_value("\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    C:\\a;%USERPROFILE%\\b\r\n").as_deref(), Some("C:\\a;%USERPROFILE%\\b"));
        assert_eq!(reg_value("    Path    REG_SZ    C:\\x").as_deref(), Some("C:\\x"));
        assert_eq!(reg_value("ERROR: not found"), None);
        std::env::set_var("AXYON_TEST_DIR", "/d");
        assert_eq!(expand("%AXYON_TEST_DIR%/bin;%NOT_SET_AT_ALL%/x;50%"), "/d/bin;%NOT_SET_AT_ALL%/x;50%");
        for name in ["gogcli_0.9.0_windows_amd64", "gog", "GogCLI", "openclaw.gogcli_Microsoft.Winget.Source_8wekyb3d8bbwe"] {
            assert!(is_gog_folder(name), "{name}");
        }
        for name in ["google", "goggles", "Documents"] {
            assert!(!is_gog_folder(name), "{name}");
        }
        let args = vec!["sheets".to_string(), "get".to_string()];
        let (wsl, all) = command_line(&Program::Wsl, &args);
        assert!(wsl.to_string_lossy().to_ascii_lowercase().ends_with("wsl.exe"));
        assert_eq!(&all[..2], ["-e", "sh"]);
        assert_eq!(&all[4..], ["gog", "sheets", "get"], "$0, then gog's own arguments as they are");
        assert!(all[3].ends_with("exec gog \"$@\""));
        assert_eq!(command_line(&Program::Native(PathBuf::from("gog")), &args), (PathBuf::from("gog"), args.clone()));
    }

    /// The values go to gog on its stdin (`--values-json @-`), whole; its answer comes back, or its last words.
    #[cfg(unix)]
    #[test]
    fn gog_gets_the_values_on_its_stdin() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("axyon-gog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("gog");
        std::fs::write(&fake, "#!/bin/sh\nif [ \"$1\" = fail ]; then echo 'error: Unable to parse range' >&2; exit 1; fi\ncat\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let values = r#"[["GGRC11","FII",150,9.2,9.5]]"#;
        assert_eq!(run(&Program::Native(fake.clone()), &[], Some(values.to_string())).unwrap(), values);
        assert_eq!(run(&Program::Native(fake.clone()), &["fail".to_string()], None).unwrap_err(), "error: Unable to parse range");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
