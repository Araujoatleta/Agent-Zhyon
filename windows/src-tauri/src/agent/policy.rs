// What Mochi may do without asking. Pure rules, tested below; the tools (tools.rs) ask them before anything runs.
//
//   * Looking (listing, reading, searching) inside the working folder needs no click. Anywhere else, or at a file
//     that looks like a secret (SSH keys, .env files, browser profiles…), it waits for one.
//   * Changing a file waits for a click, unless the user chose "Edit files without asking" — and even then only
//     inside the working folder and never for a secret-looking file.
//   * Running a command, fetching or opening a web page always waits for a click, at every level.
//   * "Read only" offers no tool that changes anything, and refuses one if a model asks anyway.
//   * An action on the house (smart_home/) asks when its device does: plugs and switches by default, any device the
//     user set to ask, every device with "Ask before every action". The others act at once. Read only refuses them.

use std::path::{Component, Path, PathBuf};

/// The level the user picked in Settings → Mochi's hands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Look only: nothing is changed and no command runs.
    ReadOnly,
    /// Every change, command and web request waits for a click (the default).
    Ask,
    /// File changes inside the working folder are applied without a click; the rest still asks.
    Edits,
}

impl Level {
    pub fn parse(raw: &str) -> Level {
        match raw {
            "read" => Level::ReadOnly,
            "edits" => Level::Edits,
            _ => Level::Ask,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::ReadOnly => "read",
            Level::Ask => "ask",
            Level::Edits => "edits",
        }
    }
}

/// What a tool call does to the computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Tells about the computer (OS, shell, folder, time): nothing to protect.
    Info,
    /// Lists, reads or searches files.
    Read,
    /// Creates or changes a file.
    Write,
    /// Runs a command.
    Command,
    /// Asks a web server for a page (what is sent can carry data out).
    Fetch,
    /// Opens a page in the browser.
    Open,
    /// Acts on a device of the user's home. `ask`: the device (or the user's setting) wants a click first.
    Home { ask: bool },
}

/// What must happen before a call runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Free,
    Ask,
    /// Never at this level; the reason goes back to the model.
    Refuse(&'static str),
}

/// The rule table. `inside`: the path is in the working folder. `secret`: it looks like a secret.
pub fn gate(level: Level, effect: Effect, inside: bool, secret: bool) -> Gate {
    let mine = inside && !secret;
    match effect {
        Effect::Info => Gate::Free,
        Effect::Read if mine => Gate::Free,
        Effect::Read => Gate::Ask,
        Effect::Write | Effect::Command | Effect::Home { .. } if level == Level::ReadOnly => {
            Gate::Refuse("AXYON's hands are set to read only on this computer: nothing can be changed and no command can run. Tell the user what you would do instead.")
        }
        Effect::Home { ask: true } => Gate::Ask,
        Effect::Home { ask: false } => Gate::Free,
        Effect::Write if level == Level::Edits && mine => Gate::Free,
        Effect::Write | Effect::Command | Effect::Fetch | Effect::Open => Gate::Ask,
    }
}

/// `raw` as a path: `~` is the home folder, a relative path starts from `folder`, and `.` / `..` are worked out
/// without touching the disk (so `..` can never be used to hide where a path goes).
pub fn resolve(folder: &Path, home: &Path, raw: &str) -> Result<PathBuf, String> {
    let raw = raw.trim();
    if raw.contains('\0') {
        return Err("The path contains a NUL character.".into());
    }
    if raw.is_empty() || raw == "." {
        return Ok(normalize(folder));
    }
    let expanded = if raw == "~" {
        home.to_path_buf()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    };
    let joined = if expanded.is_absolute() { expanded } else { folder.join(expanded) };
    Ok(normalize(&joined))
}

/// `.` dropped and `..` applied, lexically.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never above the root: "C:\.." stays "C:\".
                if !matches!(out.components().next_back(), None | Some(Component::RootDir | Component::Prefix(_))) {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The path as the file system has it: the longest part of it that exists, resolved (links followed), with the
/// rest put back. None when not even the root exists.
pub fn real(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        if let Ok(found) = current.canonicalize() {
            let mut out = found;
            for part in missing.iter().rev() {
                out.push(part);
            }
            return Some(out);
        }
        missing.push(current.file_name()?.to_os_string());
        current = current.parent()?;
    }
}

