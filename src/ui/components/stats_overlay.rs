use crate::act;
use crate::core::gfx::BackendType;
use crate::core::space::screen_width;
use crate::ui::actors::Actor;

/// Three-line stats: FPS, VPF, Backend — top-right, miso, white.
pub fn build(backend: BackendType, fps: f32, vpf: u32) -> Vec<Actor> {
    const MARGIN_X: f32 = -16.0;
    const MARGIN_Y: f32 = 16.0;

    let w = screen_width();

    // 1. Combine all stat lines into a single string with newlines.
    let stats_text = format!("{:.0} FPS\n{} VPF\n{}", fps.max(0.0), vpf, backend);

    // 2. Create a single text actor for the entire block.
    // The layout engine will handle the line breaks automatically.
    let overlay_actor = act!(text:
        align(1.0, 0.0): // Align the whole text block to its top-right corner
        xy(w + MARGIN_X, MARGIN_Y): // Position the block's top-right corner
        zoom(0.65):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        settext(stats_text): // Use the new multi-line string
        horizalign(right):   // Align each line of text to the right within the block
        z(200)
    );

    vec![overlay_actor]
}
