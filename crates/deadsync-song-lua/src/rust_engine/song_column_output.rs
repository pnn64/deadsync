//! Existing column spline tracks, retained as owned compiler output.
use super::*;
use deadsync_gameplay::{
    SongLuaColumnSplineFrame as Frame, SongLuaColumnSplineTrack as Track,
    SongLuaSplineData as Spline, SongLuaSplineSolver,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Scratch {
    pub points: Vec<[f32; 3]>,
    pub solver: SongLuaSplineSolver,
    pub recent: Option<Arc<[[[f32; 4]; 3]]>>,
    pub recent_looping: bool,
}
#[derive(Default)]
pub(super) struct Lane {
    pub frames: Vec<Frame>,
    reflexive: [Option<bool>; 2],
}
#[derive(Default)]
pub(super) struct State {
    pub scratch: Scratch,
    pub sampled: Vec<(usize, usize, Frame)>,
    pub lanes: BTreeMap<(usize, usize), Lane>,
    bytes: usize,
}
/// Load-lifetime reader/solver buffers and owned tracks. Every coordinate is
/// validated on each sample; geometry reuse never relies on Lua table identity.
pub struct SongColumnSplineCapture {
    state: Arc<Mutex<Option<State>>>,
}
impl Default for SongColumnSplineCapture {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(Some(State::default()))),
        }
    }
}
impl SongColumnSplineCapture {
    pub fn finish(self) -> Result<Vec<Track>, String> {
        let state = self
            .state
            .lock()
            .unwrap()
            .take()
            .ok_or("column spline capture is active or finished")?;
        Ok(state.finish())
    }
}
impl RustSongRuntime {
    pub fn capture_song_column_splines(
        &mut self,
        capture: &mut SongColumnSplineCapture,
        second: f32,
    ) -> Result<(), String> {
        self.actors
            .capture_column_splines(&mut self.vm, capture.state.clone(), second)
            .map_err(|e| e.to_string())
    }
    pub fn read_song_note_column_zoom_hides(
        &mut self,
    ) -> Result<Vec<crate::SongLuaNoteHideWindow>, String> {
        self.actors
            .read_column_zoom_hides(&mut self.vm)
            .map_err(|e| e.to_string())
    }
}
fn captured_spline_matches(
    previous: &Option<Spline>,
    next: &Option<Spline>,
    reflexive: &mut Option<bool>,
) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => {
            if previous.constant != next.constant
                || previous.beats_per_t != next.beats_per_t
                || previous.receptor_t != next.receptor_t
                || previous.subtract_song_beat != next.subtract_song_beat
            {
                return false;
            }
            if Arc::ptr_eq(&previous.coefficients, &next.coefficients) {
                *reflexive.get_or_insert_with(|| {
                    previous
                        .coefficients
                        .iter()
                        .flatten()
                        .flatten()
                        .all(|value| !value.is_nan())
                })
            } else {
                previous.coefficients == next.coefficients
            }
        }
        (None, None) => true,
        _ => false,
    }
}
fn captured_spline_reflexive(
    next: &Option<Spline>,
    previous: Option<&Option<Spline>>,
    previous_reflexive: Option<bool>,
) -> Option<bool> {
    if previous
        .and_then(Option::as_ref)
        .zip(next.as_ref())
        .is_some_and(|(previous, next)| Arc::ptr_eq(&previous.coefficients, &next.coefficients))
    {
        previous_reflexive
    } else {
        None
    }
}
impl State {
    pub fn ingest(&mut self, second: f32) -> Result<(), String> {
        for (player, column, mut frame) in self.sampled.drain(..) {
            let lane = self.lanes.entry((player, column)).or_default();
            let last = lane.frames.last();
            if last.is_some_and(|last| {
                captured_spline_matches(&last.position, &frame.position, &mut lane.reflexive[0])
                    && captured_spline_matches(&last.zoom, &frame.zoom, &mut lane.reflexive[1])
            }) {
                continue;
            }
            self.bytes += [&frame.position, &frame.zoom]
                .into_iter()
                .flatten()
                .map(|s| std::mem::size_of_val(s.coefficients.as_ref()))
                .sum::<usize>();
            if self.bytes > 128 * 1024 * 1024 {
                return Err("Position spline tracks exceed 128 MiB per layer".into());
            }
            lane.reflexive = [
                captured_spline_reflexive(
                    &frame.position,
                    last.map(|last| &last.position),
                    lane.reflexive[0],
                ),
                captured_spline_reflexive(
                    &frame.zoom,
                    last.map(|last| &last.zoom),
                    lane.reflexive[1],
                ),
            ];
            frame.second = second;
            lane.frames.push(frame);
        }
        Ok(())
    }
    fn finish(self) -> Vec<Track> {
        log::debug!(
            "Compiled Position splines: lanes={} frames={} coefficients_bytes={}",
            self.lanes.len(),
            self.lanes
                .values()
                .map(|lane| lane.frames.len())
                .sum::<usize>(),
            self.bytes
        );
        self.lanes
            .into_iter()
            .filter_map(|((player, column), lane)| {
                let frames = lane.frames;
                frames
                    .iter()
                    .any(|f| f.position.is_some() || f.zoom.is_some())
                    .then(|| Track {
                        player,
                        column,
                        time_offset: 0.,
                        frames: frames.into(),
                    })
            })
            .collect()
    }
}
