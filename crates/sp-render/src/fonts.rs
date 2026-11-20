use parley::{FontContext, LayoutContext};

/// Font lookup and text-layout scratch space. Costly to create: make one per thread that
/// draws and keep it.
pub struct Fonts {
    pub(crate) font_cx: FontContext,
    pub(crate) layout_cx: LayoutContext<()>,
    /// The sizes text was last fitted to its boxes at, newest last: fitting tries size after
    /// size, laying the text out each time, and a slide behind a playing video is drawn again
    /// for every picture of it.
    fitted: Vec<Fitted>,
}

/// A size text was fitted to a box at: the same text, style and box always fit the same way.
struct Fitted {
    text: String,
    style: sp_model::BoxStyle,
    box_size: (f32, f32),
    size: f32,
}

/// How many fitted sizes are remembered (a few screens' worth of boxes, several slides each).
const MOST_FITTED: usize = 64;

impl Fonts {
    /// Uses the fonts installed on this computer.
    pub fn new() -> Fonts {
        Fonts { font_cx: FontContext::new(), layout_cx: LayoutContext::new(), fitted: Vec::new() }
    }

    /// The size this text was fitted to a box of this size at before, in this style.
    pub(crate) fn fitted_size(&mut self, text: &str, style: &sp_model::BoxStyle, box_size: (f32, f32)) -> Option<f32> {
        let at = self.fitted.iter().rposition(|f| f.box_size == box_size && f.text == text && f.style == *style)?;
        // Used again: moved to the end, so the ones not used for longest are let go first.
        let found = self.fitted.remove(at);
        let size = found.size;
        self.fitted.push(found);
        Some(size)
    }

    /// Remembers the size text was fitted to a box at.
    pub(crate) fn remember_fit(&mut self, text: &str, style: &sp_model::BoxStyle, box_size: (f32, f32), size: f32) {
        if self.fitted.len() >= MOST_FITTED {
            self.fitted.remove(0);
        }
        self.fitted.push(Fitted { text: text.to_owned(), style: style.clone(), box_size, size });
    }
}

impl Fonts {
    /// The names of the font families installed on this computer, sorted.
    pub fn family_names(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self.font_cx.collection.family_names().map(str::to_owned).collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup();
        names
    }
}

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::new()
    }
}
