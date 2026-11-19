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

impl Default for Fonts {
    fn default() -> Fonts {
        Fonts::new()
    }
}
