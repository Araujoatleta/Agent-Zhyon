// The portfolio from the user's Google Sheet, read through gog (github.com/openclaw/gogcli), the Google Workspace
// command the user's assistant already uses with its gog skill: gog's own sign-in, kept by gog in the system's
// keystore, never seen here. Read only (`--readonly`, `--no-input`): AXYON never writes to the sheet.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use coucou_powers::sheet::{self, Read, SheetLink};
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

/// gog's arguments for the sheet `id`: its values as numbers, read only, never asking anything.
pub fn args(link: &SheetLink, id: &str) -> Vec<String> {
    let mut out: Vec<String> = ["sheets", "get", id, &sheet::range(&link.tab), "--json", "--render", "UNFORMATTED_VALUE", "--no-input", "--readonly"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let account = link.account.trim();
    if !account.is_empty() {
        out.push("--account".into());
        out.push(account.into());
    }
    out
}

/// What gog said when it failed: its last line that says something.
fn last_words(stderr: &str, stdout: &str) -> String {
    let said = stderr.lines().chain(stdout.lines()).map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("");
    coucou_powers::text::clip(said, 300)
}

/// Runs gog to its end (within `WAIT`): what it printed, or what it said when it failed.
fn run(program: &Path, args: &[String]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).env("NO_COLOR", "1");
    crate::platform::no_console(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| tf("gog could not start: {error}", &[("error", &e.to_string())]))?;
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

/// The sheet's positions, gog run and its JSON read.
pub async fn read(link: &SheetLink) -> Result<Read, String> {
    let id = sheet::spreadsheet_id(&link.id).ok_or_else(|| t("That is not a Google Sheets link or ID."))?;
    let program = program().ok_or_else(|| t("gog was not found on this computer: install it (github.com/openclaw/gogcli) and sign in with gog auth add, as your assistant's gog skill does."))?;
    let args = args(link, &id);
    let printed = tauri::async_runtime::spawn_blocking(move || run(&program, &args)).await.map_err(|e| e.to_string())??;
    let json: Value = serde_json::from_str(printed.trim()).map_err(|_| t("gog's answer could not be read."))?;
    sheet::positions(&sheet::values_of(&json)).map_err(|e| if e == sheet::NO_HEADER { no_header() } else { e })
}

/// The reader's own sentence, in the user's language.
fn no_header() -> String {
    t("No header with the asset and its quantity: name the columns \"Ativo\" and \"Quantidade\" (and \"Preço médio\", \"Classe\" if you like).")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gog_reads_the_tab_only_and_never_asks() {
        let link = SheetLink { id: "x".into(), tab: "Carteira".into(), account: " eu@gmail.com ".into() };
        assert_eq!(
            args(&link, "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz"),
            ["sheets", "get", "1AbCdEfGhIjKlMnOpQrStUvWxYz0123456789_-xyz", "'Carteira'!A1:Z400", "--json", "--render", "UNFORMATTED_VALUE", "--no-input", "--readonly", "--account", "eu@gmail.com"]
        );
        let first = args(&SheetLink::default(), "id");
        assert_eq!(first[3], "A1:Z400");
        assert!(!first.contains(&"--account".to_string()), "gog's own default account");
        assert_eq!(last_words("warning\n\nerror: token expired, run gog auth add\n", ""), "error: token expired, run gog auth add");
        assert_eq!(no_header(), sheet::NO_HEADER, "the same sentence, so it is translated");
    }
}
