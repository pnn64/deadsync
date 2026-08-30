use crate::act;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_width, widescale};
use std::sync::Arc;

// --- Constants to match StepMania's SystemMessage display ---
const FADE_IN_DURATION: f32 = 0.0; // SM appears instantly
const HOLD_DURATION: f32 = 3.33;
const FADE_OUT_DURATION: f32 = 0.25;

const TEXT_MARGIN_X: f32 = 10.0;
const TEXT_MARGIN_Y: f32 = 10.0; // from top of bar

pub struct Params {
    pub message: Arc<str>,
}

fn message_salt(message: &str) -> u64 {
    let mut salt = 0xcbf2_9ce4_8422_2325_u64;
    for &b in message.as_bytes() {
        salt ^= u64::from(b);
        salt = salt.wrapping_mul(0x0100_0000_01b3);
    }
    salt
}

/// Builds the actors for a temporary system message overlay at the top of the screen.
/// The actors manage their own lifecycle (fade-in, hold, fade-out) via tweens.
#[must_use]
pub fn build(params: Params) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(2);
    push(&mut actors, params);
    actors
}

/// Appends a system-message overlay using text retained by the interaction state.
pub fn push(actors: &mut Vec<Actor>, params: Params) {
    let salt = message_salt(&params.message);
    push_with_content(actors, salt, TextContent::Shared(params.message));
}

fn push_with_content(actors: &mut Vec<Actor>, salt: u64, content: TextContent) {
    actors.reserve(2);

    let bg_color = color::rgba_hex("#000000");

    // The Lua code scales the background height based on the text height, which in turn
    // is scaled by the aspect ratio. We will replicate this.
    let text_zoom = widescale(0.8, 1.0);
    // Approximate the final height of the text to scale the background correctly.
    // font `miso` has a cap height around 18-20 logical units. 20 * zoom is a safe bet.
    let approx_text_height = 20.0 * text_zoom;
    let final_bar_h = approx_text_height + 16.0; // Matches `bmt:GetHeight() + 16` logic
    let bg = act!(quad:
        tweensalt(salt):
        align(0.5, 0.0):
        xy(screen_center_x(), 0.0):
        zoomto(screen_width(), final_bar_h): // Use calculated height
        diffuse(bg_color[0], bg_color[1], bg_color[2], 0.0):
        z(1000): // High Z-order to be on top of other UI

        // Animation sequence
        linear(FADE_IN_DURATION): alpha(0.85):
        sleep(HOLD_DURATION):
        linear(FADE_OUT_DURATION): alpha(0.0)
    );

    let text = act!(text:
        tweensalt(salt):
        font("miso"):
        settext(content):
        align(0.0, 0.0): // top-left
        xy(TEXT_MARGIN_X, TEXT_MARGIN_Y):
        zoom(text_zoom): // Apply the widescale zoom
        diffusealpha(0.0):
        z(1001): // Above the background quad

        // Animation sequence, synced with the background
        linear(FADE_IN_DURATION): alpha(1.0):
        sleep(HOLD_DURATION):
        linear(FADE_OUT_DURATION): alpha(0.0)
    );

    actors.push(bg);
    actors.push(text);
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn benchmark_build_owned(message: &str) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(2);
    push_with_content(
        &mut actors,
        message_salt(message),
        TextContent::Owned(message.to_owned()),
    );
    actors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_system_message_matches_owned_batch_and_shares_source_text() {
        let message: Arc<str> = Arc::from("Benchmark gamepad connected");
        let mut legacy = benchmark_build_owned(&message);
        let Actor::Text { content, .. } = &mut legacy[1] else {
            panic!("legacy message should contain a text actor");
        };
        *content = TextContent::Shared(Arc::clone(&message));

        let mut direct = Vec::with_capacity(2);
        push(
            &mut direct,
            Params {
                message: Arc::clone(&message),
            },
        );
        assert_eq!(format!("{direct:#?}"), format!("{legacy:#?}"));

        let Actor::Text {
            content: TextContent::Shared(actual),
            ..
        } = &direct[1]
        else {
            panic!("direct message should share its source text");
        };
        assert!(Arc::ptr_eq(actual, &message));
    }
}
