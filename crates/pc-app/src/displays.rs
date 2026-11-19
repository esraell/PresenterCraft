//! Which display the output goes on.

/// A connected display, as the window system reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Borderless fullscreen on the display with this index.
    Fullscreen(usize),
    /// An ordinary window, for a computer with one display.
    Windowed,
}

/// Picks where the output goes. `home` is the index of the display holding the home window.
///
/// A requested display is used if it exists. Otherwise the output takes the first display that
/// is not the home window's, and falls back to a window when there is no other display.
pub fn choose_output(displays: &[DisplayInfo], home: usize, requested: Option<usize>) -> Result<Placement, String> {
    if let Some(wanted) = requested {
        return if displays.iter().any(|d| d.index == wanted) {
            Ok(Placement::Fullscreen(wanted))
        } else {
            Err(format!("There is no display {wanted}; run with --list-displays."))
        };
    }
    Ok(displays
        .iter()
        .map(|d| d.index)
        .filter(|index| *index != home)
        .min()
        .map_or(Placement::Windowed, Placement::Fullscreen))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn displays(n: usize) -> Vec<DisplayInfo> {
        (0..n).map(|index| DisplayInfo { index, name: format!("d{index}"), x: 0, y: 0, w: 1920, h: 1080 }).collect()
    }

    #[test]
    fn one_display_is_windowed() {
        assert_eq!(choose_output(&displays(1), 0, None), Ok(Placement::Windowed));
    }

    #[test]
    fn two_displays_picks_the_other() {
        assert_eq!(choose_output(&displays(2), 0, None), Ok(Placement::Fullscreen(1)));
        assert_eq!(choose_output(&displays(2), 1, None), Ok(Placement::Fullscreen(0)));
    }

    #[test]
    fn three_displays_picks_lowest_other() {
        assert_eq!(choose_output(&displays(3), 1, None), Ok(Placement::Fullscreen(0)));
        assert_eq!(choose_output(&displays(3), 0, None), Ok(Placement::Fullscreen(1)));
    }

    #[test]
    fn request_is_honoured_even_if_home() {
        assert_eq!(choose_output(&displays(2), 0, Some(0)), Ok(Placement::Fullscreen(0)));
    }

    #[test]
    fn bad_request_is_an_error() {
        let err = choose_output(&displays(2), 0, Some(4)).unwrap_err();
        assert!(err.contains("no display 4"), "{err}");
    }

    #[test]
    fn no_displays_is_windowed() {
        assert_eq!(choose_output(&[], 0, None), Ok(Placement::Windowed));
    }
}
