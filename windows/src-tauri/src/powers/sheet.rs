// The user's Google Sheet, through gog (github.com/openclaw/gogcli), the Google Workspace command the user's assistant
// already uses with its gog skill: gog's own sign-in, kept by gog in the system's keystore, never seen here; nothing is
// ever asked (`--no-input`). AXYON reads the tab it was given, and writes only into its own tab (powers/mod.rs,
// workspace_sync): its values, raw, from stdin.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use coucou_powers::sheet::{self, SheetLink};
use serde_json::Value;

use crate::i18n::{t, tf};

/// The longest gog may take to answer.
const WAIT: Duration = Duration::from_secs(45);

/// gog's program: on the PATH, or where its installs put it.
pub fn program() -> Option<PathBuf> {
    let name = if cfg!(windows) { "gog.exe" } else { "gog" };
    let home = crate::platform::home_dir();
    let usual = [
        home.join("go").join("bin"),
        home.join(".local").join("bin"),
        home.join("AppData").join("Local").join("Programs").join("gog"),
        home.join("AppData").join("Local").join("gogcli"),
        home.join("scoop").join("shims"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/home/linuxbrew/.linuxbrew/bin"),
    ];
    let on_path: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    on_path.into_iter().chain(usual).map(|dir| dir.join(name)).find(|p| p.is_file())
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
fn run(program: &Path, args: &[String], input: Option<String>) -> Result<String, String> {
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
        if started.elapsed() > WAIT {
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
    let program = program().ok_or_else(|| t("gog was not found on this computer: install it (github.com/openclaw/gogcli) and sign in with gog auth add, as your assistant's gog skill does."))?;
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
        assert_eq!(run(&fake, &[], Some(values.to_string())).unwrap(), values);
        assert_eq!(run(&fake, &["fail".to_string()], None).unwrap_err(), "error: Unable to parse range");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
