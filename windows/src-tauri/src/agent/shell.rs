// run_command: one command in the user's shell — PowerShell on Windows (PowerShell 7 when it is installed), bash
// on Linux — with a time limit, a stop button and output that stays small.
//
// Plain threads and std::process, no async process API: the command runs on a blocking thread that polls the child
// every few milliseconds, so a cancel (the user's Stop, the phone leaving) or the time limit ends it promptly. Ending
// it means its whole tree: `taskkill /T` on Windows, the process group on Linux, so a `npm test` that started node
// does not outlive the call.
//
// Nothing here decides whether a command may run: that is policy.rs and the user's click, before this is called.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::text;

/// How much of a command's output is kept: its first and its last KEEP bytes.
const KEEP: usize = 16 * 1024;
/// How long to wait for the output to end once the command has exited (a background process may hold it open).
const DRAIN: Duration = Duration::from_millis(1500);
const POLL: Duration = Duration::from_millis(20);

/// The shell commands run in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shell {
    /// PowerShell 7 (`pwsh`), found on PATH.
    #[cfg_attr(not(windows), allow(dead_code))]
    Pwsh(PathBuf),
    /// Windows PowerShell 5.1, always there on Windows.
    #[cfg_attr(not(windows), allow(dead_code))]
    WindowsPowerShell,
    /// bash, as a login shell so the user's PATH (nvm, cargo, pyenv…) is there.
    #[cfg_attr(windows, allow(dead_code))]
    Bash(PathBuf),
    #[cfg_attr(windows, allow(dead_code))]
    Sh,
}

impl Shell {
    pub fn detect() -> Shell {
        #[cfg(windows)]
        {
            match crate::platform::find_on_path("pwsh") {
                Some(path) => Shell::Pwsh(path),
                None => Shell::WindowsPowerShell,
            }
        }
        #[cfg(not(windows))]
        {
            ["/bin/bash", "/usr/bin/bash"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
                .map_or(Shell::Sh, Shell::Bash)
        }
    }

    /// What the model is told it writes commands for.
    pub fn describe(&self) -> &'static str {
        match self {
            Shell::Pwsh(_) => "PowerShell 7 (pwsh) on Windows",
            Shell::WindowsPowerShell => "Windows PowerShell 5.1 (no `&&` or `||`: use `;` and `if ($?) { … }`)",
            Shell::Bash(_) => "bash",
            Shell::Sh => "sh",
        }
    }

    fn is_powershell(&self) -> bool {
        matches!(self, Shell::Pwsh(_) | Shell::WindowsPowerShell)
    }
}

/// What a command did.
#[derive(Debug, Clone, PartialEq)]
pub struct Ran {
    /// None when it was ended (time limit or cancel) or killed by a signal.
    pub code: Option<i32>,
    /// stdout and stderr together, cleaned (no colours) and cut to its start and end.
    pub output: String,
    pub timed_out: bool,
    pub canceled: bool,
    /// Something the command started still holds its output open (a server started in the background).
    pub left_running: bool,
}

/// The output as it comes, from both streams: the first KEEP bytes, then a window of the last KEEP.
#[derive(Default)]
struct Collector {
    head: Vec<u8>,
    tail: std::collections::VecDeque<u8>,
    dropped: usize,
}

impl Collector {
    fn push(&mut self, mut bytes: &[u8]) {
        if self.head.len() < KEEP {
            let take = bytes.len().min(KEEP - self.head.len());
            self.head.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
        }
        self.tail.extend(bytes);
        if self.tail.len() > KEEP {
            let extra = self.tail.len() - KEEP;
            self.tail.drain(..extra);
            self.dropped += extra;
        }
    }

    fn take(&mut self) -> Vec<u8> {
        let mut out = std::mem::take(&mut self.head);
        if self.dropped > 0 {
            out.extend_from_slice(format!("\n[… {} bytes left out …]\n", self.dropped).as_bytes());
        }
        out.extend(self.tail.drain(..));
        out
    }
}

