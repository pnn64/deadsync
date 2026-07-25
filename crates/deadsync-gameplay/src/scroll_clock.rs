/// Visual clock used for scrolling note placement.
///
/// Judgments, replay, autoplay, and other gameplay decisions always use the
/// raw audio-derived song clock regardless of this setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameplayNoteScrollClock {
    #[default]
    RawAudio,
    FrameStable,
}

/// Select the timestamp used only for visual note travel.
///
/// The frame-stable value advances continuously and reconciles toward raw
/// audio time without ordinary backward corrections. The raw value remains
/// authoritative for every gameplay decision.
#[inline(always)]
pub const fn note_scroll_music_time_ns(
    mode: GameplayNoteScrollClock,
    raw_music_time_ns: SongTimeNs,
    frame_stable_music_time_ns: SongTimeNs,
) -> SongTimeNs {
    match mode {
        GameplayNoteScrollClock::RawAudio => raw_music_time_ns,
        GameplayNoteScrollClock::FrameStable => frame_stable_music_time_ns,
    }
}
