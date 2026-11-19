//! Audio: playlists of tracks to play, such as walk-in music, and the player that plays one at a
//! time (crossfading from one to the next if asked). The playlists are kept in `audio.json`;
//! the files stay where they are.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sp_model::Uuid;

/// The file endings of audio the player can play, as the file chooser lists them.
pub const AUDIO_ENDINGS: [&str; 9] = ["mp3", "wav", "m4a", "aac", "flac", "ogg", "mp4", "aif", "aiff"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
}

impl Track {
    pub fn of(path: PathBuf) -> Track {
        let name = path.file_stem().map_or_else(|| "Track".to_owned(), |stem| stem.to_string_lossy().into_owned());
        Track { id: Uuid::new_v4(), name, path }
    }
}

/// A named list of tracks, such as "Walk-in" or "Offering".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: Uuid,
    pub name: String,
    pub tracks: Vec<Track>,
}

impl Playlist {
    pub fn new(name: String) -> Playlist {
        Playlist { id: Uuid::new_v4(), name, tracks: Vec::new() }
    }
}

/// Every audio playlist, the one shown, and how tracks follow one another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub playlists: Vec<Playlist>,
    #[serde(default)]
    pub current: usize,
    /// When a track ends, a random other track of its playlist plays next.
    #[serde(default)]
    pub shuffle: bool,
    /// When a track ends, the next one plays.
    #[serde(default)]
    pub through: bool,
    /// Seconds the next track fades in over the end of the one before; 0 for none.
    #[serde(default)]
    pub crossfade: f32,
}

impl Default for Library {
    fn default() -> Library {
        Library {
            playlists: vec![Playlist::new("Audio".to_owned())],
            current: 0,
            shuffle: false,
            through: false,
            crossfade: 0.0,
        }
    }
}

impl Library {
    /// Reads the file's words. A file from before playlists is a plain list of tracks.
    pub fn from_json(text: &str) -> Option<Library> {
        if let Ok(tracks) = serde_json::from_str::<Vec<Track>>(text) {
            return Some(Library {
                playlists: vec![Playlist { tracks, ..Playlist::new("Audio".to_owned()) }],
                ..Library::default()
            });
        }
        let mut library: Library = serde_json::from_str(text).ok()?;
        if library.playlists.is_empty() {
            library.playlists.push(Playlist::new("Audio".to_owned()));
        }
        library.current = library.current.min(library.playlists.len() - 1);
        Some(library)
    }

    /// The playlist shown. There is always one.
    pub fn shown(&self) -> &Playlist {
        &self.playlists[self.current.min(self.playlists.len() - 1)]
    }

    pub fn shown_mut(&mut self) -> &mut Playlist {
        let at = self.current.min(self.playlists.len() - 1);
        &mut self.playlists[at]
    }

    /// A track in any playlist.
    pub fn track(&self, id: Uuid) -> Option<&Track> {
        self.playlists.iter().flat_map(|playlist| playlist.tracks.iter()).find(|track| track.id == id)
    }

    /// The track to play after this one in its playlist: the next (back to the first after the
    /// last), or with shuffle a random other one. `pick` chooses a number below the one given.
    pub fn after(&self, id: Uuid, pick: impl FnOnce(usize) -> usize) -> Option<&Track> {
        let playlist = self.playlists.iter().find(|playlist| playlist.tracks.iter().any(|track| track.id == id))?;
        let at = playlist.tracks.iter().position(|track| track.id == id)?;
        let count = playlist.tracks.len();
        if count < 2 {
            return None;
        }
        let next = if self.shuffle { (at + 1 + pick(count - 1)) % count } else { (at + 1) % count };
        playlist.tracks.get(next)
    }