/// True when `path`, links followed, is `folder` or inside it.
pub fn inside(folder: &Path, path: &Path) -> bool {
    match (folder.canonicalize(), real(path)) {
        (Ok(folder), Some(target)) => target.starts_with(&folder),
        _ => false,
    }
}

/// Folders whose files are keys, tokens or passwords.
const SECRET_DIRS: &[&str] = &[
    ".ssh", ".gnupg", ".aws", ".azure", ".kube", ".docker", ".password-store", "gcloud", ".config/gh",
    "user data", "firefox", "credentials", "protect",
];
/// Files that hold secrets whatever folder they are in.
const SECRET_FILES: &[&str] = &[
    ".git-credentials", ".netrc", "_netrc", ".npmrc", ".pypirc", ".pgpass", ".htpasswd", "id_rsa", "id_dsa",
    "id_ecdsa", "id_ed25519", "login data", "cookies", "key4.db", "logins.json", "cookies.sqlite", "local state",
    "wallet.dat", "secrets.json", "credentials.json", "token.json",
];
const SECRET_EXTENSIONS: &[&str] = &["pem", "key", "p12", "pfx", "kdbx", "keychain", "ppk", "jks", "keystore", "gpg"];

/// True for a path that looks like it holds a secret. A safety net on top of the working folder, never the only
/// thing between a model and the user's keys: such a path always waits for a click.
pub fn looks_secret(path: &Path) -> bool {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    let in_secret_dir = parts.iter().enumerate().any(|(i, part)| {
        SECRET_DIRS.iter().any(|dir| match dir.split_once('/') {
            Some((a, b)) => part == a && parts.get(i + 1).is_some_and(|next| next == b),
            None => part == dir,
        })
    });
    if in_secret_dir {
        return true;
    }
    let Some(name) = parts.last() else { return false };
    // An SSH key's public half (id_rsa.pub) asks too: it sits next to the private one.
    if SECRET_FILES.iter().any(|f| name == f || (f.starts_with("id_") && name.starts_with(&format!("{f}.")))) {
        return true;
    }
    if name == ".env" || name.starts_with(".env.") {
        return !name.ends_with(".example") && !name.ends_with(".sample") && !name.ends_with(".template");
    }
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| SECRET_EXTENSIONS.contains(&e.as_str()))
}

