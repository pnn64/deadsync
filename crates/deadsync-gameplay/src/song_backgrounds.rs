//! Authored background timing for an active song.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use deadsync_rules::timing::TimingData;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLayer2Flash {
    White,
    Yellow,
}

impl SongLayer2Flash {
    fn from_target(target: &SongBackgroundChangeTarget) -> Option<Self> {
        let SongBackgroundChangeTarget::Animation(name) = target else {
            return None;
        };
        if name.eq_ignore_ascii_case("white flash") {
            Some(Self::White)
        } else if name.eq_ignore_ascii_case("yellow flash") {
            Some(Self::Yellow)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
struct Layer2Event {
    start_second: f32,
    flash: Option<SongLayer2Flash>,
}

/// Song-lifetime timing and cursors shared by Gameplay and Practice. The
/// primary cursor advances by beat, while the layer-2 lookup can follow the
/// display clock in either direction. Both reuse precomputed song seconds.
#[derive(Clone, Debug, Default)]
pub struct SongBackgroundTimeline {
    changes: Vec<SongBackgroundChange>,
    start_seconds: Vec<f32>,
    next_change: usize,
    layer2: Vec<Layer2Event>,
    next_layer2: Cell<usize>,
}

impl SongBackgroundTimeline {
    #[must_use]
    pub fn new(
        changes: Vec<SongBackgroundChange>,
        layer2_changes: &[SongBackgroundChange],
        timing: &TimingData,
        current_beat: f32,
        display_second: f32,
    ) -> Self {
        let layer2: Vec<_> = layer2_changes
            .iter()
            .map(|change| Layer2Event {
                start_second: timing.get_time_for_beat(change.start_beat),
                flash: SongLayer2Flash::from_target(&change.target),
            })
            .collect();
        let next_layer2 = layer2.partition_point(|event| event.start_second <= display_second);
        let mut timeline = Self {
            layer2,
            next_layer2: Cell::new(next_layer2),
            ..Self::default()
        };
        timeline.replace_primary(changes, timing, current_beat);
        timeline
    }

    pub(crate) fn replace_primary(
        &mut self,
        changes: Vec<SongBackgroundChange>,
        timing: &TimingData,
        current_beat: f32,
    ) {
        self.next_change = changes
            .iter()
            .take_while(|change| change.start_beat <= current_beat)
            .count();
        self.start_seconds = changes
            .iter()
            .map(|change| timing.get_time_for_beat(change.start_beat))
            .collect();
        self.changes = changes;
    }

    #[must_use]
    pub fn changes(&self) -> &[SongBackgroundChange] {
        &self.changes
    }

    /// Return whether the primary cue changed, including jumps across cues.
    #[must_use]
    pub fn advance_to_beat(&mut self, beat: f32) -> bool {
        let previous = self.next_change;
        while let Some(change) = self.changes.get(self.next_change) {
            if beat < change.start_beat {
                break;
            }
            self.next_change += 1;
        }
        self.next_change != previous
    }

    #[must_use]
    pub fn active_change(&self) -> Option<&SongBackgroundChange> {
        self.next_change.checked_sub(1).map(|ix| &self.changes[ix])
    }

    #[must_use]
    pub fn active_start_second(&self) -> Option<f32> {
        self.next_change
            .checked_sub(1)
            .map(|ix| self.start_seconds[ix])
    }

    #[must_use]
    pub fn active_path<'a>(&'a self, song: &'a SongData, allow_video: bool) -> Option<&'a PathBuf> {
        song.gameplay_background_path_for_changes(&self.changes, self.next_change, allow_video)
    }

    /// A fallback image or video has no authored file start on this cue.
    #[must_use]
    pub fn video_start_second(&self, current_path: Option<&Path>) -> Option<f32> {
        let SongBackgroundChangeTarget::File(path) = &self.active_change()?.target else {
            return None;
        };
        if current_path == Some(path.as_path()) {
            self.active_start_second()
        } else {
            None
        }
    }

    /// Sample the latest layer-2 cue and its age in the display clock.
    #[must_use]
    pub fn active_layer2_flash(&self, now: f32) -> Option<(SongLayer2Flash, f32)> {
        if !now.is_finite() {
            return None;
        }
        let mut next = self.next_layer2.get();
        while next < self.layer2.len() && self.layer2[next].start_second <= now {
            next += 1;
        }
        while next > 0 && self.layer2[next - 1].start_second > now {
            next -= 1;
        }
        self.next_layer2.set(next);
        let event = self.layer2.get(next.checked_sub(1)?)?;
        Some((event.flash?, now - event.start_second))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::TimingSegments;

    fn timing() -> TimingData {
        let segments = TimingSegments {
            bpms: vec![(0.0, 120.0), (4.0, 60.0)],
            ..TimingSegments::default()
        };
        TimingData::from_segments(0.0, 0.125, &segments, &[])
    }

    #[test]
    fn primary_cues_use_the_cached_start_and_preserve_the_forward_cursor() {
        let timing = timing();
        let first = PathBuf::from("first.mp4");
        let second = PathBuf::from("second.mp4");
        let mut first_cue =
            SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::File(first.clone()));
        first_cue.rate = 1.5;
        let second_cue =
            SongBackgroundChange::new(5.0, SongBackgroundChangeTarget::File(second.clone()));
        let mut timeline =
            SongBackgroundTimeline::new(vec![first_cue, second_cue], &[], &timing, -1.0, -1.0);

        assert!(timeline.active_change().is_none());
        assert_eq!(timeline.active_start_second(), None);
        assert!(timeline.advance_to_beat(0.0));
        assert_eq!(timeline.active_change().unwrap().rate, 1.5);
        assert_eq!(
            timeline.video_start_second(Some(&first)),
            Some(timing.get_time_for_beat(0.0))
        );
        assert_eq!(timeline.video_start_second(Some(&second)), None);
        assert!(!timeline.advance_to_beat(4.999));
        assert!(timeline.advance_to_beat(5.0));
        assert!(!timeline.advance_to_beat(0.0));
        assert_eq!(
            timeline.video_start_second(Some(&second)),
            Some(timing.get_time_for_beat(5.0))
        );
    }

    #[test]
    fn layer2_sampling_can_rewind_and_unknown_cues_supersede_flashes() {
        let timing = timing();
        let layer2 = [
            (0.0, "WHITE FLASH"),
            (0.5, "unknown"),
            (5.0, "yellow flash"),
        ]
        .map(|(beat, name)| {
            SongBackgroundChange::new(beat, SongBackgroundChangeTarget::Animation(name.to_owned()))
        });
        let white = timing.get_time_for_beat(0.0);
        let unknown = timing.get_time_for_beat(0.5);
        let yellow = timing.get_time_for_beat(5.0);
        let timeline = SongBackgroundTimeline::new(Vec::new(), &layer2, &timing, 0.0, yellow);

        assert_eq!(
            timeline.active_layer2_flash(yellow),
            Some((SongLayer2Flash::Yellow, 0.0))
        );
        assert_eq!(timeline.active_layer2_flash(unknown), None);
        assert_eq!(
            timeline.active_layer2_flash(white + 0.125),
            Some((SongLayer2Flash::White, 0.125))
        );
        assert_eq!(timeline.active_layer2_flash(f32::NAN), None);
        assert_eq!(timeline.active_layer2_flash(white - 0.125), None);
        assert_eq!(
            timeline.active_layer2_flash(yellow + 0.25),
            Some((SongLayer2Flash::Yellow, 0.25))
        );
    }
}