    /// A name no audio playlist has.
    pub fn new_name(&self) -> String {
        let taken = |name: &str| self.playlists.iter().any(|playlist| playlist.name == name);
        std::iter::once("New Playlist".to_owned())
            .chain((2..).map(|n| format!("New Playlist {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default()
    }
}

/// What the player is doing, for the Audio tab.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Playing {
    pub track: Option<Uuid>,
    pub position: Duration,
    pub length: Option<Duration>,
    pub paused: bool,
}

/// Plays one track at a time. The sound output is opened the first time a track is played, so a
/// computer that never plays audio never opens it.
pub struct Player {
    stream: Option<rodio::OutputStream>,
    sink: Option<rodio::Sink>,
    track: Option<(Uuid, Option<Duration>)>,
    volume: f32,
    /// Tracks fading out under the one now playing: each with when its fade began, and how long.
    fading: Vec<(rodio::Sink, Instant, Duration)>,
}

impl Default for Player {
    fn default() -> Player {
        Player { stream: None, sink: None, track: None, volume: 1.0, fading: Vec::new() }
    }
}

impl Player {
    /// Plays a track from its start, in place of whatever was playing.
    pub fn play(&mut self, track: &Track) -> Result<(), String> {
        self.play_fading(track, Duration::ZERO)
    }

    /// Plays a track from its start, fading it in over `fade` while the one playing fades out.
    pub fn play_fading(&mut self, track: &Track, fade: Duration) -> Result<(), String> {
        use rodio::Source;
        match self.sink.take().filter(|sink| !fade.is_zero() && !sink.empty() && !sink.is_paused()) {
            Some(old) => self.fading.push((old, Instant::now(), fade)),
            None => self.stop(),
        }
        self.track = None;
        if self.stream.is_none() {
            let mut stream = rodio::OutputStreamBuilder::open_default_stream()
                .map_err(|e| format!("Could not open the computer's sound output: {e}"))?;
            stream.log_on_drop(false);
            self.stream = Some(stream);
        }
        let Some(stream) = &self.stream else { return Err("No sound output.".to_owned()) };
        let decoder = open(track)?;
        let length = decoder.total_duration();
        let sink = rodio::Sink::connect_new(stream.mixer());
        sink.set_volume(self.volume);
        if fade.is_zero() {
            sink.append(decoder);
        } else {
            sink.append(decoder.fade_in(fade));
        }
        self.sink = Some(sink);
        self.track = Some((track.id, length));
        Ok(())
    }

    /// Pauses, or goes on from where it was paused.
    pub fn toggle_pause(&mut self) {
        if let Some(sink) = &self.sink {
            if sink.is_paused() {
                sink.play();
            } else {
                sink.pause();
            }
        }
    }

    /// Stops, and forgets the track; tracks fading out stop too.
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        for (sink, _, _) in self.fading.drain(..) {
            sink.stop();
        }
        self.track = None;
    }

    /// Turns down the tracks fading out, and lets go of the ones done. True while any fade.
    pub fn tick(&mut self) -> bool {
        let volume = self.volume;
        self.fading.retain(|(sink, began, fade)| {
            let t = began.elapsed().as_secs_f32() / fade.as_secs_f32().max(0.001);
            if t >= 1.0 || sink.empty() {
                sink.stop();
                return false;
            }
            sink.set_volume(volume * (1.0 - t));
            true
        });
        !self.fading.is_empty()
    }

    /// How long the track playing has left, when its length is known.
    pub fn left(&self) -> Option<Duration> {
        let (_, length) = self.track?;
        let sink = self.sink.as_ref()?;
        length.map(|length| length.saturating_sub(sink.get_pos()))
    }

    /// Jumps by some seconds, forward or back, within the track.
    pub fn skip(&mut self, seconds: f32) {
        if let Some(sink) = &self.sink {
            let at = sink.get_pos().as_secs_f32() + seconds;
            let _ = sink.try_seek(Duration::from_secs_f32(at.max(0.0)));
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.5);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    /// The track that has just played to its end, if one has; it is forgotten.
    pub fn ended(&mut self) -> Option<Uuid> {
        if self.sink.as_ref().is_some_and(rodio::Sink::empty) {
            let ended = self.track.map(|(id, _)| id);
            if let Some(sink) = self.sink.take() {
                sink.stop();
            }
            self.track = None;
            return ended;
        }
        None
    }

    /// What is playing now. A track that has played to its end is forgotten.
    pub fn playing(&mut self) -> Playing {
        if self.sink.as_ref().is_some_and(rodio::Sink::empty) {
            if let Some(sink) = self.sink.take() {
                sink.stop();
            }
            self.track = None;
        }
        match (&self.sink, self.track) {
            (Some(sink), Some((id, length))) => {
                Playing { track: Some(id), position: sink.get_pos(), length, paused: sink.is_paused() }
            }
            _ => Playing::default(),
        }
    }
}

type FileDecoder = rodio::Decoder<std::io::BufReader<std::fs::File>>;

/// Opens a track's file and reads its sound, saying in words what went wrong if it can't.
fn open(track: &Track) -> Result<FileDecoder, String> {
    let file = std::fs::File::open(&track.path).map_err(|e| format!("Could not open {}: {e}", track.name))?;
    rodio::Decoder::new(std::io::BufReader::new(file))
        .map_err(|e| format!("{} is not audio SoftPresenter can play ({e}).", track.name))
}

/// Minutes and seconds: "3:07".
pub fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("audio.json"))
}

pub fn load() -> Library {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| Library::from_json(&text))
        .unwrap_or_default()
}

