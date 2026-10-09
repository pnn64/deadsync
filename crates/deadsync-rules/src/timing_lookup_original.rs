// Frozen implementations from main 59ea18661e1ee0a054405f77faf724c601d79b67.
use super::*;

impl TimingData {
    pub(super) fn get_beat_internal_original(
        &self,
        start: &mut GetBeatStarts,
        args: &mut GetBeatArgs,
        max_segment: usize,
    ) {
        let bpms = &self.bpms;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bpm = self.get_bpm_for_beat(note_row_to_beat(start.last_row));
        let mut bps = bpm / 60.0;
        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                *start,
                0.0,
                false,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            let time_to_next_event_ns = if start.is_warping {
                0
            } else {
                timing_ns_from_seconds(note_row_to_beat(event_row - start.last_row) / bps)
            };
            let next_event_time_ns = start.last_time_ns.saturating_add(time_to_next_event_ns);
            if args.elapsed_time_ns < next_event_time_ns {
                break;
            }
            start.last_time_ns = next_event_time_ns;

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bpm = bpms[start.bpm_idx].bpm;
                    bps = bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay | TimingEvent::StopDelay => {
                    let delay = delays[start.delay_idx];
                    let delay_end_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(delay.duration));
                    if args.elapsed_time_ns < delay_end_ns {
                        args.delay_out = true;
                        args.beat = delay.beat;
                        args.bpm_out = bpm;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time_ns = delay_end_ns;
                    start.delay_idx += 1;
                    curr_segment += 1;
                    if event_type == TimingEvent::Delay {
                        start.last_row = event_row;
                        continue;
                    }
                }
                TimingEvent::Stop => {
                    let stop = stops[start.stop_idx];
                    let stop_end_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(stop.duration));
                    if args.elapsed_time_ns < stop_end_ns {
                        args.freeze_out = true;
                        args.beat = stop.beat;
                        args.bpm_out = bpm;
                        start.last_row = event_row;
                        return;
                    }
                    start.last_time_ns = stop_end_ns;
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    args.warp_begin_out = event_row;
                    args.warp_dest_out = start.warp_destination;
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
        if args.elapsed_time_ns == INVALID_TIMING_NS {
            args.elapsed_time_ns = start.last_time_ns;
        }
        let delta_seconds = timing_ns_delta_seconds(args.elapsed_time_ns, start.last_time_ns);
        args.beat = delta_seconds.mul_add(bps, note_row_to_beat(start.last_row));
        args.bpm_out = bpm;
    }

    pub(super) fn get_elapsed_time_internal_mut_original(
        &self,
        start: &mut GetBeatStarts,
        beat: f32,
        max_segment: usize,
        continuous: bool,
    ) -> TimingNs {
        let bpms = &self.bpms;
        let warps = &self.warps;
        let stops = &self.stops;
        let delays = &self.delays;

        let mut curr_segment = start.bpm_idx + start.warp_idx + start.stop_idx + start.delay_idx;
        let mut bps = self.get_bpm_for_beat(note_row_to_beat(start.last_row)) / 60.0;
        let find_marker = beat < f32::MAX;
        let marker_beat = if continuous && beat.is_finite() {
            (beat * ROWS_PER_BEAT as f32).ceil() / ROWS_PER_BEAT as f32
        } else {
            beat
        };

        while curr_segment < max_segment {
            let mut event_row = i32::MAX;
            let mut event_type = TimingEvent::NotFound;
            find_event(
                &mut event_row,
                &mut event_type,
                *start,
                marker_beat,
                find_marker,
                bpms,
                warps,
                stops,
                delays,
            );
            if event_type == TimingEvent::NotFound {
                break;
            }
            // A fractional marker precedes every segment at the next row.
            // Earlier stops/delays/warps have already been processed, avoiding
            // the discontinuity smearing caused by interpolating row times.
            if continuous && beat < note_row_to_beat(event_row) {
                let delta = if start.is_warping {
                    0
                } else {
                    timing_ns_from_seconds((beat - note_row_to_beat(start.last_row)) / bps)
                };
                return start.last_time_ns.saturating_add(delta);
            }
            let time_to_next_event_ns = if start.is_warping {
                0
            } else {
                timing_ns_from_seconds(note_row_to_beat(event_row - start.last_row) / bps)
            };
            if event_type == TimingEvent::Marker {
                return start.last_time_ns.saturating_add(time_to_next_event_ns);
            }
            start.last_time_ns = start.last_time_ns.saturating_add(time_to_next_event_ns);

            match event_type {
                TimingEvent::WarpDest => start.is_warping = false,
                TimingEvent::Bpm => {
                    bps = bpms[start.bpm_idx].bpm / 60.0;
                    start.bpm_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Stop | TimingEvent::StopDelay => {
                    start.last_time_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(stops[start.stop_idx].duration));
                    start.stop_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Delay => {
                    start.last_time_ns = start
                        .last_time_ns
                        .saturating_add(timing_ns_from_seconds(delays[start.delay_idx].duration));
                    start.delay_idx += 1;
                    curr_segment += 1;
                }
                TimingEvent::Warp => {
                    start.is_warping = true;
                    let warp = warps[start.warp_idx];
                    let warp_sum = warp.length + warp.beat;
                    if warp_sum > start.warp_destination {
                        start.warp_destination = warp_sum;
                    }
                    start.warp_idx += 1;
                    curr_segment += 1;
                }
                _ => {}
            }
            start.last_row = event_row;
        }
        start.last_time_ns
    }

    #[inline(always)]
    pub(super) fn get_speed_multiplier_with_original(
        &self,
        beat: f32,
        time_ns: impl FnOnce() -> i64,
    ) -> f32 {
        if self.speeds.is_empty() {
            return 1.0;
        }
        let time_ns = time_ns();
        let pos = self.speeds.partition_point(|seg| seg.beat <= beat);
        let Some(i) = pos.checked_sub(1) else {
            let first = self.speeds[0];
            if beat.is_finite() && first.delay <= 0.0 {
                return first.ratio;
            }
            return 1.0;
        };
        let seg = self.speeds[i];
        // Construction and every offset update build one runtime per speed.
        let rt = self.speed_runtime[i];

        if time_ns >= rt.end_time_ns || seg.delay <= 0.0 {
            return seg.ratio;
        }
        if time_ns < rt.start_time_ns {
            return rt.prev_ratio;
        }
        let duration_seconds = timing_ns_delta_seconds(rt.end_time_ns, rt.start_time_ns);
        if duration_seconds <= 0.0 {
            return seg.ratio;
        }
        let progress = timing_ns_delta_seconds(time_ns, rt.start_time_ns) / duration_seconds;
        (seg.ratio - rt.prev_ratio).mul_add(progress, rt.prev_ratio)
    }
}
