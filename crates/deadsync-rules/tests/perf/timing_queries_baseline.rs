// Private BPM table initializers adapted to Arc slices for regression compatibility.
// Frozen from 5abf1dd0b (0.5.1684), with new metadata defaulted for layout compatibility.
use super::*;
impl TimingData {
    #[must_use]
    fn old_from_segments(
        song_offset_sec: f32,
        global_offset_sec: f32,
        segments: &TimingSegments,
        row_to_beat: &[f32],
    ) -> Self {
        let parsed_bpms: Cow<'_, [(f32, f32)]> = if segments.bpms.is_empty() {
            Cow::Borrowed(&[(0.0, 60.0)])
        } else if segments.bpms.windows(2).all(|pair| pair[0].0 <= pair[1].0) {
            Cow::Borrowed(&segments.bpms)
        } else {
            let mut sorted = segments.bpms.clone();
            sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Less));
            Cow::Owned(sorted)
        };

        let stops = sorted_timing_table(&segments.stops, |segment| segment.beat);
        let delays = sorted_timing_table(&segments.delays, |segment| segment.beat);
        let warps = sorted_timing_table(&segments.warps, |segment| segment.beat);
        let speeds = sorted_timing_table(&segments.speeds, |segment| segment.beat);
        let scrolls = sorted_timing_table(&segments.scrolls, |segment| segment.beat);
        let fakes = sorted_timing_table(&segments.fakes, |segment| segment.beat);

        let song_offset_sec = song_offset_sec + segments.beat0_offset_adjust;
        let song_offset_ns = timing_ns_from_seconds(song_offset_sec);
        let global_offset_ns = timing_ns_from_seconds(global_offset_sec);

        let mut beat_to_time = Vec::with_capacity(parsed_bpms.len());
        let mut current_time = 0.0;
        let mut last_beat = 0.0;
        let mut last_bpm = parsed_bpms[0].1;
        let mut max_bpm = 0.0;

        for &(beat, bpm) in parsed_bpms.as_ref() {
            if beat > last_beat && last_bpm > 0.0 {
                current_time = (beat - last_beat).mul_add(60.0 / last_bpm, current_time);
            }
            beat_to_time.push(BeatTimePoint {
                beat,
                time_ns: timing_ns_add_seconds(song_offset_ns, current_time),
                bpm,
            });
            if bpm.is_finite() && bpm > max_bpm {
                max_bpm = bpm;
            }
            last_beat = beat;
            last_bpm = bpm;
        }

        let mut timing_with_stops = Self {
            row_to_beat: Arc::new(row_to_beat.to_vec()),
            beat_to_time: Arc::from(beat_to_time),
            stops,
            delays,
            warps,
            speeds,
            scrolls,
            fakes,
            speed_runtime: Arc::default(),
            scroll_prefix: Arc::default(),
            pause_rows_sorted: [false; 2],
            scroll_prefix_sorted: false,
            global_offset_sec,
            global_offset_ns,
            max_bpm,
        };

        let mut beat_time_cache = BeatTimeCache::new(&timing_with_stops);
        // Time conversion reads each point's beat/BPM and the first point's
        // offset. Keep that offset unchanged until every conversion is done;
        // later timestamps can be overwritten in the uniquely owned buffer.
        let mut first_time_ns = 0;
        for index in 0..timing_with_stops.beat_to_time.len() {
            let beat = timing_with_stops.beat_to_time[index].beat;
            let time_ns =
                timing_with_stops.get_time_for_beat_internal_ns_cached(beat, &mut beat_time_cache);
            if index == 0 {
                first_time_ns = time_ns;
            } else {
                Arc::get_mut(&mut timing_with_stops.beat_to_time)
                    .expect("new timing points are uniquely owned")[index]
                    .time_ns = time_ns;
            }
        }
        Arc::get_mut(&mut timing_with_stops.beat_to_time)
            .expect("new timing points are uniquely owned")[0]
            .time_ns = first_time_ns;

        timing_with_stops.rebuild_speed_runtime();

        if !timing_with_stops.scrolls.is_empty() {
            let mut cum_displayed = 0.0_f32;
            let mut last_real_beat = 0.0_f32;
            let mut last_ratio = 1.0_f32;
            timing_with_stops.scroll_prefix = exact_arc(timing_with_stops.scrolls.len(), |index| {
                let seg = timing_with_stops.scrolls[index];
                cum_displayed = (seg.beat - last_real_beat).mul_add(last_ratio, cum_displayed);
                let prefix = ScrollPrefix {
                    beat: seg.beat,
                    cum_displayed,
                    ratio: seg.ratio,
                };
                last_real_beat = seg.beat;
                last_ratio = seg.ratio;
                prefix
            });
        }

        debug!("TimingData processed {} note rows.", row_to_beat.len());

        timing_with_stops
    }

    #[inline(always)]
    fn old_has_stop_or_delay_at_row(&self, row: i32) -> bool {
        self.stops.iter().any(|seg| {
            beat_to_note_row(seg.beat) == row && seg.duration.is_finite() && seg.duration != 0.0
        }) || self.delays.iter().any(|seg| {
            beat_to_note_row(seg.beat) == row && seg.duration.is_finite() && seg.duration != 0.0
        })
    }

    #[inline]
    fn old_get_displayed_beat_cached(&self, beat: f32, cache: &mut DisplayedBeatCache) -> f32 {
        if self.scroll_prefix.is_empty() {
            return beat;
        }
        if !beat.is_finite() {
            return self.get_displayed_beat(beat);
        }
        if cache.next_prefix > self.scroll_prefix.len()
            || (cache.initialized && beat < cache.last_beat)
        {
            cache.reset();
        }
        while cache.next_prefix < self.scroll_prefix.len()
            && self.scroll_prefix[cache.next_prefix].beat <= beat
        {
            cache.next_prefix += 1;
        }
        cache.last_beat = beat;
        cache.initialized = true;

        let prefix = if cache.next_prefix == 0 {
            self.scroll_prefix[0]
        } else {
            self.scroll_prefix[cache.next_prefix - 1]
        };
        (beat - prefix.beat).mul_add(prefix.ratio, prefix.cum_displayed)
    }
}