pub fn save(library: &Library) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(library).unwrap_or_default())
        .map_err(|e| format!("Could not save the audio list: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_named_by_their_file_and_a_missing_file_says_so() {
        let track = Track::of(PathBuf::from("/music/Walk In.mp3"));
        assert_eq!(track.name, "Walk In");
        assert_eq!(clock(Duration::from_secs(187)), "3:07");
        // Nothing is played, so no sound output is opened; a missing file fails in words.
        let mut player = Player::default();
        assert_eq!(player.playing(), Playing::default());
        let missing = Track::of(PathBuf::from("/nowhere/none.mp3"));
        assert!(open(&missing).err().unwrap_or_default().contains("Could not open none"));
    }

    #[test]
    fn an_old_list_loads_as_one_playlist_and_tracks_follow_in_order_or_at_random() {
        let one = Track::of(PathBuf::from("/m/One.mp3"));
        let two = Track::of(PathBuf::from("/m/Two.mp3"));
        let three = Track::of(PathBuf::from("/m/Three.mp3"));
        let old = serde_json::to_string(&vec![one.clone(), two.clone(), three.clone()]).unwrap();
        let mut library = Library::from_json(&old).unwrap();
        assert_eq!((library.playlists.len(), library.shown().tracks.len()), (1, 3));
        assert_eq!(library.after(one.id, |_| 0).map(|t| t.id), Some(two.id));
        assert_eq!(library.after(three.id, |_| 0).map(|t| t.id), Some(one.id), "after the last, the first");
        library.shuffle = true;
        // Shuffle never picks the same track again.
        for pick in 0..2 {
            assert_ne!(library.after(two.id, |_| pick).map(|t| t.id), Some(two.id));
        }
        library.playlists.push(Playlist {
            tracks: vec![Track::of(PathBuf::from("/m/Solo.mp3"))],
            ..Playlist::new(library.new_name())
        });
        assert_eq!(library.playlists[1].name, "New Playlist");
        let solo = library.playlists[1].tracks[0].id;
        assert!(library.after(solo, |_| 0).is_none(), "a playlist of one has nothing after");
        assert_eq!(Library::from_json(&serde_json::to_string(&library).unwrap()), Some(library));
    }

    /// Plays a quiet file at no volume and checks the player moves through it, pauses and stops.
    /// Needs a sound output, so it is run by hand: `cargo test -p sp-app plays_ -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn plays_pauses_and_stops_a_track() {
        let path = std::env::temp_dir().join(format!("pc-play-{}.wav", Uuid::new_v4()));
        std::fs::write(&path, quiet_wav(8000 * 3)).unwrap();
        let track = Track::of(path.clone());
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play(&track).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        let playing = player.playing();
        assert_eq!(playing.track, Some(track.id));
        assert!(playing.position > Duration::from_millis(200), "{:?}", playing.position);
        assert!(playing.length.is_some_and(|length| (length.as_secs_f32() - 3.0).abs() < 0.1));
        player.toggle_pause();
        let paused_at = player.playing().position;
        std::thread::sleep(Duration::from_millis(300));
        assert!(player.playing().paused);
        assert!(player.playing().position.abs_diff(paused_at) < Duration::from_millis(60));
        player.stop();
        assert_eq!(player.playing(), Playing::default());
        std::fs::remove_file(path).unwrap();
    }

    /// Plays a track, then crossfades into another: both play during the fade, then only the
    /// second. Needs a sound output: `cargo test -p sp-app crossfades -- --ignored`.
    #[test]
    #[ignore = "needs the computer's sound output"]
    fn crossfades_into_the_next_track() {
        let write = |seconds: u32| {
            let path = std::env::temp_dir().join(format!("pc-fade-{}.wav", Uuid::new_v4()));
            std::fs::write(&path, quiet_wav(8000 * seconds)).unwrap();
            path
        };
        let (first, second) = (Track::of(write(4)), Track::of(write(4)));
        let mut player = Player::default();
        player.set_volume(0.0);
        player.play(&first).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        player.play_fading(&second, Duration::from_millis(500)).unwrap();
        assert!(player.tick(), "the first is fading out");
        assert_eq!(player.playing().track, Some(second.id));
        std::thread::sleep(Duration::from_millis(650));
        assert!(!player.tick(), "the fade is over");
        assert_eq!(player.playing().track, Some(second.id));
        assert!(player.left().is_some_and(|left| left < Duration::from_secs(4)));
        player.stop();
        for track in [first, second] {
            std::fs::remove_file(track.path).unwrap();
        }
    }

    /// A wave file of silence: one channel, 16 bits, 8000 samples a second.
    fn quiet_wav(samples: u32) -> Vec<u8> {
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&16_000u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(samples * 2).to_le_bytes());
        wav.resize(wav.len() + samples as usize * 2, 0);
        wav
    }

    #[test]
    fn a_wave_file_is_read_with_its_length() {
        use rodio::Source;
        // Two seconds of silence.
        let wav = quiet_wav(16_000);
        let dir = std::env::temp_dir().join(format!("pc-audio-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Quiet.wav");
        std::fs::write(&path, wav).unwrap();
        let decoder = open(&Track::of(path)).unwrap();
        let length = decoder.total_duration().unwrap();
        assert!((length.as_secs_f32() - 2.0).abs() < 0.05, "{length:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
