// Frozen capture from 98d79dc83 (0.5.1683).
use super::*;
#[derive(Default)]
pub(super) struct ColumnSplineLane {
    pub(super) frames: Vec<deadsync_gameplay::SongLuaColumnSplineFrame>,
    // NaN coefficients are deliberately non-reflexive under PartialEq.
    // Cache this property once per solved buffer, including across metadata edits.
    reflexive: [bool; 2],
}

pub(super) fn captured_spline_matches(
    previous: &Option<deadsync_gameplay::SongLuaSplineData>,
    next: &Option<deadsync_gameplay::SongLuaSplineData>,
    reflexive: bool,
) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => {
            previous.constant == next.constant
                && previous.beats_per_t == next.beats_per_t
                && previous.receptor_t == next.receptor_t
                && previous.subtract_song_beat == next.subtract_song_beat
                && ((reflexive
                    && std::sync::Arc::ptr_eq(&previous.coefficients, &next.coefficients))
                    || previous.coefficients == next.coefficients)
        }
        (None, None) => true,
        _ => false,
    }
}

pub(super) fn captured_spline_reflexive(
    next: &Option<deadsync_gameplay::SongLuaSplineData>,
    previous: Option<&Option<deadsync_gameplay::SongLuaSplineData>>,
    previous_reflexive: bool,
) -> bool {
    let Some(next) = next else {
        return true;
    };
    if previous
        .and_then(Option::as_ref)
        .is_some_and(|previous| std::sync::Arc::ptr_eq(&previous.coefficients, &next.coefficients))
    {
        return previous_reflexive;
    }
    next.coefficients
        .iter()
        .flatten()
        .flatten()
        .all(|value| !value.is_nan())
}

#[derive(Default)]
/// Compile-lifetime scratch: one point buffer (at most 65536 coordinates)
/// and one sample buffer for the authored lanes. Coefficients are shared only
/// with the preceding lane frame; there is no global cache or eviction policy.
pub(super) struct ColumnSplineCapture {
    points: Vec<[f32; 3]>,
    sampled: Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
    pub(super) lanes: BTreeMap<(usize, usize), ColumnSplineLane>,
    pub(super) bytes: usize,
}

impl ColumnSplineCapture {
    pub(super) fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {
        crate::lua_util::spline_workspace_reader_perf::baseline::read_column_position_splines(
            lua,
            |player, column| {
                self.lanes
                    .get(&(player, column))
                    .and_then(|lane| lane.frames.last())
            },
            &mut self.points,
            &mut self.sampled,
        )?;
        for (player, column, mut frame) in self.sampled.drain(..) {
            let lane = self.lanes.entry((player, column)).or_default();
            let last = lane.frames.last();
            if last.is_some_and(|last| {
                captured_spline_matches(&last.position, &frame.position, lane.reflexive[0])
                    && captured_spline_matches(&last.zoom, &frame.zoom, lane.reflexive[1])
            }) {
                continue;
            }
            self.bytes += [&frame.position, &frame.zoom]
                .into_iter()
                .flatten()
                .map(|spline| std::mem::size_of_val(spline.coefficients.as_ref()))
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

    pub(super) fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {
        log::debug!(
            "Compiled Position splines: lanes={} frames={} coefficients_bytes={}",
            self.lanes.len(),
            self.lanes
                .values()
                .map(|lane| lane.frames.len())
                .sum::<usize>(),
            self.bytes
        );
        out.extend(
            self.lanes
                .into_iter()
                .filter_map(|((player, column), lane)| {
                    let frames = lane.frames;
                    frames
                        .iter()
                        .any(|frame| frame.position.is_some() || frame.zoom.is_some())
                        .then(|| deadsync_gameplay::SongLuaColumnSplineTrack {
                            player,
                            column,
                            time_offset: 0.0,
                            frames: frames.into(),
                        })
                }),
        );
    }
}
