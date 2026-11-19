//! The app's log: what happened while it ran, each line with the time and how long the app had
//! been running, so slow spots and problems can be found afterwards. It is kept in
//! `~/Library/Logs/SoftPresenter/SoftPresenter.log` on a Mac (a run with its own data folder,
//! such as a test, logs there instead). Past 5 MB the file is moved to `SoftPresenter.1.log`
//! and a new one begun, so it never grows without end. Nothing in it leaves the computer.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// A log larger than this is moved aside and a new one begun.
const LIMIT_BYTES: u64 = 5 * 1024 * 1024;

static FILE: OnceLock<Mutex<Option<(File, PathBuf)>>> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Where the log is kept.
fn folder() -> Option<PathBuf> {
    if let Some(data) = std::env::var_os("SOFTPRESENTER_DATA") {
        return Some(PathBuf::from(data));
    }
    if cfg!(target_os = "macos") {
        return std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/SoftPresenter"));
    }
    crate::bible_store::data_dir()
}

fn open(path: &PathBuf) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// Opens the log and says the app has started. Also writes any crash to it.
pub fn start() {
    STARTED.get_or_init(Instant::now);
    let opened = folder().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join("SoftPresenter.log");
        open(&path).map(|file| (file, path))
    });
    let _ = FILE.set(Mutex::new(opened));
    event(
        "start",
        &format!("SoftPresenter {} on {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH),
    );
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        event("crash", &info.to_string());
        before(info);
    }));
}

/// Writes one line: the clock time, the seconds since the start, what part of the app it is
/// about, and what happened.
pub fn event(area: &str, message: &str) {
    let Some(Ok(mut guard)) = FILE.get().map(Mutex::lock) else { return };
    let Some((file, path)) = guard.as_mut() else { return };
    let since = STARTED.get().map_or(0.0, |started| started.elapsed().as_secs_f64());
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let _ = writeln!(file, "{now} +{since:.1}s [{area}] {message}");
    // Too big: move it aside and begin again.
    if file.metadata().is_ok_and(|meta| meta.len() > LIMIT_BYTES) {
        let older = path.with_file_name("SoftPresenter.1.log");
        let _ = std::fs::rename(&*path, older);
        if let Some(fresh) = open(path) {
            *file = fresh;
        }
    }
}

/// Writes a problem to the log, and to the terminal for someone running the app from one.
pub fn problem(area: &str, message: &str) {
    eprintln!("{message}");
    event(area, message);
}

/// How the app's own resident memory stands, in megabytes, as the system reports it.
pub fn memory_mb() -> Option<u64> {
    let out =
        std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

/// Counts and times something done again and again, such as drawing a window, and says how it
/// went once a minute.
#[derive(Debug)]
pub struct Timing {
    what: &'static str,
    count: u64,
    total: f64,
    longest: f64,
    since: Instant,
}

impl Timing {
    pub fn new(what: &'static str) -> Timing {
        Timing { what, count: 0, total: 0.0, longest: 0.0, since: Instant::now() }
    }

    /// Counts one, which took this long.
    pub fn add(&mut self, took: std::time::Duration) {
        let ms = took.as_secs_f64() * 1000.0;
        self.count += 1;
        self.total += ms;
        self.longest = self.longest.max(ms);
    }

    /// Once a minute has passed: writes how many there were and how long they took, with
    /// anything else worth saying, and begins counting again. True when it wrote.
    pub fn report_each_minute(&mut self, area: &str, extra: impl FnOnce() -> String) -> bool {
        let elapsed = self.since.elapsed().as_secs_f64();
        if elapsed < 60.0 {
            return false;
        }
        let average = if self.count == 0 { 0.0 } else { self.total / self.count as f64 };
        let extra = extra();
        event(
            area,
            &format!(
                "{} {} in {elapsed:.0}s ({:.1}/s), {average:.1} ms each, longest {:.1} ms{}{extra}",
                self.count,
                self.what,
                self.count as f64 / elapsed,
                self.longest,
                if extra.is_empty() { "" } else { "; " }
            ),
        );
        *self = Timing::new(self.what);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timings_count_average_and_keep_the_longest() {
        let mut timing = Timing::new("frames");
        timing.add(std::time::Duration::from_millis(2));
        timing.add(std::time::Duration::from_millis(6));
        assert_eq!((timing.count, timing.total.round(), timing.longest.round()), (2, 8.0, 6.0));
        // Under a minute nothing is written and the counts stay.
        assert!(!timing.report_each_minute("test", String::new));
        assert_eq!(timing.count, 2);
    }
}
