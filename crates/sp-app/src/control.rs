//! Controlling a running SoftPresenter from outside its window: `softpresenter ctl …`.
//!
//! While it runs, the app listens on a socket file that only the user it runs as can reach (it
//! sits in a folder only that user may enter). The command line sends one request, a line of
//! JSON with the words typed, and gets one reply. Each command is carried out on the app's
//! main thread by the same code the window's buttons use, so the two can never disagree.
//!
//! Who can use it: programs running as the same user, who could change the app's files anyway.
//! Nothing listens on the network, so no web page or other computer can reach it.

#[cfg(unix)]
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sp_engine::{Command, Layer, SlideRef};
use winit::event_loop::ActiveEventLoop;

/// Changes when requests or replies change shape, so an older command line is told so.
pub const PROTOCOL: u32 = 1;

/// The most a request may be; a longer one is refused.
const MAX_REQUEST: u64 = 64 * 1024;

/// One request: the words typed after `ctl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub args: Vec<String>,
}

/// One reply: whether it worked, what to tell the person, and the same as data for programs.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub text: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub data: serde_json::Value,
}

impl Reply {
    pub(crate) fn done(text: impl Into<String>) -> Reply {
        Reply { ok: true, text: text.into(), data: serde_json::Value::Null }
    }

    pub(crate) fn with(text: impl Into<String>, data: serde_json::Value) -> Reply {
        Reply { ok: true, text: text.into(), data }
    }

    fn failed(text: impl Into<String>) -> Reply {
        Reply { ok: false, text: text.into(), data: serde_json::Value::Null }
    }
}

/// A request that has come in, with where its reply goes.
pub struct Incoming {
    pub request: Request,
    pub reply: Sender<Reply>,
}

/// Where the socket is: `SOFTPRESENTER_CONTROL_SOCKET` if set (for tests, whose folders have
/// paths too long for a socket); else, on a Mac, in ~/Library/Caches/SoftPresenter/control,
/// outside the library in Documents (which macOS guards, and may sync), so `sp` reaches the app
/// from any terminal or over ssh; else the `control` folder in the app's data folder (as for a
/// test copy with a data folder of its own).
pub fn socket_path() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(path) = var("SOFTPRESENTER_CONTROL_SOCKET") {
        return Some(path);
    }
    if cfg!(target_os = "macos")
        && var("SOFTPRESENTER_DATA").is_none()
        && let Some(home) = var("HOME")
    {
        return Some(home.join("Library/Caches/SoftPresenter/control/ctl.sock"));
    }
    crate::bible_store::data_dir().map(|dir| dir.join("control").join("ctl.sock"))
}

/// The socket file while the app listens; dropping it removes the file.
pub struct Listening {
    path: PathBuf,
}

impl Drop for Listening {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Starts listening, on a thread of its own. Each request is handed to `deliver`, which wakes
/// the app's main thread with it; the reply is written back when the main thread sends it.
#[cfg(unix)]
pub fn listen(deliver: impl Fn(Incoming) + Send + 'static) -> Result<Listening, String> {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};

    let path = socket_path().ok_or("Could not find where to keep the control socket.")?;
    // A socket path must be short (about 104 bytes on macOS).
    if path.as_os_str().len() > 100 {
        return Err(format!("The control socket's path is too long: {}", path.display()));
    }
    let folder = path.parent().ok_or("The control socket has no folder.")?;
    if !folder.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(folder)
            .map_err(|e| format!("Could not make {}: {e}", folder.display()))?;
    }
    // Only this user may enter the folder. It must be a real folder, not a link to somewhere
    // else; and setting its permissions fails unless this user owns it.
    let meta = std::fs::symlink_metadata(folder).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(format!("{} is not a folder.", folder.display()));
    }
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("Could not keep {} private: {e}", folder.display()))?;
    // A socket left by an app that stopped is removed; one another running copy is listening on
    // is left alone.
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if !meta.file_type().is_socket() {
            return Err(format!("{} is in the way of the control socket.", path.display()));
        }
        if UnixStream::connect(&path).is_ok() {
            return Err("Another SoftPresenter is already listening for commands.".to_owned());
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = UnixListener::bind(&path).map_err(|e| format!("Could not listen at {}: {e}", path.display()))?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("control".to_owned())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                answer(stream, &deliver);
            }
        })
        .map_err(|e| format!("Could not start listening for commands: {e}"))?;
    Ok(Listening { path })
}

#[cfg(not(unix))]
pub fn listen(_deliver: impl Fn(Incoming) + Send + 'static) -> Result<Listening, String> {
    Err("Controlling SoftPresenter from the command line works on macOS and Linux so far.".to_owned())
}

/// Reads one request from a connection, hands it to the app and writes back its reply.
#[cfg(unix)]
fn answer(stream: std::os::unix::net::UnixStream, deliver: &impl Fn(Incoming)) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    let read = BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line);
    let too_long = line.len() as u64 >= MAX_REQUEST && !line.ends_with('\n');
    let reply = match read.map_err(|e| e.to_string()).and_then(|_| {
        if too_long {
            return Err(format!("A command may be at most {} KB.", MAX_REQUEST / 1024));
        }
        serde_json::from_str::<Request>(line.trim())
            .map_err(|e| format!("Not a request SoftPresenter understands: {e}"))
    }) {
        Err(problem) => Reply::failed(problem),
        Ok(request) if request.v != PROTOCOL => Reply::failed(format!(
            "This command line speaks version {} of the protocol, the running app version {PROTOCOL}. Use the same build for both.",
            request.v
        )),
        Ok(request) => {
            let (tx, rx) = std::sync::mpsc::channel();
            deliver(Incoming { request, reply: tx });
            rx.recv_timeout(Duration::from_secs(15))
                .unwrap_or_else(|_| Reply::failed("SoftPresenter did not answer in time."))
        }
    };
    let mut text = serde_json::to_string(&reply).unwrap_or_default();
    text.push('\n');
    let _ = (&stream).write_all(text.as_bytes());
}

/// The command line: sends the words to the running app and prints its reply. `--json` prints
/// the reply as JSON. Returns the process's exit code.
pub fn run_client(args: Vec<String>) -> std::process::ExitCode {
    let json = args.iter().any(|a| a == "--json");
    let wait = args.iter().any(|a| a == "--wait");
    let args = with_full_paths(args.into_iter().filter(|a| a != "--json" && a != "--wait").collect());
    // Help needs no app: it is printed here.
    let reply = if args.first().is_none_or(|w| w.eq_ignore_ascii_case("help")) {
        Reply::done(HELP.trim_end())
    } else {
        send(&args).unwrap_or_else(Reply::failed)
    };
    // `--wait`: a job the app does in the background (a song pack with videos) is waited for, and
    // its own reply printed.
    let reply = match reply.data.get("job").and_then(serde_json::Value::as_u64) {
        Some(job) if wait && reply.ok => loop {
            std::thread::sleep(Duration::from_millis(500));
            match send(&["job".to_owned(), job.to_string()]) {
                Ok(now) if now.data.get("done").and_then(serde_json::Value::as_bool) == Some(false) => {}
                Ok(now) => break now,
                Err(problem) => break Reply::failed(problem),
            }
        },
        _ => reply,
    };
    // A command that did what it was asked always says so.
    let reply =
        if reply.ok && reply.text.trim().is_empty() { Reply { text: "Done.".to_owned(), ..reply } } else { reply };
    if json {
        println!("{}", serde_json::to_string_pretty(&reply).unwrap_or_default());
    } else if reply.ok {
        println!("{}", reply.text);
    } else {
        eprintln!("{}", reply.text);
    }
    if reply.ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::from(1) }
}

/// `sp` with nothing after it: what is on screen, when the app is running; otherwise it starts
/// the app (`bundle`), saying so. Either way it says where the commands are listed. A test
/// setup (its own SOFTPRESENTER_CONTROL_SOCKET) is never made to start the app.
#[cfg(target_os = "macos")]
pub fn bare(bundle: &Path) -> std::process::ExitCode {
    const MORE: &str = "sp help lists the commands.";
    let answers = socket_path().is_some_and(|path| std::os::unix::net::UnixStream::connect(path).is_ok());
    if answers {
        return match send(&["status".to_owned()]) {
            Ok(reply) if reply.ok => {
                println!("{}\n\n{MORE}", reply.text);
                std::process::ExitCode::SUCCESS
            }
            Ok(reply) => {
                eprintln!("{}", reply.text);
                std::process::ExitCode::from(1)
            }
            Err(problem) => {
                eprintln!("{problem}");
                std::process::ExitCode::from(1)
            }
        };
    }
    if let Some(own) = std::env::var_os("SOFTPRESENTER_CONTROL_SOCKET").filter(|p| !p.is_empty()) {
        eprintln!("Nothing answers at {} (SOFTPRESENTER_CONTROL_SOCKET). {MORE}", Path::new(&own).display());
        return std::process::ExitCode::from(1);
    }
    println!("SoftPresenter isn't running; starting it. {MORE}");
    match std::process::Command::new("/usr/bin/open").arg(bundle).status() {
        Ok(status) if status.success() => std::process::ExitCode::SUCCESS,
        _ => {
            eprintln!("Could not start {}.", bundle.display());
            std::process::ExitCode::from(1)
        }
    }
}