/// A path as people write it: Windows' `\\?\` prefix (from canonicalize) taken off.
pub fn display(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_table() {
        use Effect::*;
        use Level::*;
        // Looking at the folder is free at every level; elsewhere, or a secret, asks.
        for level in [ReadOnly, Ask, Edits] {
            assert_eq!(gate(level, Read, true, false), Gate::Free);
            assert_eq!(gate(level, Read, false, false), Gate::Ask);
            assert_eq!(gate(level, Read, true, true), Gate::Ask);
            assert_eq!(gate(level, Info, false, false), Gate::Free);
            // Commands, fetches and pages always ask, never free.
            assert_ne!(gate(level, Command, true, false), Gate::Free);
            assert_eq!(gate(level, Fetch, true, false), Gate::Ask);
            assert_eq!(gate(level, Open, true, false), Gate::Ask);
        }
        assert!(matches!(gate(ReadOnly, Write, true, false), Gate::Refuse(_)));
        assert!(matches!(gate(ReadOnly, Command, true, false), Gate::Refuse(_)));
        assert_eq!(gate(Ask, Write, true, false), Gate::Ask);
        assert_eq!(gate(Ask, Command, true, false), Gate::Ask);
        // "Edit files without asking": only in the folder, never a secret, never a command.
        assert_eq!(gate(Edits, Write, true, false), Gate::Free);
        assert_eq!(gate(Edits, Write, false, false), Gate::Ask);
        assert_eq!(gate(Edits, Write, true, true), Gate::Ask);
        assert_eq!(gate(Edits, Command, true, false), Gate::Ask);
        // The house: the device's own rule, at every level but read only.
        for level in [Ask, Edits] {
            assert_eq!(gate(level, Home { ask: true }, true, false), Gate::Ask);
            assert_eq!(gate(level, Home { ask: false }, true, false), Gate::Free);
        }
        assert!(matches!(gate(ReadOnly, Home { ask: false }, true, false), Gate::Refuse(_)));
    }

    #[test]
    fn levels_read_and_write_their_settings_value() {
        for level in [Level::ReadOnly, Level::Ask, Level::Edits] {
            assert_eq!(Level::parse(level.as_str()), level);
        }
        assert_eq!(Level::parse(""), Level::Ask);
        assert_eq!(Level::parse("anything"), Level::Ask, "an unknown value is the safe default");
    }

    #[cfg(unix)]
    #[test]
    fn paths_are_resolved_from_the_folder_and_dots_are_worked_out() {
        let folder = Path::new("/work/proj");
        let home = Path::new("/home/me");
        let r = |raw: &str| resolve(folder, home, raw).unwrap();
        assert_eq!(r("src/main.rs"), Path::new("/work/proj/src/main.rs"));
        assert_eq!(r("./src/../README.md"), Path::new("/work/proj/README.md"));
        assert_eq!(r("../other/x"), Path::new("/work/other/x"));
        assert_eq!(r("/etc/hosts"), Path::new("/etc/hosts"));
        assert_eq!(r("~/notes.txt"), Path::new("/home/me/notes.txt"));
        assert_eq!(r("~"), Path::new("/home/me"));
        assert_eq!(r(""), Path::new("/work/proj"));
        assert_eq!(r("/../../x"), Path::new("/x"));
        assert!(resolve(folder, home, "a\0b").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_are_resolved_from_the_folder() {
        let folder = Path::new(r"C:\work\proj");
        let home = Path::new(r"C:\Users\me");
        let r = |raw: &str| resolve(folder, home, raw).unwrap();
        assert_eq!(r(r"src\main.rs"), Path::new(r"C:\work\proj\src\main.rs"));
        assert_eq!(r(r"..\other"), Path::new(r"C:\work\other"));
        assert_eq!(r(r"D:\data\x.csv"), Path::new(r"D:\data\x.csv"));
        assert_eq!(r(r"~\Desktop"), Path::new(r"C:\Users\me\Desktop"));
        assert_eq!(r(r"C:\..\..\x"), Path::new(r"C:\x"));
    }

    #[test]
    fn inside_follows_links_and_accepts_files_that_do_not_exist_yet() {
        let base = std::env::temp_dir().join(format!("coucou-agent-policy-{}", std::process::id()));
        let folder = base.join("proj");
        let outside = base.join("elsewhere");
        std::fs::create_dir_all(folder.join("src")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        assert!(inside(&folder, &folder));
        assert!(inside(&folder, &folder.join("src")));
        assert!(inside(&folder, &folder.join("new/dir/file.txt")), "a file about to be created");
        assert!(!inside(&folder, &outside));
        assert!(!inside(&folder, &base.join("proj-other/file")), "a sibling with the same prefix is outside");
        assert!(!inside(&folder, &normalize(&folder.join("../elsewhere/x"))));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, folder.join("escape")).unwrap();
            assert!(!inside(&folder, &folder.join("escape/secret.txt")), "a link out of the folder is outside");
        }
        assert!(!inside(&base.join("missing-folder"), &folder));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn secrets_are_recognised_and_ordinary_files_are_not() {
        for secret in [
            "/home/me/.ssh/id_ed25519", "/home/me/.ssh/config", "/home/me/proj/.env", "/home/me/proj/.env.local",
            "/home/me/.aws/credentials", "/home/me/certs/server.pem", "/home/me/.git-credentials",
            "/home/me/.config/gh/hosts.yml", "/home/me/keys/my.KEY", "/home/me/id_rsa.pub", "/x/db.kdbx",
            "/home/me/.local/share/Google/Chrome/User Data/Default/Login Data",
        ] {
            assert!(looks_secret(Path::new(secret)), "{secret}");
        }
        for ordinary in [
            "/home/me/proj/src/main.rs", "/home/me/proj/.env.example", "/home/me/proj/README.md",
            "/home/me/proj/keyboard.ts", "/home/me/proj/environment.ts", "/home/me/proj/.envrc.md",
        ] {
            assert!(!looks_secret(Path::new(ordinary)), "{ordinary}");
        }
    }

    #[test]
    fn displayed_paths_lose_the_verbatim_prefix() {
        assert_eq!(display(Path::new(r"\\?\C:\Users\me")), r"C:\Users\me");
        assert_eq!(display(Path::new(r"\\?\UNC\server\share")), r"\\server\share");
        assert_eq!(display(Path::new("/home/me")), "/home/me");
    }
}