/// Runs `script` in `shell` from `cwd`, for at most `timeout`, ending it early when `cancel` is set. Blocks: call it
/// from a blocking thread.
pub fn run(shell: &Shell, script: &str, cwd: &Path, timeout: Duration, cancel: &AtomicBool) -> Result<Ran, String> {
    if !cwd.is_dir() {
        return Err(format!("The folder {} does not exist.", super::policy::display(cwd)));
    }
    let script_file = if shell.is_powershell() { Some(write_script(script)?) } else { None };
    let result = run_with(shell, script, script_file.as_deref(), cwd, timeout, cancel);
    if let Some(file) = script_file {
        let _ = std::fs::remove_file(file);
    }
    result
}

fn run_with(
    shell: &Shell,
    script: &str,
    script_file: Option<&Path>,
    cwd: &Path,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Ran, String> {
    let mut cmd = match (shell, script_file) {
        (Shell::Pwsh(path), Some(file)) => powershell(Command::new(path), file),
        (Shell::WindowsPowerShell, Some(file)) => powershell(Command::new("powershell.exe"), file),
        (Shell::Bash(path), _) => {
            let mut c = Command::new(path);
            c.arg("-lc").arg(script);
            c
        }
        _ => {
            let mut c = Command::new("/bin/sh");
            c.arg("-c").arg(script);
            c
        }
    };
    cmd.current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Nothing may wait for a person at a keyboard nobody is watching, or page its output.
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.env("TERM", "dumb").env("DEBIAN_FRONTEND", "noninteractive");
        // Its own process group, so the time limit ends everything it started.
        cmd.process_group(0);
    }
    crate::platform::no_console(&mut cmd);

    let mut child = cmd.spawn().map_err(|e| format!("Could not start the shell: {e}"))?;
    let collector = Arc::new(Mutex::new(Collector::default()));
    let mut readers = Vec::new();
    for stream in [child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>), child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)]
        .into_iter()
        .flatten()
    {
        let collector = collector.clone();
        readers.push(std::thread::spawn(move || {
            let mut stream = stream;
            let mut buf = [0u8; 8192];
            loop {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => collector.lock().unwrap_or_else(|e| e.into_inner()).push(&buf[..n]),
                }
            }
        }));
    }

    let started = Instant::now();
    let (mut timed_out, mut canceled) = (false, false);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        if cancel.load(Ordering::SeqCst) {
            canceled = true;
        } else if started.elapsed() >= timeout {
            timed_out = true;
        }
        if canceled || timed_out {
            kill_tree(&mut child);
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(POLL);
    };

    // The output ends when everything holding it has exited; a server the command left in the background never
    // does, so it is not waited for.
    let deadline = Instant::now() + DRAIN;
    let mut left_running = false;
    for reader in readers {
        while !reader.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if reader.is_finished() {
            let _ = reader.join();
        } else {
            left_running = true;
        }
    }
    let bytes = collector.lock().unwrap_or_else(|e| e.into_inner()).take();
    let output = text::clean_output(&bytes);
    Ok(Ran { code: status.and_then(|s| s.code()), output, timed_out, canceled, left_running: left_running && !(timed_out || canceled) })
}

fn powershell(mut cmd: Command, file: &Path) -> Command {
    cmd.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(file);
    cmd
}

/// The command as a .ps1 file next to Coucou's other files: `-Command` and `-EncodedCommand` mangle quotes or turn
/// errors into CLIXML, a file is read as written. UTF-8 with a BOM, which Windows PowerShell 5.1 needs to read
/// anything but ASCII. The exit code is the last program's, or 1 when the last command failed.
fn write_script(script: &str) -> Result<PathBuf, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = crate::platform::local_dir().join("agent");
    crate::platform::ensure_private_dir(&dir).map_err(|e| format!("Could not prepare the command: {e}"))?;
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    let file = dir.join(format!("command-{}-{n}.ps1", std::process::id()));
    std::fs::write(&file, powershell_script(script)).map_err(|e| format!("Could not prepare the command: {e}"))?;
    Ok(file)
}

