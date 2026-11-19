//! Other systems: pictures in formats beyond the common few aren't read there yet.

use std::path::Path;

use crate::Frame;

pub fn picture(_path: &Path, _max_side: u32) -> Result<Frame, String> {
    Err("This picture format can be opened on macOS only, for now.".to_owned())
}