/// The app doesn't share the terminal's folder, so a file or folder named in `song import` or
/// `song new` is sent as a full path. Only those commands: elsewhere a word that happens to match a file
/// in the folder (a song called "notes") is meant as words.
fn with_full_paths(args: Vec<String>) -> Vec<String> {
    let second = |words: &[&str]| args.get(1).is_some_and(|w| words.iter().any(|word| w.eq_ignore_ascii_case(word)));
    let first = |word: &str| args.first().is_some_and(|w| w.eq_ignore_ascii_case(word));
    // `song export … --to FILE`: the file isn't there yet, so it is made whole from the
    // terminal's folder.
    if first("song") && second(&["export"]) {
        let mut args = args;
        if let Some(at) = args.iter().position(|w| w == "--to")
            && let Some(word) = args.get(at + 1)
            && Path::new(word).is_relative()
            && let Ok(here) = std::env::current_dir()
        {
            args[at + 1] = here.join(word).to_string_lossy().into_owned();
        }
        return args;
    }
    let names_files = (first("song") && second(&["import", "new", "text"])) || (first("media") && second(&["add"]));
    if !names_files {
        return args;
    }
    args.into_iter()
        .enumerate()
        .map(|(at, word)| match Path::new(&word) {
            path if at >= 2 && path.exists() => {
                path.canonicalize().map_or(word.clone(), |full| full.to_string_lossy().into_owned())
            }
            _ => word,
        })
        .collect()
}

#[cfg(unix)]
fn send(args: &[String]) -> Result<Reply, String> {
    let probe = serde_json::to_string(&Request { v: PROTOCOL, args: args.to_vec() }).map_err(|e| e.to_string())?;
    if probe.len() as u64 >= MAX_REQUEST {
        return Err(format!(
            "That command is {} KB; one may be at most {} KB. Give long words in a file (song text SONG FILE).",
            probe.len() / 1024,
            MAX_REQUEST / 1024
        ));
    }
    let path = socket_path().ok_or("Could not find where the control socket is.")?;
    let mut stream = std::os::unix::net::UnixStream::connect(&path).map_err(|_| {
        "SoftPresenter isn't running (or isn't listening for commands). Open it, then try again.".to_owned()
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(20)));
    let mut request =
        serde_json::to_string(&Request { v: PROTOCOL, args: args.to_vec() }).map_err(|e| e.to_string())?;
    request.push('\n');
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| format!("Could not read SoftPresenter's answer: {e}"))
}

#[cfg(not(unix))]
fn send(_args: &[String]) -> Result<Reply, String> {
    Err("Controlling SoftPresenter from the command line works on macOS and Linux so far.".to_owned())
}

/// The first words of the commands, so `softpresenter next` can be typed without `ctl`.
const COMMANDS: &[&str] = &[
    "help",
    "status",
    "songs",
    "slides",
    "slide",
    "playlists",
    "libraries",
    "library",
    "playlist",
    "media",
    "media-playlist",
    "themes",
    "screens",
    "timers",
    "messages",
    "props",
    "macros",
    "audio",
    "next",
    "previous",
    "prev",
    "back",
    "first",
    "last",
    "go",
    "select",
    "clear",
    "audience",
    "stage",
    "identify",
    "view",
    "message",
    "prop",
    "timer",
    "macro",
    "announce",
    "bible",
    "song",
    "undo",
    "redo",
    "job",
    "search",
    "menu",
];

/// True when `word` starts a command.
pub fn is_command(word: &str) -> bool {
    COMMANDS.contains(&word.to_lowercase().as_str())
}

/// The names the command is installed as: the short one to type, and the full one.
pub const COMMAND_NAMES: [&str; 2] = ["sp", "softpresenter"];

/// Puts the `sp` (and `softpresenter`) command on the PATH: links to this program in the
/// first of these folders that is there and can be written to without an administrator:
/// `~/.local/bin`, then `/opt/homebrew/bin`, then `/usr/local/bin`; failing those,
/// `~/.local/bin` is made. A link already there is replaced; a file of that name that is not a
/// link is left alone. Returns where the short one is.
#[cfg(unix)]
pub fn install_command() -> Result<PathBuf, String> {
    let program = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|e| format!("Could not find this program: {e}"))?;
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or("Could not find the home folder.")?;
    let local = home.join(".local/bin");
    let writable = |dir: &Path| {
        std::fs::metadata(dir).is_ok_and(|meta| meta.is_dir())
            && std::fs::File::create(dir.join(".softpresenter-check")).is_ok_and(|_| {
                let _ = std::fs::remove_file(dir.join(".softpresenter-check"));
                true
            })
    };
    let folder = [local.clone(), PathBuf::from("/opt/homebrew/bin"), PathBuf::from("/usr/local/bin")]
        .into_iter()
        .find(|dir| writable(dir))
        .map_or_else(
            || {
                std::fs::create_dir_all(&local)
                    .map(|()| local.clone())
                    .map_err(|e| format!("Could not make {}: {e}", local.display()))
            },
            Ok,
        )?;
    for name in COMMAND_NAMES {
        let link = folder.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&link) {
            if !meta.file_type().is_symlink() {
                return Err(format!("There is already a file at {}; it was left alone.", link.display()));
            }
            std::fs::remove_file(&link).map_err(|e| format!("Could not replace {}: {e}", link.display()))?;
        }
        std::os::unix::fs::symlink(&program, &link).map_err(|e| format!("Could not make {}: {e}", link.display()))?;
    }
    let link = folder.join(COMMAND_NAMES[0]);
    let on_path = std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir == folder));
    if !on_path && folder == local {
        return Err(format!(
            "Installed {}, but ~/.local/bin may not be on your PATH. Add this line to ~/.zshrc: export PATH=\"$HOME/.local/bin:$PATH\"",
            link.display()
        ));
    }
    Ok(link)
}

#[cfg(not(unix))]
pub fn install_command() -> Result<PathBuf, String> {
    Err("Installing the command line tool works on macOS and Linux so far.".to_owned())
}

/// The app bundle this program is in, when it was started through a link from outside it (the
/// installed `softpresenter` command): opening the bundle names and places the app properly.
#[cfg(target_os = "macos")]
pub fn bundle_opened_through_link() -> Option<PathBuf> {
    let started_as = PathBuf::from(std::env::args_os().next()?);
    let link = std::fs::symlink_metadata(&started_as).ok().filter(|meta| meta.file_type().is_symlink());
    let started_as = match link {
        Some(_) => started_as,
        // Started by name: find it on the PATH.
        None if started_as.components().count() == 1 => {
            let path = std::env::var_os("PATH")?;
            std::env::split_paths(&path).map(|dir| dir.join(&started_as)).find(|candidate| {
                std::fs::symlink_metadata(candidate).is_ok_and(|meta| meta.file_type().is_symlink())
            })?
        }
        None => return None,
    };
    let program = std::fs::canonicalize(&started_as).ok()?;
    let bundle = program.ancestors().find(|dir| dir.extension().is_some_and(|ext| ext == "app"))?;
    Some(bundle.to_path_buf())
}

/// What `ctl help` says.
pub const HELP: &str = "\
sp COMMAND …   (the same as softpresenter COMMAND …; add --json for the reply as JSON)
sp             what is on screen; starts SoftPresenter first if it isn't running

What is going on
  status                       what is on screen, what is next, what is playing
  songs | slides SONG | playlists | playlist [NAME] | media | themes | screens
  timers | messages | props | macros | audio | menu

Running the show
  next | previous (or back) | first | last
  go SONG [SLIDE]              put a slide on screen (SONG by name or number; SLIDE from 1)
  select SONG                  show a song's slides without putting anything on screen
  clear [all|slide|media|props|messages|audio|announcements]
  audience on|off              stage on|off           identify on|off
  view show|edit|bible|themes

Layers and controls
  media show ITEM | media add FILE… | media delete ITEM --yes
  media-playlist play NAME | media-playlist stop
  message show NAME | message hide
  prop show NAME | prop hide NAME
  timer start|stop|reset NAME
  macro run NAME
  announce SONG | announce stop
  audio play TRACK | audio pause | audio stop | audio volume 0-100
  bible REFERENCE [--show]     look a passage up (and put its first verse on screen)
  bible REFERENCE --add [to PLAYLIST]
                               look it up and put it in a playlist, with the page's rows
                               and options, as Add to does

Songs, libraries and playlists
  song new TITLE [FILE.txt]    a new song, its words from a song text file
  song import FILE… | FOLDER…  import songs (text, lyric, ChordPro, rich text, Word, .pro); a
                               folder becomes a library of its name
  libraries                    the libraries, with how many songs each has
  library NAME                 list a library's songs
  library open NAME | all      show a library's songs, or all songs
  song delete SONG --yes
  playlist open NAME
  playlist add SONG [to PLAYLIST]
  playlist new NAME | playlist rename OLD to NEW | playlist delete NAME --yes
  playlist remove ITEM [from PLAYLIST]          (ITEM by number or title)
  playlist move ITEM to N [in PLAYLIST]
  playlist header TEXT [at N] [in PLAYLIST]
  library new NAME | library rename OLD to NEW | library delete NAME --yes (songs stay)
  library add SONG… to LIBRARY | library remove SONG… from LIBRARY
  song rename SONG to TITLE | song duplicate SONG [as TITLE]
  song theme SONG THEME|main | song transition SONG KIND [SECONDS] | none
  song info SONG [artist=… author=… publisher=… year=… ccli=…]
  search WORDS                 songs with those words in their title or slides
  song text SONG [FILE]        the song as song text, or its words replaced from a file (its
                               slides keep their looks, labels and notes)
  song reload [SONG [--now]]   read songs again from their files, if changed outside the app
                               (all of them, and songs put in the Songs folder, with no SONG);
                               one on screen or being edited waits unless --now
  song keep SONG               save SoftPresenter's version over a file changed outside it
  song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]
                               a song pack of the songs (and the playlist), with the themes
                               they set, their pictures and videos, to open on another computer
  song import FILE.sppack [--skip|--replace|--keep-both]
                               a song pack's songs; ones already here are skipped unless told
                               (a pack with videos goes on in the background: add --wait to
                               wait for it, or ask with job N)