fn powershell_script(script: &str) -> Vec<u8> {
    let body = format!(
        "$ProgressPreference = 'SilentlyContinue'\r\n\
         try {{ [Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8 }} catch {{}}\r\n\
         $global:LASTEXITCODE = $null\r\n\
         {script}\r\n\
         $__coucouOk = $?; $__coucouCode = $global:LASTEXITCODE\r\n\
         if ($__coucouCode) {{ exit $__coucouCode }} elseif (-not $__coucouOk) {{ exit 1 }} else {{ exit 0 }}\r\n"
    );
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(body.as_bytes());
    bytes
}

/// Ends the command and everything it started.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut taskkill = Command::new("taskkill");
        taskkill.args(["/T", "/F", "/PID", &child.id().to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        crate::platform::no_console(&mut taskkill);
        let _ = taskkill.status();
    }
    #[cfg(target_os = "linux")]
    {
        // The group the child leads (process_group(0) above).
        if let Ok(pid) = i32::try_from(child.id()) {
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn here() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn the_collector_keeps_the_start_and_the_end() {
        let mut c = Collector::default();
        c.push(&vec![b'a'; KEEP]);
        c.push(&vec![b'b'; KEEP * 3]);
        c.push(b"END");
        let out = c.take();
        assert!(out.starts_with(&vec![b'a'; KEEP]));
        assert!(out.ends_with(b"END"));
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains(&format!("{} bytes left out", KEEP * 2 + 3)), "{}", &text[KEEP..KEEP + 60]);
        let mut small = Collector::default();
        small.push(b"hello");
        assert_eq!(small.take(), b"hello");
    }

    #[test]
    fn the_powershell_wrapper_reports_the_real_exit_code() {
        let bytes = powershell_script("npm test");
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "a BOM, for Windows PowerShell 5.1");
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        assert!(text.contains("\r\nnpm test\r\n"));
        assert!(text.contains("exit $__coucouCode"));
        assert!(text.contains("SilentlyContinue"));
    }

    #[cfg(unix)]
    #[test]
    fn a_command_runs_in_its_folder_and_says_how_it_ended() {
        let cancel = AtomicBool::new(false);
        let shell = Shell::detect();
        let ran = run(&shell, "echo hello; echo oops 1>&2; pwd", &here(), Duration::from_secs(20), &cancel).unwrap();
        assert_eq!(ran.code, Some(0));
        assert!(ran.output.contains("hello") && ran.output.contains("oops"), "{}", ran.output);
        let real_here = here().canonicalize().unwrap();
        assert!(ran.output.contains(&*real_here.to_string_lossy()) || ran.output.contains(&*here().to_string_lossy()));
        assert!(!ran.timed_out && !ran.canceled && !ran.left_running);

        let ran = run(&shell, "exit 3", &here(), Duration::from_secs(20), &cancel).unwrap();
        assert_eq!(ran.code, Some(3));
        assert!(run(&shell, "true", Path::new("/no/such/folder"), Duration::from_secs(5), &cancel).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_time_limit_ends_the_whole_tree() {
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        // A child of the shell that would outlive it if only the shell were killed.
        let ran = run(&Shell::detect(), "sleep 30 & sleep 30; echo never", &here(), Duration::from_millis(400), &cancel).unwrap();
        assert!(ran.timed_out);
        assert_eq!(ran.code, None);
        assert!(!ran.output.contains("never"));
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    }

    #[cfg(unix)]
    #[test]
    fn a_cancel_ends_the_command_at_once() {
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            flag.store(true, Ordering::SeqCst);
        });
        let started = Instant::now();
        let ran = run(&Shell::detect(), "sleep 30", &here(), Duration::from_secs(60), &cancel).unwrap();
        assert!(ran.canceled && !ran.timed_out);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_server_left_in_the_background_does_not_hold_the_answer() {
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        // setsid: out of the group, like a daemon; it keeps the output pipe open.
        let ran = run(&Shell::detect(), "echo started; (setsid sleep 5 &) ; exit 0", &here(), Duration::from_secs(20), &cancel).unwrap();
        assert_eq!(ran.code, Some(0));
        assert!(ran.output.contains("started"));
        assert!(ran.left_running);
        assert!(started.elapsed() < Duration::from_secs(4), "took {:?}", started.elapsed());
    }
}
