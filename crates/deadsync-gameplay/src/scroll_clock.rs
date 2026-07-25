/// Visual clock used for scrolling note placement.
///
/// Judgments, replay, autoplay, and other gameplay decisions always use the
/// raw audio-derived song clock regardless of this setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayNoteScrollClock {
    #[default]
    RawAudio,
    ItgDeStepped,
}

/// ITG-style visual correction for duplicate audio positions.
///
/// Audio drivers can publish the exact same position across multiple render
/// frames. In the experimental mode, repeated samples continue from the last
/// raw position using elapsed host time. As soon as the raw clock changes, the
/// visual clock snaps back to that raw position and establishes a new anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameplayNoteScrollClockState {
    last_raw_time_ns: SongTimeNs,
    raw_anchor_host_nanos: u64,
}

impl GameplayNoteScrollClockState {
    #[inline(always)]
    pub const fn new(raw_time_ns: SongTimeNs) -> Self {
        Self {
            last_raw_time_ns: raw_time_ns,
            raw_anchor_host_nanos: 0,
        }
    }

    #[inline(always)]
    pub fn reset(&mut self, raw_time_ns: SongTimeNs) -> SongTimeNs {
        self.last_raw_time_ns = raw_time_ns;
        self.raw_anchor_host_nanos = 0;
        raw_time_ns
    }

    #[inline(always)]
    pub fn step(
        &mut self,
        mode: GameplayNoteScrollClock,
        raw_time_ns: SongTimeNs,
        at_host_nanos: u64,
        seconds_per_second: f32,
    ) -> SongTimeNs {
        if mode == GameplayNoteScrollClock::RawAudio
            || song_time_ns_invalid(raw_time_ns)
            || at_host_nanos == 0
            || raw_time_ns != self.last_raw_time_ns
            || self.raw_anchor_host_nanos == 0
            || at_host_nanos < self.raw_anchor_host_nanos
        {
            self.last_raw_time_ns = raw_time_ns;
            self.raw_anchor_host_nanos = at_host_nanos;
            return raw_time_ns;
        }

        let elapsed_host_nanos =
            i128::from(at_host_nanos.saturating_sub(self.raw_anchor_host_nanos));
        clamp_song_time_ns(
            i128::from(raw_time_ns)
                + scaled_song_delta_ns(
                    elapsed_host_nanos,
                    normalized_song_rate(seconds_per_second),
                ),
        )
    }
}

