use crate::act;
use crate::core::gfx::BackendType;
use crate::core::space::{screen_height, screen_width};
use crate::ui::actors::Actor;

pub struct StutterEvent {
    pub timestamp_seconds: f32,
    pub frame_ms: f32,
    pub frame_multiple: f32,
    pub severity: u8,
    pub age_seconds: f32,
}

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

fn format_stutter_time(seconds: f32) -> String {
    let centi_total = (seconds.max(0.0) * 100.0).round() as u64;
    let minutes = centi_total / 6_000;
    let rem = centi_total % 6_000;
    let secs = rem / 100;
    let centis = rem % 100;
    format!("{minutes:02}:{secs:02}.{centis:02}")
}

fn stutter_color(severity: u8, age_seconds: f32) -> [f32; 4] {
    const STUTTER_FADE_SECONDS: f32 = 3.4;
    let alpha = (1.0 - age_seconds / STUTTER_FADE_SECONDS).clamp(0.0, 1.0);
    let rgb = match severity {
        1 => [1.0, 1.0, 1.0],
        2 => [1.0, 1.0, 0.0],
        _ => [1.0, 0.4, 0.4],
    };
    [rgb[0], rgb[1], rgb[2], alpha]
}

pub fn build_stutter(events: &[StutterEvent]) -> Vec<Actor> {
    if events.is_empty() {
        return Vec::new();
    }
    // Match ITG/Simply Love ScreenStatsOverlay skip box metrics:
    // SkipX=SCREEN_RIGHT-100, SkipY=SCREEN_BOTTOM-85, SkipWidth=190, SkipSpacingY=14.
    const SKIP_X_FROM_RIGHT: f32 = 100.0;
    const SKIP_Y_FROM_BOTTOM: f32 = 85.0;
    const SKIP_WIDTH: f32 = 190.0;
    const SKIP_SPACING_Y: f32 = 14.0;
    const SKIP_SLOTS: usize = 5;
    const EDGE_PAD_Y: f32 = 10.0;
    const TEXT_ZOOM: f32 = 1.0;
    const Z: i32 = 200;
    let w = screen_width();
    let h = screen_height();
    let skip_x = w - SKIP_X_FROM_RIGHT;
    let skip_y = h - SKIP_Y_FROM_BOTTOM;
    let half_h = (SKIP_SPACING_Y * SKIP_SLOTS as f32) * 0.5 + EDGE_PAD_Y;
    let top = skip_y - half_h;
    let bottom = skip_y + half_h;
    let mut actors = Vec::with_capacity(events.len() + 1);
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(skip_x - SKIP_WIDTH * 0.5, top):
        zoomto(SKIP_WIDTH, bottom - top):
        diffuse(0.0, 0.0, 0.0, 0.4):
        z(Z)
    ));
    let visible = events.len().min(SKIP_SLOTS);
    let line_top = top + EDGE_PAD_Y;
    let line_bottom = bottom - EDGE_PAD_Y;
    for (i, event) in events.iter().take(visible).enumerate() {
        // Match ScreenStatsOverlay's fixed 5-row lane geometry.
        let y = if SKIP_SLOTS == 1 {
            line_top
        } else {
            line_top + (line_bottom - line_top) * (i as f32 / (SKIP_SLOTS - 1) as f32)
        };
        let c = stutter_color(event.severity, event.age_seconds);
        let line = format!(
            "{}: {:.0}ms ({:.0})",
            format_stutter_time(event.timestamp_seconds),
            event.frame_ms.max(0.0),
            event.frame_multiple.max(0.0)
        );
        actors.push(act!(text:
            align(0.5, 0.0):
            xy(skip_x, y - 7.0):
            zoom(TEXT_ZOOM):
            shadowlength(0.0):
            diffuse(c[0], c[1], c[2], c[3]):
            font("miso"):
            settext(line):
            horizalign(center):
            z(Z + 1)
        ));
    }
    actors
}
