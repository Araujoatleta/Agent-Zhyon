// What a panic leaves behind. A panic in a background task ends that task, not Coucou (the release profile unwinds),
// and in either case it is written down first: in coucou.log, with the thread and the line it happened at, and in
// crash.txt, which the next start reads and logs once. "Coucou closed by itself" then has a cause to look at.

use crate::{log, platform, settings};

const FILE: &str = "crash.txt";

/// Called first thing at start: the previous run's panic, if there was one, then the hook for this run's.
pub fn install() {
    if let Some(previous) = take_previous() {
        log::line(format!("the previous run stopped on a panic: {previous}"));
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(no message)".into());
        let place = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();
        let text = describe(&thread, &what, &place);
        log::line(format!("PANIC {text}"));
        let t = platform::local_time();
        let stamp = format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", t.year, t.month, t.day, t.hour, t.minute, t.second);
        let _ = std::fs::write(settings::local_dir().join(FILE), format!("{stamp} {text}\n"));
        default(info);
    }));
}

fn describe(thread: &str, what: &str, place: &str) -> String {
    let what: String = what.chars().take(500).collect();
    if place.is_empty() {
        format!("in thread {thread}: {what}")
    } else {
        format!("in thread {thread} at {place}: {what}")
    }
}

/// The last panic written down, once: the file goes after it is read.
fn take_previous() -> Option<String> {
    let path = settings::local_dir().join(FILE);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::describe;

    #[test]
    fn a_panic_is_told_with_its_thread_and_line() {
        assert_eq!(describe("tokio-runtime-worker", "index out of bounds", "powers/src/x.rs:12:5"), "in thread tokio-runtime-worker at powers/src/x.rs:12:5: index out of bounds");
        assert_eq!(describe("main", "boom", ""), "in thread main: boom");
        assert_eq!(describe("main", &"x".repeat(900), "").len(), "in thread main: ".len() + 500, "a long message is cut");
    }
}
