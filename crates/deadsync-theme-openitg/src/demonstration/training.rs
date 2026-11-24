use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_to_seconds};
use deadsync_rules::{note::Note, timing::TimingData};

use super::OpenItgTrainingFeedback;

struct TrainingRow {
    beat: f32,
    at: f32,
    panels: u8,
}

struct TrainingLightCue {
    start: f32,
    end: f32,
}

pub(crate) struct TrainingCues {
    rows: Vec<TrainingRow>,
    panels: [Vec<f32>; 4],
    preview: [Vec<TrainingLightCue>; 3],
}

impl TrainingCues {
    pub(crate) fn new(notes: &[Note], note_times: &[SongTimeNs], timing: &TimingData) -> Self {
        let mut rows: Vec<_> = notes
            .iter()
            .zip(note_times)
            .filter(|(note, _)| {
                note.can_be_judged
                    && note.column < 4
                    && matches!(
                        note.note_type,
                        NoteType::Tap | NoteType::Hold | NoteType::Roll
                    )
            })
            .map(|(note, &time)| TrainingRow {
                beat: note.beat,
                at: song_time_ns_to_seconds(time),
                panels: 1 << note.column,
            })
            .collect();
        rows.sort_unstable_by(|a, b| a.beat.total_cmp(&b.beat));
        rows.dedup_by(|next, row| {
            if next.beat != row.beat {
                return false;
            }
            row.panels |= next.panels;
            true
        });

        // OpenITG forecasts on the whole-beat messages. Map both ends through
        // the actual timeline so dots arrive in time across stops and BPM changes.
        let preview = std::array::from_fn(|index| {
            let beats_ahead = (index + 1) as f32;
            let ahead_beats = || {
                rows.iter()
                    .filter(|row| row.beat.fract() == 0.0)
                    .map(|row| row.beat - beats_ahead)
            };
            let mut cues: Vec<_> = timing
                .get_times_for_beats_exact(ahead_beats())
                .zip(timing.get_times_for_beats_exact(ahead_beats().map(|beat| beat + 1.0)))
                .map(|(start, end)| TrainingLightCue { start, end })
                .collect();
            cues.sort_unstable_by(|a, b| a.start.total_cmp(&b.start));
            cues
        });
        rows.sort_unstable_by(|a, b| a.at.total_cmp(&b.at).then(a.beat.total_cmp(&b.beat)));
        let panels = std::array::from_fn(|index| {
            rows.iter()
                .filter(|row| row.panels & (1 << index) != 0)
                .map(|row| row.at)
                .collect()
        });
        Self {
            rows,
            panels,
            preview,
        }
    }

    pub(crate) fn sample(&self, now: f32) -> OpenItgTrainingFeedback {
        let step = self.rows[..self.rows.partition_point(|row| row.at <= now)]
            .last()
            .map(|row| (row.panels, now - row.at));
        let mut feedback = OpenItgTrainingFeedback {
            step,
            panel_ages: self.panels.each_ref().map(|times| {
                times[..times.partition_point(|&time| time <= now)]
                    .last()
                    .map(|time| now - time)
            }),
            ..Default::default()
        };
        feedback.light_ages[0] = step.map(|(_, age)| age);
        for (index, cues) in self.preview.iter().enumerate() {
            if let Some(cue) = cues[..cues.partition_point(|cue| cue.start <= now)].last() {
                let age = now - cue.start;
                feedback.light_ages[index + 1] = Some(age);
                if now < cue.end {
                    feedback.dot_progress[index] = Some(age / (cue.end - cue.start));
                }
            }
        }
        feedback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::TimingSegments;

    #[test]
    fn training_cues_follow_cached_steps_and_preview_beats_across_a_bpm_change() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 60.0), (3.0, 120.0)],
                ..Default::default()
            },
            &[],
        );
        let note = |beat, column, note_type, is_fake| Note {
            participant: None,
            beat,
            quantization_idx: 0,
            column,
            note_type,
            row_index: (beat * 48.0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake,
            can_be_judged: !is_fake,
        };
        let cues = TrainingCues::new(
            &[
                note(3.75, 1, NoteType::Tap, false),
                note(4.0, 0, NoteType::Tap, false),
                note(4.0, 2, NoteType::Hold, false),
                note(4.0, 3, NoteType::Mine, false),
                note(4.0, 3, NoteType::Lift, false),
                note(4.0, 3, NoteType::Tap, true),
            ],
            &[
                3_375_000_000,
                3_510_000_000,
                3_510_000_000,
                3_500_000_000,
                3_500_000_000,
                3_500_000_000,
            ],
            &timing,
        );

        let preview = cues.sample(3.25);
        assert_eq!(preview.dot_progress, [Some(0.5), None, None]);
        assert!(preview.step.is_none());
        assert_eq!(cues.sample(3.505).step.unwrap().0, 0b0010);

        let jump = cues.sample(3.56);
        let (panels, age) = jump.step.unwrap();
        assert_eq!(panels, 0b0101);
        assert!((age - 0.05).abs() < 1e-6);
        assert_eq!(jump.light_ages[0], Some(age));
        assert_eq!(jump.panel_ages[0], Some(age));
        assert!((jump.panel_ages[1].unwrap() - 0.185).abs() < 1e-6);
        assert_eq!(jump.panel_ages[2], Some(age));
        assert!(jump.panel_ages[3].is_none());
    }
}
