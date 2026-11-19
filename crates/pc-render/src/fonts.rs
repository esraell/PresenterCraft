use parley::{FontContext, LayoutContext};

/// Font lookup and text-layout scratch space. Costly to create: make one per thread that
/// draws and keep it.
pub struct Fonts {
    pub(crate) font_cx: FontContext,
    pub(crate) layout_cx: LayoutContext<()>,
}

impl Fonts {
    /// Uses the fonts installed on this computer.
    pub fn new() -> Fonts {
        Fonts { font_cx: FontContext::new(), layout_cx: LayoutContext::new() }
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