Changing a slide (each change is saved, can be undone, and shows on screen at once)
  slide SONG N [--json]        the slide's objects: words, place and look (* = the slide's own)
  slide text SONG N [OBJECT] WORDS
                               an object's words (\\n starts a new line); OBJECT is a number,
                               name or role, the main text if left out
  slide style SONG N|all [OBJECT] key=value…
                               an object's look: size=90 font=… weight=bold italic=on
                               color=#ffffff align=center …; key=reset, or reset alone, for the
                               theme's look; `slide style keys` lists them all
  slide frame SONG N|all OBJECT x= y= w= h=
                               where an object sits, in the slide's pixels (reset: the theme's)
  slide add SONG [after N] [group NAME] WORDS   a new slide (at the end unless after N)
  slide delete SONG N --yes | slide duplicate SONG N | slide move SONG N to M
  slide group SONG N NAME      slide N starts a group of that name (or renames its group)
  slide label SONG N LABEL|none | slide notes SONG N TEXT|none
  slide enable SONG N | slide disable SONG N     (a disabled slide is passed over)
  slide hotkey SONG N KEY|none | slide transition SONG N KIND [SECONDS] | none
  slide template SONG N TEMPLATE|auto | slide media SONG N MEDIA-ITEM|none

  Put a song name with spaces in quotes: slide style \"Blessed Assurance\" all size=90

Undo
  undo                         undo the last change to a song, whatever the window shows
  redo                         do it again

Everything on the menu bar
  menu                         list the menu items
  menu ITEM                    choose one, as a click on it does

