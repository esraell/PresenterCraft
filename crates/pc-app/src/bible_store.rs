//! Where installed Bibles are kept on this computer, and fetching new ones.
//!
//! A downloaded file is only kept if its checksum is the one this version of the app expects
//! and it reads as a Bible, so a damaged or swapped file is never installed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use pc_bible::{Bible, Info};
use sha2::{Digest, Sha256};

/// Where the Bible files are published: the address their names are added to. They are
/// files attached to a release of the project on GitHub. While the project is private this
/// address answers nobody, and Install says the download could not be made.
const DOWNLOADS: Option<&str> = Some("https://github.com/esraell/presentercraft/releases/download/bibles-2");

/// The largest Bible file that will be fetched. The biggest so far is under 2 MB.
const LARGEST: u64 = 16 * 1024 * 1024;

/// The address Bible files are fetched from, if there is one yet. A testing aid can name
/// another with `PRESENTERCRAFT_BIBLE_URL`; the checksum is checked all the same.
pub fn download_base() -> Option<String> {
    std::env::var("PRESENTERCRAFT_BIBLE_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .or_else(|| DOWNLOADS.map(str::to_owned))
}

/// The folder installed Bibles are kept in. `PRESENTERCRAFT_DATA` names another place for
/// the app's files, which tests use.
pub fn folder() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if let Some(base) = var("PRESENTERCRAFT_DATA") {
        base
    } else if cfg!(target_os = "macos") {
        var("HOME")?.join("Library/Application Support/PresenterCraft")
    } else if cfg!(target_os = "windows") {
        var("APPDATA")?.join("PresenterCraft")
    } else {
        var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local/share")))?.join("presentercraft")
    };
    Some(base.join("Bibles"))
}

/// The Bible files in the folder that read as Bibles, with what each says about itself.
pub fn installed() -> Vec<(PathBuf, Info)> {
    let Some(entries) = folder().and_then(|folder| std::fs::read_dir(folder).ok()) else {
        return Vec::new();
    };
    let mut found: Vec<(PathBuf, Info)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pcbible"))
        .filter_map(|path| {
            let info = Bible::read_info(&std::fs::read(&path).ok()?).ok()?;
            Some((path, info))
        })
        .collect();
    found.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    found
}

fn checksum(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A file name made only from the Bible's short name's safe letters, so what a file says about
/// itself can never choose where it is written.
fn file_name(id: &str) -> Option<String> {
    let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    (!safe.is_empty() && safe.len() <= 40).then(|| format!("{safe}.pcbible"))
}

/// Checks that the bytes are a whole Bible and keeps them in the folder. Returns where, and
/// what the Bible says about itself. The error is a plain sentence.
pub fn install_bytes(bytes: &[u8]) -> Result<(PathBuf, Info), String> {
    let folder = folder().ok_or_else(|| "Could not find where to keep Bibles on this computer.".to_owned())?;
    install_bytes_in(&folder, bytes)
}

fn install_bytes_in(folder: &Path, bytes: &[u8]) -> Result<(PathBuf, Info), String> {
    let bible = Bible::from_bytes(bytes)?;
    let name = file_name(&bible.info.id).ok_or_else(|| "This Bible file does not say which Bible it is.".to_owned())?;
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make the Bibles folder: {e}"))?;
    let path = folder.join(name);
    // Write beside it and rename, so a half-written file is never left under the real name.
    let partial = path.with_extension("pcbible.part");
    std::fs::write(&partial, bytes)
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the Bible: {e}"))?;
    Ok((path, bible.info))
}

/// Installs a Bible file the operator has, such as one dropped on the window.
pub fn install_file(path: &Path) -> Result<(PathBuf, Info), String> {
    let size = std::fs::metadata(path).map_err(|e| format!("Could not read that file: {e}"))?.len();
    if size > LARGEST {
        return Err("That file is too large to be a Bible.".to_owned());
    }
    install_bytes(&std::fs::read(path).map_err(|e| format!("Could not read that file: {e}"))?)
}

fn fetch(address: &str) -> Result<Vec<u8>, String> {
    let offline = |e: ureq::Error| format!("Could not download the Bible: {e}");
    let mut response = ureq::get(address).call().map_err(offline)?;
    response.body_mut().with_config().limit(LARGEST).read_to_vec().map_err(offline)
}

/// Fetches one Bible and installs it, if it is the file expected.
fn download_into(folder: &Path, address: &str, id: &str, expected_checksum: &str) -> Result<(PathBuf, Info), String> {
    let bytes = fetch(address)?;
    if checksum(&bytes) != expected_checksum {
        return Err("The downloaded Bible is not the file this version expects, so it was not installed.".to_owned());
    }
    // Checked before anything is written, so a wrong file leaves nothing behind.
    if Bible::read_info(&bytes)?.id != id {
        return Err("The downloaded file is a different Bible, so it was not installed.".to_owned());
    }
    install_bytes_in(folder, &bytes)
}

/// How a download ended: the Bible's short name for files, and the installed file or why not.
pub type Finished = (&'static str, Result<(PathBuf, Info), String>);

/// Downloads under way. Each runs on its own thread so the window never waits on the network.
pub struct Downloads {
    tx: Sender<Finished>,
    rx: Receiver<Finished>,
    busy: Vec<&'static str>,
}

impl Default for Downloads {
    fn default() -> Downloads {
        let (tx, rx) = channel();
        Downloads { tx, rx, busy: Vec::new() }
    }
}

impl std::fmt::Debug for Downloads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Downloads({:?})", self.busy)
    }
}

