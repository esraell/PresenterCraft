//! Audio: tracks to play, such as walk-in music, and the player that plays one at a time. The
//! list of tracks is kept in `audio.json`; the files stay where they are.

use std::path::PathBuf;
use std::time::Duration;

use pc_model::Uuid;
use serde::{Deserialize, Serialize};

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
}

impl Default for Player {
    fn default() -> Player {
        Player { stream: None, sink: None, track: None, volume: 1.0 }
    }
}

impl Player {
    /// Plays a track from its start, in place of whatever was playing.
    pub fn play(&mut self, track: &Track) -> Result<(), String> {
        use rodio::Source;
        self.stop();
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
        sink.append(decoder);
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

    /// Stops, and forgets the track.
    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.track = None;
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

    /// What is playing now. A track that has played to its end is forgotten.
    pub fn playing(&mut self) -> Playing {
        if self.sink.as_ref().is_some_and(rodio::Sink::empty) {
            self.stop();
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
        .map_err(|e| format!("{} is not audio PresenterCraft can play ({e}).", track.name))
}

/// Minutes and seconds: "3:07".
pub fn clock(time: Duration) -> String {
    let seconds = time.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("audio.json"))
}

pub fn load() -> Vec<Track> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(tracks: &[Track]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(tracks).unwrap_or_default())
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
    fn a_wave_file_is_read_with_its_length() {
        use rodio::Source;
        // Two seconds of silence at 8000 samples a second, one channel, 16 bits.
        let samples = 16_000u32;
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
