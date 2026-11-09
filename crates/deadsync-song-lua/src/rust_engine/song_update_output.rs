//! Owned output for the existing update compiler's static-tree branch.
use super::*;
use crate::{SongLuaColumnOffsetBuildParams, SongLuaSpanMode, SongLuaTimeUnit};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct SongStaticUpdateOutput {
    pub eases: Vec<crate::SongLuaEaseWindow>,
    pub column_transforms: Vec<crate::SongLuaColumnOffsetWindow>,
    pub column_splines: Vec<deadsync_gameplay::SongLuaColumnSplineTrack>,
}
impl RustSongRuntime {
    /// Finish the static branch after the caller has captured splines at zero
    /// and selected the existing no-update/no-model (or empty duration) path.
    /// Keeping that initial capture separate preserves its order before the
    /// tree eligibility getters. Dynamic trees require chronological replay.
    pub fn compile_song_static_update_output(
        &mut self,
        context: &SongLuaCompileContext,
        spline_capture: SongColumnSplineCapture,
    ) -> Result<SongStaticUpdateOutput, String> {
        let mut options = self
            .actors
            .update_options(&mut self.vm)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let states = options
                .sample(&mut self.vm, false)
                .map_err(|e| e.to_string())?;
            let speeds = options
                .sample(&mut self.vm, true)
                .map_err(|e| e.to_string())?;
            let origin = context
                .song_timing
                .as_ref()
                .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0));
            let end = crate::update_function_end_beat(context);
            let limit = origin + song_elapsed_seconds_at(end, context) * song_music_rate(context);
            let unit = if context.song_timing.is_some() {
                SongLuaTimeUnit::Second
            } else {
                SongLuaTimeUnit::BeatClock
            };
            let mut output = SongStaticUpdateOutput::default();
            crate::perframe::push_update_mod_targets_with_key(
                &mut output.eases,
                origin,
                limit.max(origin.next_up()),
                &states,
                &states,
                &states,
                &speeds,
                None,
                &mut BTreeMap::new(),
                &mut (0, String::new()),
                unit,
            );
            let columns = self.read_song_note_column_transform_samples()?;
            crate::append_column_transform_windows_from_samples(
                &mut output.column_transforms,
                &columns,
                &columns,
                SongLuaColumnOffsetBuildParams {
                    unit,
                    start: origin,
                    limit: limit.max(origin).next_up() - origin,
                    span_mode: SongLuaSpanMode::Len,
                    easing: None,
                    sustain: None,
                    opt1: None,
                    opt2: None,
                },
            );
            output.column_splines = spline_capture.finish()?;
            Ok(output)
        })();
        options.release(&mut self.vm).map_err(|e| e.to_string())?;
        result
    }
}