Names can be typed in part (\"amazing\" finds \"Amazing Grace\"); a name that fits several
things lists them instead of guessing. Numbers are as the lists show them, from 1.
";

/// A thing found by what was typed: its place in the list.
pub(crate) fn find(kind: &str, names: &[String], typed: &str) -> Result<usize, String> {
    let wanted = typed.trim().to_lowercase();
    if wanted.is_empty() {
        return Err(format!("Which {kind}?"));
    }
    let exact: Vec<usize> = (0..names.len()).filter(|at| names[*at].to_lowercase() == wanted).collect();
    match exact.as_slice() {
        [one] => return Ok(*one),
        [] => {}
        many => {
            return Err(format!(
                "Several {kind}s are called \u{201c}{typed}\u{201d}: {}. Give its number instead.",
                many.iter().map(|at| format!("{} ({})", names[*at], at + 1)).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if let Ok(number) = wanted.parse::<usize>()
        && (1..=names.len()).contains(&number)
    {
        return Ok(number - 1);
    }
    let close: Vec<usize> = (0..names.len()).filter(|at| names[*at].to_lowercase().contains(&wanted)).collect();
    match close.as_slice() {
        [one] => Ok(*one),
        [] => Err(format!("No {kind} called \u{201c}{typed}\u{201d}.")),
        many => Err(format!(
            "\u{201c}{typed}\u{201d} fits several {kind}s: {}. Type more of the name, or its number.",
            many.iter().map(|at| format!("{} ({})", names[*at], at + 1)).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Which slides differ between two versions of a song, as words to follow its name:
/// ", slide 2", ", slides 2 and 5", ", its slides (now 6)", or nothing when only the song's
/// other settings changed.
fn changed_slides(before: &sp_model::Presentation, after: &sp_model::Presentation) -> String {
    if before.slide_count() != after.slide_count() {
        return format!(", its slides (now {})", after.slide_count());
    }
    let changed: Vec<String> = (0..after.slide_count())
        .filter(|at| before.slide(*at).map(|(_, slide)| slide) != after.slide(*at).map(|(_, slide)| slide))
        .map(|at| (at + 1).to_string())
        .collect();
    match changed.as_slice() {
        [] => String::new(),
        [one] => format!(", slide {one}"),
        [rest @ .., last] => format!(", slides {} and {last}", rest.join(", ")),
    }
}

/// Lists names, numbered from 1, one to a line.
pub(crate) fn numbered(names: &[String]) -> String {
    if names.is_empty() {
        return "(none)".to_owned();
    }
    names.iter().enumerate().map(|(at, name)| format!("{:>3}. {name}", at + 1)).collect::<Vec<_>>().join("\n")
}

/// Words joined back up, for names with spaces typed without quotes.
fn rest(args: &[String], from: usize) -> String {
    args.get(from..).map(|words| words.join(" ")).unwrap_or_default()
}

fn on_off(word: Option<&String>) -> Result<bool, String> {
    match word.map(|w| w.to_lowercase()).as_deref() {
        Some("on") => Ok(true),
        Some("off") => Ok(false),
        _ => Err("Say on or off.".to_owned()),
    }
}

impl crate::App {
    /// Carries out one command from outside the window, as the window's own controls do, and
    /// says what happened.
    pub(crate) fn control(&mut self, el: &ActiveEventLoop, args: &[String]) -> Reply {
        let held_before = self.held_songs.clone();
        self.last_job = None;
        let word = args.first().map(|w| w.to_lowercase()).unwrap_or_default();
        let result = match word.as_str() {
            "" | "help" => Ok(Reply::done(HELP.trim_end())),
            "status" => Ok(self.ctl_status()),
            "songs" => Ok(self.ctl_songs()),
            "slides" => self.ctl_slides(&rest(args, 1)),
            "slide" => self.ctl_slide(args),
            "playlists" => Ok(self.ctl_playlists()),
            "libraries" => Ok(self.ctl_libraries()),
            "library" => self.ctl_library(args),
            "playlist" => self.ctl_playlist(args),
            "media" => self.ctl_media(args),
            "media-playlist" => self.ctl_media_playlist(args),
            "themes" => Ok(self.ctl_named_list("themes", |s| s.theme_names.clone())),
            "screens" => Ok(self.ctl_screens()),
            "timers" => Ok(self.ctl_named_list("timers", |s| s.timers.iter().map(|x| x.name.clone()).collect())),
            "messages" => Ok(self.ctl_named_list("messages", |s| s.messages.iter().map(|x| x.name.clone()).collect())),
            "props" => Ok(self.ctl_named_list("props", |s| s.props.iter().map(|x| x.name.clone()).collect())),
            "macros" => Ok(self.ctl_named_list("macros", |s| s.macros.iter().map(|x| x.name.clone()).collect())),
            "audio" => self.ctl_audio(args),
            "next" | "previous" | "prev" | "back" | "first" | "last" => {
                let command = match word.as_str() {
                    "next" => self.continue_playlist(Command::Next),
                    "first" => Command::First,
                    "last" => Command::Last,
                    _ => Command::Previous,
                };
                self.apply(vec![command]);
                Ok(Reply::done(self.live_words()))
            }
            "go" => self.ctl_go(args),
            "select" => self.ctl_select(&rest(args, 1)),
            "clear" => self.ctl_clear(args.get(1).map(String::as_str).unwrap_or("all")),
            "audience" => on_off(args.get(1)).map(|on| {
                self.apply(vec![Command::SetOutputEnabled(on)]);
                Reply::done(format!("The audience screens are {}.", if on { "on" } else { "off" }))
            }),
            "stage" => on_off(args.get(1)).map(|on| {
                self.with_state(|state| state.stage_on = on);
                Reply::done(format!("The stage screens are {}.", if on { "on" } else { "off" }))
            }),
            "identify" => on_off(args.get(1)).map(|on| {
                self.with_state(|state| state.identify = on);
                Reply::done(if on { "Identifying the screens." } else { "Stopped identifying the screens." })
            }),
            "view" => self.ctl_view(args.get(1).map(String::as_str).unwrap_or("")),
            "message" => self.ctl_message(args),
            "prop" => self.ctl_prop(args),
            "timer" => self.ctl_timer(args),
            "macro" => self.ctl_macro(args),
            "announce" => self.ctl_announce(args),
            "bible" => self.ctl_bible(args),
            "song" => self.ctl_song(args),
            "job" => self.ctl_job(args),
            "search" => self.ctl_search(&rest(args, 1)),
            "undo" => self.ctl_undo(el, false),
            "redo" => self.ctl_undo(el, true),
            "menu" => self.ctl_menu(el, args),
            other => Err(format!("SoftPresenter doesn't know the command \u{201c}{other}\u{201d}. See: sp help")),
        };
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        // A change that couldn't be saved, because the song's file changed outside the app, is
        // said.
        let mut reply = result.unwrap_or_else(Reply::failed);
        // A song pack job started in the background: its number, for `job N` and `--wait`.
        if let Some(job) = self.last_job.take() {
            match &mut reply.data {
                serde_json::Value::Object(map) => {
                    map.insert("job".to_owned(), serde_json::json!(job));
                }
                data => *data = serde_json::json!({"job": job}),
            }
            reply.text.push_str(&format!(" (job {job}; sp job {job} says how it went)"));
        }
        for at in self.held_songs.iter().filter(|at| !held_before.contains(at)) {
            // The change is made in the app but not saved: a script must be able to tell.
            reply.ok = false;
            let title = self.show.presentations().get(*at).map_or("The song", |song| song.title.as_str());
            reply.text.push_str(&format!(
                "\nNot saved: {title}'s file was changed outside SoftPresenter. sp song reload {title} --now takes the file's version; sp song keep {title} saves this one."
            ));
        }
        reply
    }

    pub(crate) fn with_state(&mut self, change: impl FnOnce(&mut crate::home::HomeState)) {
        if let Some(running) = self.running.as_mut() {
            change(&mut running.home_state);
        }
    }

    /// The songs as `ctl songs` lists them (Bible pages and deleted songs left out): each one's
    /// place in the show, and its title.
    pub(crate) fn listed_songs(&self) -> Vec<(usize, String)> {
        let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
        self.show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(at, _)| !self.bible_slots.contains(at) && !removed.contains(at))
            .map(|(at, song)| (at, song.title.clone()))
            .collect()
    }

    /// The song a name or number stands for, read again first if its file changed outside the
    /// app (unless it is in use; then the window offers Reload).
    pub(crate) fn find_song(&mut self, typed: &str) -> Result<usize, String> {
        let at = self.find_song_as_is(typed)?;
        self.freshen(at, false);
        Ok(at)
    }

    /// The song a name or number stands for, as the app has it.
    pub(crate) fn find_song_as_is(&self, typed: &str) -> Result<usize, String> {
        let songs = self.listed_songs();
        let names: Vec<String> = songs.iter().map(|(_, title)| title.clone()).collect();
        find("song", &names, typed).map(|at| songs[at].0)
    }

    /// `song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]`.
    fn ctl_song_export(&mut self, args: &[String]) -> Result<Reply, String> {
        const USAGE: &str = "song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]";
        let mut songs = Vec::new();
        let (mut to, mut playlist, mut videos) = (None, None, true);
        let mut words = args.iter();
        while let Some(word) = words.next() {
            match word.as_str() {
                "--to" => to = Some(words.next().ok_or(USAGE)?.clone()),
                "--playlist" => playlist = Some(words.next().ok_or(USAGE)?.clone()),
                "--no-videos" => videos = false,
                song => songs.push(song.to_owned()),
            }
        }
        let to = to.ok_or_else(|| format!("Where should the pack go? {USAGE}"))?;
        let mut to = std::path::PathBuf::from(to);
        if !crate::pack::is_pack(&to) {
            to.set_extension(crate::pack::ENDING);
        }
        if !to.parent().is_some_and(|dir| dir.as_os_str().is_empty() || dir.is_dir()) {
            return Err(format!(
                "There is no folder {}.",
                to.parent().map_or_else(String::new, |d| d.display().to_string())
            ));
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.services.list.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        let playlist = playlist.map(|typed| find("playlist", &names, &typed)).transpose()?;
        if songs.is_empty() && playlist.is_none() {
            return Err(format!("Which songs? {USAGE}"));
        }
        let mut indices = Vec::new();
        for song in &songs {
            indices.push(self.find_song(song)?);
        }
        let mut export = self.plan_pack(&indices, playlist)?;
        if !videos {
            export.leave_out_videos();
        }
        let said = self.write_pack(export, to)?;
        Ok(Reply::done(said))
    }

    /// `job N`: how a song pack written or opened in the background went, or that it is still
    /// going.
    fn ctl_job(&self, args: &[String]) -> Result<Reply, String> {
        let job: u64 = args
            .get(1)
            .and_then(|w| w.parse().ok())
            .ok_or("job N (the number a background song pack job was given)")?;
        match self.jobs.iter().find(|(have, _)| *have == job) {
            None => Err(format!("There is no job {job}.")),
            Some((_, None)) => {
                Ok(Reply::with(format!("Job {job} is still going."), serde_json::json!({"job": job, "done": false})))
            }
            Some((_, Some(Ok(said)))) => Ok(Reply::with(said.clone(), serde_json::json!({"job": job, "done": true}))),
            Some((_, Some(Err(problem)))) => {
                Ok(Reply { ok: false, text: problem.clone(), data: serde_json::json!({"job": job, "done": true}) })
            }
        }
    }

    /// `song reload [SONG [--now]]`.
    fn ctl_song_reload(&mut self, args: &[String]) -> Result<Reply, String> {
        let now = args.iter().any(|w| w == "--now");
        let typed = args.iter().filter(|w| *w != "--now").cloned().collect::<Vec<_>>().join(" ");
        if typed.trim().is_empty() || typed.trim().eq_ignore_ascii_case("all") {
            let done = self.reload_all();
            let data = serde_json::json!({"read": done.read, "added": done.added, "waiting": done.waiting, "problems": done.problems});
            return Ok(Reply::with(done.summary(), data));
        }
        let at = self.find_song_as_is(&typed)?;
        let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
        if now {
            let title = self.reload_song(at)?;
            crate::applog::event("outside", &format!("read {title} again (sp song reload --now)"));
            return Ok(Reply::done(format!("Read {title} again from its file.")));
        }
        if self.freshen(at, false) {
            return Ok(Reply::done(format!("Read {title} again from its file.")));
        }
        let waiting =
            self.running.as_ref().is_some_and(|r| r.home_state.waiting.contains(&crate::outside::Waiting::Song(at)));
        if waiting {
            return Err(format!(
                "{title} is on screen, being edited, holds changes not saved, or its file is gone, so it wasn't read again; the window offers Reload. \
                 sp song reload {title} --now takes the file's version; sp song keep {title} saves SoftPresenter's."
            ));
        }
        Ok(Reply::done(format!("{title} is as its file has it.")))
    }

    /// What is on screen, in words: "Sample song, slide 3 of 8 (Chorus)".
    fn live_words(&self) -> String {
        match self.show.live() {
            None => "Nothing is on screen.".to_owned(),
            Some(at) => {
                let song = self.show.presentations().get(at.presentation);
                let title = song.map_or("?", |s| s.title.as_str());
                let count = song.map_or(0, |s| s.slide_count());
                let group = song.and_then(|s| s.slide(at.slide)).map(|(g, _)| g.name.clone()).filter(|n| !n.is_empty());
                let group = group.map(|g| format!(" ({g})")).unwrap_or_default();
                format!("On screen: {title}, slide {} of {count}{group}.", at.slide + 1)
            }
        }
    }

    fn ctl_status(&self) -> Reply {
        let live = self.show.live();
        let song = live.and_then(|at| self.show.presentations().get(at.presentation));
        let words = live.and_then(|at| song?.slide(at.slide)).map(|(_, slide)| {
            slide.texts().into_iter().map(|(_, text)| text.replace('\n', " / ")).collect::<Vec<_>>().join(" / ")
        });
        let next = live.and(self.show.upcoming()).map(|at| at.slide + 1);
        let media = self.show.live_media().and_then(|at| self.show.media().get(at)).map(|item| item.name.clone());
        let state = self.running.as_ref().map(|r| &r.home_state);
        let message = state.and_then(|s| {
            let (id, _) = s.message_live?;
            s.messages.iter().find(|m| m.id == id).map(|m| m.name.clone())
        });
        let props: Vec<String> = state
            .map(|s| {
                s.props_live
                    .iter()
                    .filter_map(|(id, _)| s.props.iter().find(|p| p.id == *id))
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let selected = self.show.presentations().get(self.show.selected()).map(|s| s.title.clone());
        let mut lines = vec![self.live_words()];
        if let Some(words) = &words {
            lines.push(format!("Words: {words}"));
        }
        if let Some(next) = next {
            lines.push(format!("Next: slide {next}"));
        }
        lines.push(format!("Media: {}", media.clone().unwrap_or_else(|| "none".to_owned())));
        if let Some(message) = &message {
            lines.push(format!("Message: {message}"));
        }
        if !props.is_empty() {
            lines.push(format!("Props: {}", props.join(", ")));
        }
        lines.push(format!(
            "Audience screens: {}   Stage screens: {}",
            if self.show.output_enabled() { "on" } else { "off" },
            if state.is_some_and(|s| s.stage_on) { "on" } else { "off" }
        ));
        if let Some(selected) = &selected {
            lines.push(format!("Song shown in the window: {selected}"));
        }
        let data = serde_json::json!({
            "live": live.map(|at| serde_json::json!({
                "song": song.map(|s| s.title.clone()),
                "slide": at.slide + 1,
                "slides": song.map(|s| s.slide_count()),
                "words": words,
            })),
            "next_slide": next,
            "media": media,
            "message": message,
            "props": props,
            "audience_on": self.show.output_enabled(),
            "stage_on": state.is_some_and(|s| s.stage_on),
            "selected_song": selected,
        });
        Reply::with(lines.join("\n"), data)
    }

    fn ctl_songs(&self) -> Reply {
        // A song whose file changed outside the app (looked at now), or that waits on Reload or
        // Keep mine, is marked.
        let waiting = |at: usize| {
            let file_changed =
                self.sources.get(at).cloned().flatten().is_some_and(|path| {
                    self.stamps.check(&crate::load::song_file_for(&path)) != crate::outside::Now::Same
                });
            file_changed
                || self
                    .running
                    .as_ref()
                    .is_some_and(|r| r.home_state.waiting.contains(&crate::outside::Waiting::Song(at)))
        };
        let names: Vec<String> = self
            .listed_songs()
            .into_iter()
            .map(|(at, title)| if waiting(at) { format!("{title} (changed outside SoftPresenter)") } else { title })
            .collect();
        Reply::with(numbered(&names), serde_json::json!(names))
    }

    fn ctl_named_list(&self, _kind: &str, names: impl Fn(&crate::home::HomeState) -> Vec<String>) -> Reply {
        let names = self.running.as_ref().map(|r| names(&r.home_state)).unwrap_or_default();
        Reply::with(numbered(&names), serde_json::json!(names))
    }

    fn ctl_slides(&mut self, typed: &str) -> Result<Reply, String> {
        let at = if typed.trim().is_empty() { self.show.selected() } else { self.find_song(typed)? };
        let song = self.show.presentations().get(at).ok_or("That song is gone.")?;
        let mut lines = Vec::new();
        let mut data = Vec::new();
        for (index, (group, slide)) in song.slides().enumerate() {
            let words =
                slide.texts().into_iter().map(|(_, text)| text.replace('\n', " / ")).collect::<Vec<_>>().join(" / ");
            let live = self.show.live() == Some(SlideRef { presentation: at, slide: index });
            lines.push(format!(
                "{:>3}. {}{}{}",
                index + 1,
                if group.name.is_empty() { String::new() } else { format!("[{}] ", group.name) },
                words,
                if live { "   ← on screen" } else { "" }
            ));
            data.push(serde_json::json!({"slide": index + 1, "group": group.name, "words": words, "on_screen": live}));
        }
        Ok(Reply::with(format!("{}\n{}", song.title, lines.join("\n")), serde_json::json!(data)))
    }

    fn ctl_go(&mut self, args: &[String]) -> Result<Reply, String> {
        // The last word is the slide's number, when it is a number and there is a song before it.
        let (song_words, slide) = match args.last().and_then(|w| w.parse::<usize>().ok()) {
            Some(number) if args.len() > 2 => (rest(&args[..args.len() - 1], 1), Some(number)),
            _ => (rest(args, 1), None),
        };
        let at = self.find_song(&song_words)?;
        let song = self.show.presentations().get(at).ok_or("That song is gone.")?.clone();
        if song.slide_count() == 0 {
            return Err(format!("{} has no slides.", song.title));
        }
        let slide = match slide {
            Some(number) if (1..=song.slide_count()).contains(&number) => number - 1,
            Some(number) => {
                return Err(format!("{} has {} slides; there is no slide {number}.", song.title, song.slide_count()));
            }
            None => song.order().into_iter().find(|s| song.slide(*s).is_some_and(|(_, sl)| sl.enabled)).unwrap_or(0),
        };
        self.apply(vec![Command::Select(at), Command::GoTo(SlideRef { presentation: at, slide })]);
        Ok(Reply::done(self.live_words()))
    }

    fn ctl_select(&mut self, typed: &str) -> Result<Reply, String> {
        let at = self.find_song(typed)?;
        self.apply(vec![Command::Select(at)]);
        self.with_state(|state| {
            state.bible_open = false;
            state.services.chosen = None;
        });
        let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
        Ok(Reply::done(format!("Showing {title}'s slides.")))
    }

    fn ctl_clear(&mut self, what: &str) -> Result<Reply, String> {
        match what.to_lowercase().as_str() {
            "all" => self.apply(vec![Command::ClearAll]),
            "slide" | "slides" => self.apply(vec![Command::Clear(Layer::Slides)]),
            "media" => self.apply(vec![Command::Clear(Layer::Media)]),
            "props" => self.apply(vec![Command::Clear(Layer::Props)]),
            "messages" | "message" => self.apply(vec![Command::Clear(Layer::Messages)]),
            "audio" => self.apply(vec![Command::Clear(Layer::Audio)]),
            "announcements" | "announcement" => {
                self.with_state(|state| state.announcement_request = Some(None));
                self.handle_announcement();
            }
            other => return Err(format!("Clear what? \u{201c}{other}\u{201d} isn't a layer.")),
        }
        Ok(Reply::done(format!("Cleared {what}.")))
    }

    fn ctl_view(&mut self, which: &str) -> Result<Reply, String> {
        let at = match which.to_lowercase().as_str() {
            "show" => 0,
            "edit" => 1,
            "bible" => 2,
            "themes" => 3,
            _ => return Err("Which view: show, edit, bible or themes?".to_owned()),
        };
        self.with_state(|state| state.view_request = Some(at));
        Ok(Reply::done(format!("Switching to the {which} view.")))
    }

    /// Each screen, and what it is doing: filling a display, in a window, sending over NDI, or
    /// off and why.
    fn ctl_screens(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else {
            return Reply::with("", serde_json::json!([]));
        };
        let lines: Vec<String> = state
            .screens
            .iter()
            .enumerate()
            .map(|(at, screen)| match state.screen_status.get(at) {
                Some((doing, _)) => format!("{}: {doing}", screen.name),
                None => screen.name.clone(),
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .screens
            .iter()
            .enumerate()
            .map(|(at, screen)| {
                let (doing, shown) = state.screen_status.get(at).cloned().unwrap_or_default();
                serde_json::json!({"name": screen.name, "doing": doing, "shown": shown})
            })
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    fn ctl_libraries(&self) -> Reply {
        let Some(state) = self.running.as_ref().map(|r| &r.home_state) else { return Reply::done("(none)") };
        // Each library's songs, and how many of them can't be found.
        let counts: Vec<(usize, usize)> = state
            .libraries
            .iter()
            .map(|l| {
                let missing =
                    l.songs.iter().filter(|key| crate::studio::playlist_song(&self.show, state, key).is_none()).count();
                (l.songs.len(), missing)
            })
            .collect();
        let lines: Vec<String> = state
            .libraries
            .iter()
            .zip(&counts)
            .map(|(l, (songs, missing))| {
                let plural = if *songs == 1 { "" } else { "s" };
                match missing {
                    0 => format!("{} ({songs} song{plural})", l.name),
                    _ => format!("{} ({songs} song{plural}, {missing} missing)", l.name),
                }
            })
            .collect();
        let data: Vec<serde_json::Value> = state
            .libraries
            .iter()
            .zip(&counts)
            .map(|(l, (songs, missing))| serde_json::json!({"name": l.name, "songs": songs, "missing": missing}))
            .collect();
        Reply::with(numbered(&lines), serde_json::json!(data))
    }

    fn ctl_library(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_library_change(args) {
            return result;
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.libraries.iter().map(|l| l.name.clone()).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("open") if rest(args, 2).eq_ignore_ascii_case("all") => {
                self.with_state(|state| {
                    state.library_shown = None;
                    state.playlist_open = false;
                });
                Ok(Reply::done("Showing all songs."))
            }
            Some("open") => {
                let at = find("library", &names, &rest(args, 2))?;
                self.with_state(|state| {
                    state.library_shown = state.libraries.get(at).map(|l| l.id);
                    state.playlist_open = false;
                });
                Ok(Reply::done(format!("Showing the library {}.", names[at])))
            }
            Some(_) => {
                // `library NAME`: its songs, one that can't be found marked as missing.
                let at = find("library", &names, &rest(args, 1))?;
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let library = state.libraries.get(at).ok_or("No such library.")?;
                let songs: Vec<String> = library
                    .songs
                    .iter()
                    .map(|key| match crate::studio::playlist_song(&self.show, state, key) {
                        Some(index) => self.show.presentations()[index].title.clone(),
                        None => format!("{} (missing)", key.strip_prefix("title:").unwrap_or("a song")),
                    })
                    .collect();
                Ok(Reply::with(format!("{}\n{}", library.name, numbered(&songs)), serde_json::json!(songs)))
            }
            None => Err("library NAME, library open NAME, or library open all".to_owned()),
        }
    }

    fn ctl_playlists(&self) -> Reply {
        let state = self.running.as_ref().map(|r| &r.home_state);
        let names: Vec<String> =
            state.map(|s| s.services.list.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
        Reply::with(numbered(&names), serde_json::json!(names))
    }

    fn ctl_playlist(&mut self, args: &[String]) -> Result<Reply, String> {
        if let Some(result) = self.ctl_playlist_change(args) {
            return result;
        }
        let names: Vec<String> = self
            .running
            .as_ref()
            .map(|r| r.home_state.services.list.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("open") => {
                let at = find("playlist", &names, &rest(args, 2))?;
                self.with_state(|state| {
                    state.services.show(at);
                    state.playlist_open = true;
                });
                Ok(Reply::done(format!("Showing the playlist {}.", names[at])))
            }
            Some("add") => {
                // `playlist add SONG [to PLAYLIST]`, or the playlist in front.
                let words = &args[2..];
                let (song_words, list_words) = match words.iter().position(|w| w.eq_ignore_ascii_case("to")) {
                    Some(at) => (words[..at].join(" "), Some(words[at + 1..].join(" "))),
                    None => (words.join(" "), None),
                };
                let song = self.find_song(&song_words)?;
                let list = list_words.map(|typed| find("playlist", &names, &typed)).transpose()?;
                let running = self.running.as_mut().ok_or("The app isn't ready.")?;
                let state = &mut running.home_state;
                let key = state.song_keys.get(song).cloned().ok_or("That song can't go in a playlist.")?;
                let title = self.show.presentations().get(song).map(|s| s.title.clone()).unwrap_or_default();
                let item = crate::service::Item::Song { key, title: title.clone() };
                let at = list.unwrap_or_else(|| {
                    state.services.list.iter().position(|p| p.name == state.services.front().name).unwrap_or(0)
                });
                let into = names.get(at).cloned().unwrap_or_default();
                if state.services.add_to(at, item) {
                    Ok(Reply::done(format!("Added {title} to {into}.")))
                } else {
                    Ok(Reply::done(format!("{title} is already in {into}.")))
                }
            }
            // `playlist [NAME]`: the items of a playlist, or of the one in front.
            _ => {
                let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
                let at = match rest(args, 1) {
                    typed if typed.trim().is_empty() => {
                        state.services.list.iter().position(|p| p.name == state.services.front().name).unwrap_or(0)
                    }
                    typed => find("playlist", &names, &typed)?,
                };
                let playlist = state.services.list.get(at).ok_or("No such playlist.")?;
                let items: Vec<String> = playlist
                    .items
                    .iter()
                    .map(|item| match item {
                        crate::service::Item::Header { name } => format!("— {name} —"),
                        // A song that can't be found is kept, and shown as missing.
                        crate::service::Item::Song { key, title }
                            if crate::studio::playlist_song(&self.show, state, key).is_none() =>
                        {
                            format!("{title} (missing)")
                        }
                        other => other.title().to_owned(),
                    })
                    .collect();
                Ok(Reply::with(format!("{}\n{}", playlist.name, numbered(&items)), serde_json::json!(items)))
            }
        }
    }

    fn media_names(&self) -> Vec<String> {
        self.show.media().iter().map(|item| item.name.clone()).collect()
    }

    fn ctl_media(&mut self, args: &[String]) -> Result<Reply, String> {
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => {
                let at = find("media item", &self.media_names(), &rest(args, 2))?;
                self.apply(vec![Command::ShowMedia(at)]);
                Ok(Reply::done(format!("Showing {}.", self.show.media()[at].name)))
            }
            Some("add") => {
                let files: Vec<PathBuf> = args[2..].iter().map(PathBuf::from).collect();
                if files.is_empty() {
                    return Err("Add which files?".to_owned());
                }
                for file in &files {
                    if !file.is_file() {
                        return Err(format!("There is no file {}.", file.display()));
                    }
                }
                // Pictures and videos only: songs are imported with song import. Each file not
                // added says why.
                let mut added = Vec::new();
                let mut problems = Vec::new();
                for file in &files {
                    let name = file
                        .file_name()
                        .map_or_else(|| file.display().to_string(), |n| n.to_string_lossy().into_owned());
                    if !crate::media::is_media(file) {
                        problems.push(if crate::audio::is_audio(file) {
                            format!("{name} is a sound: sounds go in the Audio tab (there is no command for that yet).")
                        } else {
                            format!("{name} isn't a picture or video (songs come in with song import).")
                        });
                        continue;
                    }
                    match self.add_media_file(file) {
                        Ok(true) => added.push(name),
                        Ok(false) => problems.push(format!("{name} is already in the media bin.")),
                        Err(problem) => problems.push(format!("{name}: {problem}")),
                    }
                }
                let data = serde_json::json!({"added": added, "problems": problems});
                if added.is_empty() {
                    return Err(problems.join("\n"));
                }
                let mut text = match added.as_slice() {
                    [one] => format!("Added {one} to the media bin."),
                    many => format!("Added {} to the media bin: {}.", many.len(), many.join(", ")),
                };
                for problem in &problems {
                    text.push_str(&format!("\n{problem}"));
                }
                Ok(Reply::with(text, data))
            }
            Some("delete") => {
                let yes = args.iter().any(|w| w == "--yes");
                let words: Vec<String> = args[2..].iter().filter(|w| *w != "--yes").cloned().collect();
                let at = find("media item", &self.media_names(), &words.join(" "))?;
                let name = self.show.media()[at].name.clone();
                if !yes {
                    return Err(format!("Deleting {name} from the media bin can't be undone. Add --yes to do it."));
                }
                self.with_state(|state| state.media_request = Some(crate::home::MediaRequest::Delete(vec![at])));
                self.handle_media_request();
                Ok(Reply::done(format!("Deleted {name} from the media bin (the file stays where it is).")))
            }
            _ => {
                let names = self.media_names();
                Ok(Reply::with(numbered(&names), serde_json::json!(names)))
            }
        }
    }

    fn ctl_media_playlist(&mut self, args: &[String]) -> Result<Reply, String> {
        let lists: Vec<(sp_model::Uuid, String)> = self
            .running
            .as_ref()
            .map(|r| r.home_state.media_playlists.iter().map(|p| (p.id, p.name.clone())).collect())
            .unwrap_or_default();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("play") => {
                let names: Vec<String> = lists.iter().map(|(_, name)| name.clone()).collect();
                let at = find("media playlist", &names, &rest(args, 2))?;
                let has_items = self.running.as_ref().is_some_and(|r| {
                    r.home_state
                        .media_playlists
                        .iter()
                        .find(|p| p.id == lists[at].0)
                        .is_some_and(|p| self.show.media().iter().any(|item| p.has(&item.path)))
                });
                if !has_items {
                    return Err(format!("The media playlist {} has nothing in it to play.", names[at]));
                }
                self.play_media_playlist(lists[at].0);
                Ok(Reply::done(format!("Playing the media playlist {}.", names[at])))
            }
            Some("stop") => {
                self.stop_media_playlist();
                Ok(Reply::done("Stopped the media playlist."))
            }
            _ => {
                let names: Vec<String> = lists.into_iter().map(|(_, name)| name).collect();
                Ok(Reply::with(numbered(&names), serde_json::json!(names)))
            }
        }
    }

    fn ctl_message(&mut self, args: &[String]) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.messages.iter().map(|m| m.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.messages.iter().map(|m| m.id).collect();
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => {
                let at = find("message", &names, &rest(args, 2))?;
                self.run_action(sp_model::ShowAction::Message { id: ids[at], show: true });
                Ok(Reply::done(format!("Showing the message {}.", names[at])))
            }
            Some("hide") => {
                self.apply(vec![Command::Clear(Layer::Messages)]);
                Ok(Reply::done("The message is off the screens."))
            }
            _ => Err("message show NAME, or message hide".to_owned()),
        }
    }

    fn ctl_prop(&mut self, args: &[String]) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.props.iter().map(|p| p.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.props.iter().map(|p| p.id).collect();
        let show = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("show") => true,
            Some("hide") => false,
            _ => return Err("prop show NAME, or prop hide NAME".to_owned()),
        };
        let at = find("prop", &names, &rest(args, 2))?;
        self.run_action(sp_model::ShowAction::Prop { id: ids[at], show });
        Ok(Reply::done(format!("{} the prop {}.", if show { "Showing" } else { "Hid" }, names[at])))
    }

    fn ctl_timer(&mut self, args: &[String]) -> Result<Reply, String> {
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.timers.iter().map(|t| t.name.clone()).collect();
        let ids: Vec<sp_model::Uuid> = state.timers.iter().map(|t| t.id).collect();
        let act = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("start") => sp_model::TimerDo::Start,
            Some("stop") => sp_model::TimerDo::Stop,
            Some("reset") => sp_model::TimerDo::Reset,
            _ => return Err("timer start|stop|reset NAME".to_owned()),
        };
        let at = find("timer", &names, &rest(args, 2))?;
        self.run_action(sp_model::ShowAction::Timer { id: ids[at], act });
        let done = match act {
            sp_model::TimerDo::Start => "Started",
            sp_model::TimerDo::Stop => "Stopped",
            sp_model::TimerDo::Reset => "Reset",
        };
        Ok(Reply::done(format!("{done} the timer {}.", names[at])))
    }

    fn ctl_macro(&mut self, args: &[String]) -> Result<Reply, String> {
        if args.get(1).map(|w| w.to_lowercase()).as_deref() != Some("run") {
            return Err("macro run NAME".to_owned());
        }
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let names: Vec<String> = state.macros.iter().map(|m| m.name.clone()).collect();
        let at = find("macro", &names, &rest(args, 2))?;
        let actions = state.macros[at].actions.clone();
        // A "go to song" step whose song can't be found is said, not passed over in silence.
        let lost = actions
            .iter()
            .filter(|action| match action {
                sp_model::ShowAction::Song { key } => crate::studio::playlist_song(&self.show, state, key).is_none(),
                _ => false,
            })
            .count();
        self.with_state(|state| state.actions_to_run.extend(actions));
        self.run_actions();
        let mut text = format!("Ran the macro {}. {}", names[at], self.live_words());
        if lost > 0 {
            text.push_str(match lost {
                1 => "\nIts \"go to song\" step found no song.",
                _ => "\nSome of its \"go to song\" steps found no song.",
            });
        }
        Ok(Reply::done(text))
    }

    /// Does one slide action now, as a slide or a macro does it.
    fn run_action(&mut self, action: sp_model::ShowAction) {
        self.with_state(|state| state.actions_to_run.push(action));
        self.run_actions();
    }

    fn ctl_announce(&mut self, args: &[String]) -> Result<Reply, String> {
        let typed = rest(args, 1);
        if typed.eq_ignore_ascii_case("stop") || typed.trim().is_empty() {
            self.with_state(|state| state.announcement_request = Some(None));
            self.handle_announcement();
            return Ok(Reply::done("The Announcements layer is clear."));
        }
        let at = self.find_song(&typed)?;
        self.with_state(|state| state.announcement_request = Some(Some(at)));
        self.handle_announcement();
        let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
        Ok(Reply::done(format!("Playing {title} on the Announcements layer.")))
    }

    fn ctl_audio(&mut self, args: &[String]) -> Result<Reply, String> {
        use crate::home::AudioRequest;
        let state = &self.running.as_ref().ok_or("The app isn't ready.")?.home_state;
        let tracks: Vec<(sp_model::Uuid, String)> = state
            .audio
            .playlists
            .iter()
            .flat_map(|playlist| playlist.tracks.iter().map(|track| (track.id, track.name.clone())))
            .collect();
        let (request, done) = match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("play") => {
                let names: Vec<String> = tracks.iter().map(|(_, name)| name.clone()).collect();
                let at = find("track", &names, &rest(args, 2))?;
                (AudioRequest::Play(tracks[at].0), format!("Playing {}.", names[at]))
            }
            Some("pause") => {
                (AudioRequest::TogglePause, "Paused the audio, or played it on if it was paused.".to_owned())
            }
            Some("stop") => (AudioRequest::Stop, "Stopped the audio.".to_owned()),
            Some("volume") => {
                let level: f32 = args.get(2).and_then(|w| w.parse().ok()).ok_or("audio volume 0-100")?;
                if !(0.0..=100.0).contains(&level) {
                    return Err(format!("The volume goes from 0 to 100, not {level}."));
                }
                (AudioRequest::Volume(level / 100.0), format!("Volume {level}."))
            }
            _ => {
                let names: Vec<String> = tracks.into_iter().map(|(_, name)| name).collect();
                return Ok(Reply::with(numbered(&names), serde_json::json!(names)));
            }
        };
        self.with_state(|state| state.audio_request = Some(request));
        self.handle_audio();
        Ok(Reply::done(done))
    }

    fn ctl_bible(&mut self, args: &[String]) -> Result<Reply, String> {
        let show_it = args.iter().any(|w| w == "--show");
        // `--add [to PLAYLIST]`: the passage goes into a playlist, as Add to puts it there.
        let (args, add_to) = match args.iter().position(|w| w == "--add") {
            Some(at) => {
                let tail = &args[at + 1..];
                let into = match tail.first() {
                    Some(word) if word.eq_ignore_ascii_case("to") => Some(Some(tail[1..].join(" "))),
                    _ => Some(None),
                };
                let mut kept = args[..at].to_vec();
                if !matches!(into, Some(Some(_))) {
                    kept.extend(tail.iter().cloned());
                }
                (kept, into)
            }
            None => (args.to_vec(), None),
        };
        let args = args.as_slice();
        let reference: String = args[1..].iter().filter(|w| *w != "--show").cloned().collect::<Vec<_>>().join(" ");
        if reference.trim().is_empty() {
            return Err("Which passage? For example: bible John 3:16-18".to_owned());
        }
        let live = self.show.live().map(|at| at.presentation);
        // A chapter or verse the book doesn't have is said, rather than read as the last one
        // (as typing in Scripture Lookup reads it).
        if let Some(running) = self.running.as_mut() {
            let page = &mut running.home_state.bible;
            if let Some(bible) = page.lead().and_then(|lead| page.shelf.get(lead).ok())
                && let Ok(range) = sp_bible::parse_reference_in(&reference, &bible.info.language)
            {
                let book = bible.book_name(range.start.book);
                let chapters = bible.chapter_count(range.start.book);
                if chapters > 0 && (range.start.chapter > chapters || range.end.chapter > chapters) {
                    return Err(format!("{book} has {chapters} chapters."));
                }
                let verses = bible.last_verse(range.start.book, range.start.chapter);
                if verses > 0 && range.start.verse != sp_bible::LAST && range.start.verse > verses {
                    return Err(format!("{book} {} has {verses} verses.", range.start.chapter));
                }
            }
        }
        let (found, message) = {
            let running = self.running.as_mut().ok_or("The app isn't ready.")?;
            let state = &mut running.home_state;
            state.bible_open = true;
            state.editing = None;
            state.bible.lookup = reference.clone();
            state.bible.look_up(live);
            (state.bible.pending.take(), state.bible.message.clone())
        };
        let Some((slot, passage)) = found else {
            return Err(message.unwrap_or_else(|| format!("Could not look up {reference}.")));
        };
        let title = passage.title.clone();
        self.show.replace_presentation(slot, std::sync::Arc::new(passage));
        if let Some(into) = add_to {
            let list = self.playlist_at_for_cli(into.as_deref())?;
            let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
            let saved = state.bible.saved().ok_or("The passage could not be kept for a playlist.")?;
            let name = state.services.list[list].name.clone();
            state.services.add_to(list, crate::service::Item::Passage(saved));
            if show_it {
                self.apply(vec![Command::Select(slot), Command::GoTo(SlideRef { presentation: slot, slide: 0 })]);
            }
            return Ok(Reply::done(format!("Added {title} to {name}.")));
        }
        if show_it {
            self.apply(vec![Command::Select(slot), Command::GoTo(SlideRef { presentation: slot, slide: 0 })]);
            return Ok(Reply::done(format!("{title}: {}", self.live_words())));
        }
        Ok(Reply::done(format!("Looked up {title}; it is on the Bible page.")))
    }

    fn ctl_song(&mut self, args: &[String]) -> Result<Reply, String> {
        if !args.iter().any(|w| w == "--library")
            && let Some(result) = self.ctl_song_change(args)
        {
            return result;
        }
        // `--library NAME` (or `--library none`): the library new and imported songs go into,
        // instead of the one shown in the window.
        if let Some(at) = args.iter().position(|w| w == "--library") {
            let name = args.get(at + 1).cloned().ok_or("--library NAME, or --library none")?;
            let mut rest: Vec<String> = args.to_vec();
            rest.drain(at..=at + 1);
            let state = &mut self.running.as_mut().ok_or("The app isn't ready.")?.home_state;
            let wanted = if name.eq_ignore_ascii_case("none") {
                None
            } else {
                let names: Vec<String> = state.libraries.iter().map(|l| l.name.clone()).collect();
                Some(state.libraries[find("library", &names, &name)?].id)
            };
            let shown = std::mem::replace(&mut state.library_shown, wanted);
            let result = self.ctl_song(&rest);
            if let Some(running) = self.running.as_mut() {
                running.home_state.library_shown = shown;
            }
            return result;
        }
        // Where new songs go: the library shown, said in the reply.
        let into = self.running.as_ref().and_then(|r| {
            let state = &r.home_state;
            state.library_shown.and_then(|id| state.libraries.iter().find(|l| l.id == id)).map(|l| l.name.clone())
        });
        let into_words = into.map_or_else(String::new, |name| format!(" into the library {name}"));
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("new") => {
                // `song new TITLE [FILE.txt]`: the file is the last word when it names a file.
                let words = &args[2..];
                let (title_words, file) = match words.last() {
                    Some(last) if words.len() > 1 && Path::new(last).is_file() => {
                        (&words[..words.len() - 1], Some(last))
                    }
                    // A last word that names a file that isn't there is a mistake, not the title.
                    Some(last)
                        if words.len() > 1 && (last.contains('/') || crate::load::is_importable(Path::new(last))) =>
                    {
                        return Err(format!("There is no file {last}."));
                    }
                    _ => (words, None),
                };
                let title = title_words.join(" ");
                if title.trim().is_empty() {
                    return Err("song new TITLE [FILE.txt]".to_owned());
                }
                let words = match file {
                    Some(file) => {
                        sp_model::decode_text(&std::fs::read(file).map_err(|e| format!("Could not read {file}: {e}"))?)
                    }
                    None => String::new(),
                };
                let request = crate::home::NewSong { title: title.trim().to_owned(), theme: None, words, focus: false };
                self.with_state(|state| state.new_song_request = Some(request));
                let same =
                    self.listed_songs().iter().filter(|(_, have)| have.eq_ignore_ascii_case(title.trim())).count();
                self.handle_songs();
                let mut text = format!("Made the song {}{into_words}.", title.trim());
                if same > 0 {
                    text.push_str(&format!(
                        " There was already a song called {}; give its number (sp songs) to tell them apart.",
                        title.trim()
                    ));
                }
                Ok(Reply::done(text))
            }
            Some("delete") => {
                let yes = args.iter().any(|w| w == "--yes");
                let words: Vec<String> = args[2..].iter().filter(|w| *w != "--yes").cloned().collect();
                let at = self.find_song(&words.join(" "))?;
                let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
                if !yes {
                    return Err(format!(
                        "Deleting {title} can't be undone (a song made in SoftPresenter goes to the Trash). Add --yes to do it."
                    ));
                }
                self.with_state(|state| state.songs_to_delete = vec![at]);
                self.handle_song_request();
                Ok(Reply::done(format!("Deleted {title}.")))
            }
            Some("text") => self.ctl_song_text(&args[2..]),
            Some("reload") => self.ctl_song_reload(&args[2..]),
            Some("keep") => {
                let at = self.find_song_as_is(&rest(args, 2))?;
                let waiting = self.held_songs.contains(&at)
                    || self.running.as_ref().is_some_and(|r| {
                        r.home_state.waiting.contains(&crate::outside::Waiting::Song(at))
                    });
                let title = self.show.presentations().get(at).map(|s| s.title.clone()).unwrap_or_default();
                if !waiting {
                    return Err(format!("{title} isn't waiting on a choice: its file hasn't changed outside SoftPresenter."));
                }
                let title = self.keep_song(at)?;
                crate::applog::event("outside", &format!("kept SoftPresenter's version of {title} (sp song keep)"));
                Ok(Reply::done(format!("Saved SoftPresenter's version of {title} over its file.")))
            }
            Some("export") => self.ctl_song_export(&args[2..]),
            Some("import") => {
                // Song packs: what to do with songs the library has already (skip, unless told).
                let same = if args.iter().any(|w| w == "--replace") {
                    crate::pack::Same::Replace
                } else if args.iter().any(|w| w == "--keep-both") {
                    crate::pack::Same::KeepBoth
                } else {
                    crate::pack::Same::Skip
                };
                let mut paths: Vec<std::path::PathBuf> = args[2..]
                    .iter()
                    .filter(|w| !matches!(w.as_str(), "--skip" | "--replace" | "--keep-both"))
                    .map(std::path::PathBuf::from)
                    .collect();
                if paths.is_empty() {
                    return Err("song import FILE…".to_owned());
                }
                // Packs are opened here, and what came in is said; the rest import as songs.
                let library = self.running.as_ref().and_then(|r| r.home_state.library_shown);
                let packs: Vec<std::path::PathBuf> = paths.iter().filter(|path| crate::pack::is_pack(path)).cloned().collect();
                paths.retain(|path| !crate::pack::is_pack(path));
                let mut said_packs = Vec::new();
                let mut any_in = false;
                for path in packs {
                    if !path.is_file() {
                        return Err(format!("There is no file {}.", path.display()));
                    }
                    match self.open_pack(path, Some(same), library, false) {
                        Ok(said) => {
                            any_in = true;
                            said_packs.push(said);
                        }
                        Err(problem) => said_packs.push(problem),
                    }
                }
                if paths.is_empty() {
                    let text = said_packs.join("\n");
                    return if any_in { Ok(Reply::done(text)) } else { Err(text) };
                }
                let (imported, problems, decks) = self.import_songs(&paths);
                let making = (!decks.is_empty()).then(|| {
                    format!("Making slides from {}; each appears in the song list when it is done.", decks.join(", "))
                });
                if imported.is_empty() {
                    if let Some(making) = making {
                        let mut text = making;
                        for problem in problems.iter().take(10) {
                            text.push_str(&format!("\n{problem}"));
                        }
                        let data = serde_json::json!({"imported": imported, "problems": problems, "making": decks});
                        return Ok(Reply::with(text, data));
                    }
                    return Err(if problems.is_empty() {
                        "Nothing was imported.".to_owned()
                    } else {
                        problems.join("\n")
                    });
                }
                // The text names the first few; the data lists them all.
                const NAMED: usize = 10;
                let mut text = match imported.as_slice() {
                    [one] => format!("Imported {one}{into_words}."),
                    many if many.len() <= NAMED => {
                        format!("Imported {} songs{into_words}: {}.", many.len(), many.join(", "))
                    }
                    many => format!(
                        "Imported {} songs{into_words}: {}, and {} more.",
                        many.len(),
                        many[..NAMED].join(", "),
                        many.len() - NAMED
                    ),
                };
                for problem in problems.iter().take(NAMED) {
                    text.push_str(&format!("\n{problem}"));
                }
                if problems.len() > NAMED {
                    text.push_str(&format!("\n…and {} more not imported (see the log).", problems.len() - NAMED));
                }
                if let Some(making) = making {
                    text.push_str(&format!("\n{making}"));
                }
                Ok(Reply::with(text, serde_json::json!({"imported": imported, "problems": problems, "making": decks})))
            }
            _ => Err("song new TITLE [FILE.txt], song import FILE…, song text SONG [FILE], song reload [SONG], song keep SONG, or song delete SONG --yes"
                .to_owned()),
        }
    }

    /// Undoes (or redoes) the last change to a song, whatever view the window shows, and says
    /// what changed. In a song's Edit view it takes the step Cmd-Z would there, as the editor
    /// keeps its own order of steps (typing in a box, then the song's slides).
    fn ctl_undo(&mut self, el: &ActiveEventLoop, redo: bool) -> Result<Reply, String> {
        let word = if redo { "redo" } else { "undo" };
        let in_editor = self.running.as_ref().is_some_and(|r| {
            let state = &r.home_state;
            state.theme_edit.is_none()
                && state.editing.as_ref().is_some_and(|draft| draft.at.presentation != crate::props::PROP_SLOT)
        });
        if in_editor {
            self.menu_action(el, if redo { crate::menubar::Action::Redo } else { crate::menubar::Action::Undo });
            return Ok(Reply::done(format!("Took the Edit view's {word} step (as Cmd-Z does there).")));
        }
        let index = self.history.peek(redo).ok_or_else(|| format!("There is nothing to {word}."))?;
        let before = self.show.presentations().get(index).cloned().ok_or("That song is gone.")?;
        self.undo_show(redo);
        let after = self.show.presentations().get(index).cloned().ok_or("That song is gone.")?;
        let what = changed_slides(&before, &after);
        let done = if redo { "Redid" } else { "Undid" };
        Ok(Reply::with(
            format!("{done} a change to {}{what}.", after.title),
            serde_json::json!({"song": after.title, "number": index + 1}),
        ))
    }

    fn ctl_menu(&mut self, el: &ActiveEventLoop, args: &[String]) -> Result<Reply, String> {
        let items = crate::menubar::labelled_items();
        if args.len() < 2 {
            let lines: Vec<String> =
                items.iter().map(|(action, label)| format!("{:<24} {label}", crate::menubar::id_of(*action))).collect();
            let data: Vec<serde_json::Value> = items
                .iter()
                .map(|(action, label)| serde_json::json!({"id": crate::menubar::id_of(*action), "label": label}))
                .collect();
            return Ok(Reply::with(lines.join("\n"), serde_json::json!(data)));
        }
        // By its id ("ClearAll") or the words on it ("Clear All").
        let typed = rest(args, 1);
        let action = items
            .iter()
            .find(|(action, label)| {
                crate::menubar::id_of(*action).eq_ignore_ascii_case(&typed)
                    || label.trim_end_matches('…').eq_ignore_ascii_case(typed.trim_end_matches('…'))
            })
            .map(|(action, _)| *action)
            .ok_or_else(|| format!("No menu item called \u{201c}{typed}\u{201d}. sp menu lists them."))?;
        self.menu_action(el, action);
        Ok(Reply::done(format!("Chose {} from the menu.", crate::menubar::id_of(action))))
    }
}

#[cfg(test)]
mod tests {

    /// Every first word the app carries out a command for starts a command when typed after
    /// `sp` (without `ctl`), so a new command can't be left out of COMMANDS again.
    #[test]
    fn every_command_the_app_knows_can_be_typed_after_sp() {
        let source = include_str!("control.rs");
        let start = source.find("let result = match word.as_str() {").expect("the commands' match");
        let end = start + source[start..].find("            _ =>").expect("its end");
        let mut words = Vec::new();
        for line in source[start..end].lines() {
            let line = line.trim();
            let Some((pattern, _)) = line.split_once("=>") else { continue };
            if !pattern.trim_start().starts_with('"') {
                continue;
            }
            for word in pattern.split('|') {
                let word = word.trim().trim_matches('"');
                if !word.is_empty() {
                    words.push(word.to_owned());
                }
            }
        }
        assert!(words.len() > 20, "found the commands: {words:?}");
        for word in &words {
            assert!(
                is_command(word),
                "\"{word}\" is carried out but not in COMMANDS, so `sp {word}` would open it as a file"
            );
        }
    }

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_name_is_found_whole_in_part_or_by_number_and_never_guessed() {
        let songs = names(&["Amazing Grace", "How Great Thou Art", "Great Is Thy Faithfulness"]);
        assert_eq!(find("song", &songs, "amazing grace"), Ok(0));
        assert_eq!(find("song", &songs, "amaz"), Ok(0));
        assert_eq!(find("song", &songs, "2"), Ok(1));
        let both = find("song", &songs, "great").unwrap_err();
        assert!(both.contains("How Great Thou Art") && both.contains("Great Is Thy Faithfulness"), "{both}");
        assert!(find("song", &songs, "nothing like it").is_err());
        assert!(find("song", &songs, "9").is_err(), "a number past the end is not a place");
    }

    #[test]
    fn requests_and_replies_round_trip_as_lines_of_json() {
        let request = Request { v: PROTOCOL, args: names(&["go", "Amazing Grace", "3"]) };
        let text = serde_json::to_string(&request).unwrap();
        assert!(!text.contains('\n'));
        let back: Request = serde_json::from_str(&text).unwrap();
        assert_eq!(back.args, request.args);
        let reply = Reply::done("On screen.");
        let back: Reply = serde_json::from_str(&serde_json::to_string(&reply).unwrap()).unwrap();
        assert!(back.ok && back.text == "On screen.");
    }
}
