use crate::act;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::space::{screen_center_x, widescale};
use std::sync::Arc;

/// Retained elapsed-time text owned by one screen on the game thread.
///
/// The fixed-capacity value is warmed at screen initialization and compares one
/// integer on each synchronization. It allocates only when the visible second
/// changes, never misses or evicts, performs no locking or background work, and
/// is dropped with its screen. Focused key/format tests provide instrumentation;
/// worst-case synchronization is one short format and allocation.
#[derive(Clone, Debug)]
pub struct TimerText {
    second: u64,
    text: Arc<str>,
}

impl TimerText {
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
    pub const fn text(&self) -> &Arc<str> {
        &self.text
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

fn format_elapsed(second: u64) -> Arc<str> {
    let hours = second / 3600;
    let minutes = (second % 3600) / 60;
    let seconds = second % 60;
    if second < 3600 {
        format!("{minutes:02}:{seconds:02}").into()
    } else if second < 36000 {
        format!("{hours}:{minutes:02}:{seconds:02}").into()
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}").into()
    }
}

pub fn build_session(text: impl Into<TextContent>, machine_font: MachineFont) -> Actor {
    build_header_timer(text, screen_center_x(), machine_font)
}

pub fn build_gameplay(text: impl Into<TextContent>, machine_font: MachineFont) -> Actor {
    build_header_timer(
        text,
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
        let first = Arc::clone(timer.text());

        assert_eq!(timer.text().as_ref(), "00:59");
        assert!(!timer.sync(59.9));
        assert!(Arc::ptr_eq(timer.text(), &first));
        assert!(timer.sync(60.0));
        assert_eq!(timer.text().as_ref(), "01:00");
        assert!(!Arc::ptr_eq(timer.text(), &first));
    }

    #[test]
    fn timer_text_formats_hours_and_sanitizes_invalid_values() {
        assert_eq!(TimerText::new(3_600.0).text().as_ref(), "1:00:00");
        assert_eq!(TimerText::new(36_000.0).text().as_ref(), "10:00:00");
        for elapsed in [-1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(TimerText::new(elapsed).text().as_ref(), "00:00");
        }
    }
}
