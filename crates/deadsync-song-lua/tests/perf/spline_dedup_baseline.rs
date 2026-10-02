// Frozen capture from d209a1380; both variants use the same current reader.
use super::*;
#[derive(Default)]
/// Compile-lifetime scratch: one point buffer (at most 65536 coordinates)
/// and one sample buffer for the authored lanes. Coefficients are shared only
/// with the preceding lane frame; there is no global cache or eviction policy.
pub(super) struct ColumnSplineCapture {
    points: crate::lua_util::ColumnSplineReadScratch,
    sampled: Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
    pub(super) lanes: BTreeMap<(usize, usize), Vec<deadsync_gameplay::SongLuaColumnSplineFrame>>,
    pub(super) bytes: usize,
}

impl ColumnSplineCapture {
    pub(super) fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {
        crate::lua_util::read_column_position_splines(
            lua,
            |player, column| {
                self.lanes
                    .get(&(player, column))
                    .and_then(|frames| frames.last())
            },
            &mut self.points,
            &mut self.sampled,
        )?;
        for (player, column, mut frame) in self.sampled.drain(..) {
            let frames = self.lanes.entry((player, column)).or_default();
            if frames
                .last()
                .is_some_and(|last| last.position == frame.position && last.zoom == frame.zoom)
            {
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
            frame.second = second;
            frames.push(frame);
        }
        Ok(())
    }

    pub(super) fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {
        log::debug!(
            "Compiled Position splines: lanes={} frames={} coefficients_bytes={}",
            self.lanes.len(),
            self.lanes.values().map(Vec::len).sum::<usize>(),
            self.bytes
        );
        out.extend(
            self.lanes
                .into_iter()
                .filter_map(|((player, column), frames)| {
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
