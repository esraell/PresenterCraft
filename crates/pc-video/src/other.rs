//! Other systems: videos are not played there yet.

use std::path::Path;
use std::time::Duration;

use crate::Frame;

const NOT_HERE: &str = "Videos play on macOS only, for now.";

pub struct Player;

impl Player {
    pub fn open(_path: &Path) -> Result<Player, String> {
        Err(NOT_HERE.to_owned())
    }
    pub fn frames(&self) -> Frames {
        Frames
    }
    pub fn play(&mut self) {}
    pub fn pause(&self) {}
    pub fn playing(&self) -> bool {
        false
    }
    pub fn seek(&mut self, _to: f64) {}
    pub fn position(&self) -> f64 {
        0.0
    }
    pub fn duration(&self) -> Option<f64> {
        None
    }
    pub fn set_looping(&mut self, _looping: bool) {}
    pub fn set_volume(&self, _volume: f32) {}
    pub fn ended(&self) -> bool {
        true
    }
    pub fn problem(&self) -> Option<String> {
        Some(NOT_HERE.to_owned())
    }
    pub fn tick(&mut self) -> Duration {
        Duration::from_secs(1)
    }
}

#[derive(Clone)]
pub struct Frames;

impl Frames {
    pub fn same(&self, _other: &Frames) -> bool {
        true
    }
    pub fn next(&self) -> Option<Frame> {
        None
    }
}

pub fn still(_path: &Path, _max_side: u32) -> Result<Frame, String> {
    Err(NOT_HERE.to_owned())
}

pub fn picture(_path: &Path, _max_side: u32) -> Result<Frame, String> {
    Err(NOT_HERE.to_owned())
}
