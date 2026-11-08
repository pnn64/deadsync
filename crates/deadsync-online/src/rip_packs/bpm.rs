//! A card's BPM: what SMO says the song's tempo is -- `150`, `150-190` -- and
//! how fast the song plays, for the card's colour.
//!
//! The pack page and `/api/chart` both print the song's BPM as text, one
//! tempo or a range, and a rip has both for free. A range says nothing of
//! which tempo the song keeps to, so the colour goes by the tempo its Expert
//! chart averages -- its measures over the song's length -- held to the
//! range: a half-time intro or a doubled break leaves it where most of the
//! song is. The colours are the ITGmania browser's: slow under 140, medium to
//! 180, fast from there.

/// What a card says of the song's tempo, and how fast the song plays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardBpm {
    /// `150`, `150-190`, `???` for a tempo the site hides; empty for none.
    pub text: String,
    /// The tempo the song plays at, for the colour; 0 for unknown or hidden.
    pub tempo: u32,
}

impl CardBpm {
    #[must_use]
    pub fn pace(&self) -> Pace {
        Pace::of(self.tempo)
    }
}

/// How fast a song plays, as its card colours it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Unknown,
    Slow,
    Medium,
    Fast,
}

impl Pace {
    #[must_use]
    pub fn of(tempo: u32) -> Self {
        match tempo {
            0 => Self::Unknown,
            1..140 => Self::Slow,
            140..180 => Self::Medium,
            _ => Self::Fast,
        }
    }
}

/// The card's BPM from SMO's text for it, the song's length in milliseconds
/// and its Expert chart's length in measures, the last two where they are
/// known.
#[must_use]
pub fn card_bpm(shown: &str, length_ms: Option<u32>, measures: Option<u32>) -> CardBpm {
    let shown = shown.trim();
    if shown == "*" || shown.contains('?') {
        return CardBpm {
            text: "???".to_owned(),
            tempo: 0,
        };
    }
    let tempos: Vec<u32> = shown
        .split('-')
        .filter_map(|part| part.trim().parse::<f64>().ok())
        .filter(|tempo| tempo.is_finite() && *tempo > 0.0 && *tempo < 100_000.0)
        .map(|tempo| tempo.round() as u32)
        .filter(|tempo| *tempo > 0)
        .collect();
    let (Some(&lo), Some(&hi)) = (tempos.iter().min(), tempos.iter().max()) else {
        return CardBpm::default();
    };
    if lo == hi {
        return CardBpm {
            text: lo.to_string(),
            tempo: lo,
        };
    }
    // the tempo it averages, where its length and its chart's are known;
    // the range's top otherwise, which a half-time section most often is
    let average = match (length_ms, measures) {
        (Some(length_ms), Some(measures)) if length_ms > 1000 && measures > 0 => {
            let minutes = f64::from(length_ms) / 60_000.0;
            Some((f64::from(measures) * 4.0 / minutes).round() as u32)
        }
        _ => None,
    };
    CardBpm {
        text: format!("{lo}-{hi}"),
        tempo: average.map_or(hi, |average| average.clamp(lo, hi)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bpm(text: &str, tempo: u32) -> CardBpm {
        CardBpm {
            text: text.to_owned(),
            tempo,
        }
    }

    #[test]
    fn one_tempo_is_what_it_says() {
        assert_eq!(card_bpm("145", None, None), bpm("145", 145));
        assert_eq!(card_bpm(" 149.99 ", None, None), bpm("150", 150));
        assert_eq!(
            card_bpm("185", Some(156_734), Some(10)),
            bpm("185", 185),
            "one tempo needs no average"
        );
    }

    #[test]
    fn a_range_plays_at_its_average_held_to_the_range() {
        // a half-time intro: 75-150, most of it at 150 -- 2:00 of 72 measures
        assert_eq!(card_bpm("75-150", Some(120_000), Some(72)).tempo, 144);
        // a gimmick spike: 10-315, averaging 188
        assert_eq!(card_bpm("10-315", Some(144_195), Some(113)).tempo, 188);
        // an average outside the range is held to it
        assert_eq!(card_bpm("120-130", Some(60_000), Some(50)).tempo, 130);
        assert_eq!(card_bpm("150-190", Some(60_000), Some(10)).tempo, 150);
        assert_eq!(card_bpm("150-190", None, None), bpm("150-190", 190));
        assert_eq!(
            card_bpm("150-190", Some(500), Some(10)).tempo,
            190,
            "no length to speak of"
        );
    }

    #[test]
    fn a_hidden_or_missing_tempo() {
        assert_eq!(card_bpm("???", None, None), bpm("???", 0));
        assert_eq!(card_bpm("*", None, None).text, "???");
        assert_eq!(card_bpm("", None, None), CardBpm::default());
        assert_eq!(card_bpm("n/a", None, None), CardBpm::default());
    }

    #[test]
    fn paces_follow_the_browsers_bands() {
        assert_eq!(Pace::of(0), Pace::Unknown);
        assert_eq!(Pace::of(139), Pace::Slow);
        assert_eq!(Pace::of(140), Pace::Medium);
        assert_eq!(Pace::of(179), Pace::Medium);
        assert_eq!(Pace::of(180), Pace::Fast);
        assert_eq!(card_bpm("200", None, None).pace(), Pace::Fast);
        assert_eq!(card_bpm("???", None, None).pace(), Pace::Unknown);
    }
}
