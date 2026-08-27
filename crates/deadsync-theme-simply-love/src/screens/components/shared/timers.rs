use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::{Actor, InlineText, TextContent};
use deadlib_present::space::{screen_center_x, widescale};
use std::sync::Arc;

const SESSION_LAYOUT_SLOT: u8 = 0;
const GAMEPLAY_LAYOUT_SLOT: u8 = 1;

/// Retained elapsed-time text owned by one screen on the game thread.
///
/// Common elapsed values remain inline and use one of two reusable layout slots.
/// The single-threaded value is warmed at screen initialization, compares one
/// integer per sync, performs no locking or background work, and is dropped with
/// its screen. A slot overwrites its prior layout when the visible second changes;
/// there is no scan or eviction. Only values too large for 14 inline bytes fall
/// back to shared heap text. Focused format tests and the screen allocation
/// benchmark instrument the path; worst-case normal synchronization is fixed-size
/// decimal formatting.
#[derive(Clone, Debug)]
pub struct TimerText {
    second: u64,
    text: TextContent,
}

impl TimerText {
    #[must_use]
    pub fn new(elapsed: f32) -> Self {
        let second = elapsed_second(elapsed);
        Self {
            second,
            text: format_elapsed(second),
        }
    }

    pub fn sync(&mut self, elapsed: f32) -> bool {
        let second = elapsed_second(elapsed);
        if second == self.second {
            return false;
        }
        self.second = second;
        self.text = format_elapsed(second);
        true
    }

    #[inline(always)]
    #[must_use]
    pub fn text(&self) -> &str {
        self.text.as_str()
    }

    #[inline(always)]
    fn content(&self, slot: u8) -> TextContent {
        self.text.clone().with_frame_inline_slot(slot)
    }
}

impl Default for TimerText {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[inline(always)]
fn elapsed_second(elapsed: f32) -> u64 {
    if elapsed.is_finite() && elapsed > 0.0 {
        elapsed as u64
    } else {
        0
    }
}

fn format_elapsed(second: u64) -> TextContent {
    let hours = second / 3600;
    let minutes = (second % 3600) / 60;
    let seconds = second % 60;
    let mut text = InlineText::new();
    let fits = if hours == 0 {
        push_two_digits(&mut text, minutes)
    } else {
        u32::try_from(hours).is_ok_and(|hours| text.push_u32(hours))
            && text.push_ascii(b':')
            && push_two_digits(&mut text, minutes)
    } && text.push_ascii(b':')
        && push_two_digits(&mut text, seconds);
    if fits {
        return TextContent::Inline(text);
    }
    TextContent::Shared(Arc::from(if second < 3600 {
        format!("{minutes:02}:{seconds:02}")
    } else if second < 36000 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }))
}

#[inline(always)]
fn push_two_digits(text: &mut InlineText, value: u64) -> bool {
    debug_assert!(value < 100);
    text.push_ascii(b'0' + (value / 10) as u8) && text.push_ascii(b'0' + (value % 10) as u8)
}

#[must_use]
pub fn build_session(timer: &TimerText, machine_font: MachineFont) -> Actor {
    build_header_timer(
        timer.content(SESSION_LAYOUT_SLOT),
        screen_center_x(),
        machine_font,
    )
}

#[must_use]
pub fn build_gameplay(timer: &TimerText, machine_font: MachineFont) -> Actor {
    build_header_timer(
        timer.content(GAMEPLAY_LAYOUT_SLOT),
        screen_center_x() + widescale(150.0, 200.0),
        machine_font,
    )
}

fn build_header_timer(text: impl Into<TextContent>, x: f32, machine_font: MachineFont) -> Actor {
    let text = text.into();
    act!(text:
        font(machine_font_key(machine_font, FontRole::Numbers)):
        settext(text):
        align(0.5, 0.5):
        xy(x, 10.0):
        zoom(widescale(0.3, 0.36)):
        z(121):
        diffuse(1.0, 1.0, 1.0, 1.0):
        horizalign(center)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_text_updates_only_at_visible_second_boundaries() {
        let mut timer = TimerText::new(59.1);

        assert_eq!(timer.text(), "00:59");
        assert!(!timer.sync(59.9));
        assert_eq!(timer.text(), "00:59");
        assert!(timer.sync(60.0));
        assert_eq!(timer.text(), "01:00");
    }

    #[test]
    fn timer_text_formats_hours_and_sanitizes_invalid_values() {
        assert_eq!(TimerText::new(3_600.0).text(), "1:00:00");
        assert_eq!(TimerText::new(36_000.0).text(), "10:00:00");
        for elapsed in [-1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(TimerText::new(elapsed).text(), "00:00");
        }
    }

    #[test]
    fn timer_text_preserves_large_elapsed_format_with_heap_fallback() {
        let text = format_elapsed(100_000_000_u64 * 3_600);

        assert_eq!(text.as_str(), "100000000:00:00");
        assert!(matches!(text, TextContent::Shared(_)));
    }

    #[test]
    fn timer_actor_content_uses_independent_reusable_slots() {
        let timer = TimerText::new(3_661.0);

        assert!(matches!(
            timer.content(SESSION_LAYOUT_SLOT),
            TextContent::FrameInline {
                slot: SESSION_LAYOUT_SLOT,
                ..
            }
        ));
        assert!(matches!(
            timer.content(GAMEPLAY_LAYOUT_SLOT),
            TextContent::FrameInline {
                slot: GAMEPLAY_LAYOUT_SLOT,
                ..
            }
        ));
    }
}