impl Downloads {
    pub fn is_busy(&self, id: &str) -> bool {
        self.busy.contains(&id)
    }

    /// Starts fetching a Bible. `wake` is called when it ends, to redraw the window.
    pub fn start(&mut self, id: &'static str, expected_checksum: &'static str, wake: impl FnOnce() + Send + 'static) {
        let Some(base) = download_base() else { return };
        if self.is_busy(id) {
            return;
        }
        self.busy.push(id);
        let tx = self.tx.clone();
        let address = format!("{}/{id}.pcbible", base.trim_end_matches('/'));
        let work = move || {
            let result = folder()
                .ok_or_else(|| "Could not find where to keep Bibles on this computer.".to_owned())
                .and_then(|folder| download_into(&folder, &address, id, expected_checksum));
            let _ = tx.send((id, result));
            wake();
        };
        if std::thread::Builder::new().name("bible download".to_owned()).spawn(work).is_err() {
            self.busy.retain(|busy| *busy != id);
        }
    }

    /// The downloads that have ended since the last look.
    pub fn finished(&mut self) -> Vec<Finished> {
        let done: Vec<Finished> = self.rx.try_iter().collect();
        for (id, _) in &done {
            self.busy.retain(|busy| busy != id);
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use pc_bible::Numbering;

    use super::*;

    fn small_bible(id: &str) -> Vec<u8> {
        let info = Info {
            id: id.to_owned(),
            name: "Test Bible".to_owned(),
            short: "TB".to_owned(),
            language: "en".to_owned(),
            terms: "Public domain".to_owned(),
            credit: String::new(),
            source: "made for a test".to_owned(),
            numbering: Numbering::English,
            loose: false,
        };
        Bible::from_verse_lines(info, "JOH 3:16 For God so loved the world", str::to_owned).to_bytes()
    }

    fn empty_folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("presentercraft-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    /// Answers one request on this computer with the given bytes, and returns its address.
    fn serve_once(body: Vec<u8>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/file.pcbible", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        address
    }

    #[test]
    fn a_file_name_comes_only_from_safe_letters() {
        assert_eq!(file_name("kjv").as_deref(), Some("kjv.pcbible"));
        assert_eq!(file_name("../../etc/passwd").as_deref(), Some("etcpasswd.pcbible"));
        assert_eq!(file_name("a/b\\c:d").as_deref(), Some("abcd.pcbible"));
        assert_eq!(file_name("..//"), None);
        assert_eq!(file_name(&"x".repeat(41)), None);
    }

    #[test]
    fn the_checksum_is_sha256_in_hex() {
        assert_eq!(checksum(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn a_bible_is_installed_under_its_own_name_and_rubbish_is_refused() {
        let folder = empty_folder("install");
        let (path, info) = install_bytes_in(&folder, &small_bible("tb")).unwrap();
        assert_eq!(path, folder.join("tb.pcbible"));
        assert_eq!(info.name, "Test Bible");
        assert!(Bible::from_bytes(&std::fs::read(&path).unwrap()).is_ok());
        assert!(install_bytes_in(&folder, b"not a bible").is_err());
        // A file that names itself with a path still lands inside the folder.
        let (path, _) = install_bytes_in(&folder, &small_bible("../escape")).unwrap();
        assert_eq!(path, folder.join("escape.pcbible"));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_download_is_installed_only_when_it_is_the_expected_file() {
        let folder = empty_folder("download");
        let bytes = small_bible("tb");
        let sum = checksum(&bytes);

        let (path, info) = download_into(&folder, &serve_once(bytes.clone()), "tb", &sum).unwrap();
        assert_eq!((path, info.id.as_str()), (folder.join("tb.pcbible"), "tb"));
        let _ = std::fs::remove_dir_all(&folder);

        // The wrong checksum: nothing is written.
        let wrong = download_into(&folder, &serve_once(bytes.clone()), "tb", &"0".repeat(64)).unwrap_err();
        assert!(wrong.contains("not the file this version expects"), "{wrong}");
        assert!(!folder.exists());

        // The right checksum for a different Bible than asked for: nothing is written.
        let other = download_into(&folder, &serve_once(bytes), "kjv", &sum).unwrap_err();
        assert!(other.contains("different Bible"), "{other}");
        assert!(!folder.exists());

        // Nobody answering: a plain sentence, not a crash.
        let nobody = download_into(&folder, "http://127.0.0.1:9/none.pcbible", "tb", &sum).unwrap_err();
        assert!(nobody.contains("Could not download"), "{nobody}");
    }
}
