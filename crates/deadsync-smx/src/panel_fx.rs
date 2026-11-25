use std::sync::Arc;

use deadsync_core::note::NoteType;

use crate::gifs::{GifRegistry, PadSize, PanelAnim};
use crate::panels::OverlayDrive;

/// Sentinel `*_at_screen_s` meaning "nothing seen yet for this column".
pub const NO_EVENT: f32 = f32::NEG_INFINITY;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TapLightEffect {
    FantasticBlue,
    FantasticWhite,
    Excellent,
    Great,
    Decent,
    WayOff,
    Miss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldLightEffect {
    Ok,
    Bad,
}

#[derive(Clone, Copy, Debug)]
pub struct TapLightJudgment {
    pub effect: TapLightEffect,
    pub at_screen_s: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HoldLightJudgment {
    /// A judgment without a light effect still advances the event tracker.
    pub effect: Option<HoldLightEffect>,
    pub at_screen_s: f32,
}

/// Resolved per-panel judgement animations from the GIF registry. A slot is
/// `None` only when nothing resolved for that name (the shipped default pack
/// covers every name, so in practice that means the pack opted out via
/// `CanBeEmpty` or the asset is missing): the event then shows no panel
/// effect at all; there is no solid-colour fallback.
#[derive(Default, Clone)]
pub struct JudgementGifs {
    pub fantastic_blue: Option<Arc<PanelAnim>>,
    pub fantastic_white: Option<Arc<PanelAnim>>,
    pub excellent: Option<Arc<PanelAnim>>,
    pub great: Option<Arc<PanelAnim>>,
    pub decent: Option<Arc<PanelAnim>>,
    pub way_off: Option<Arc<PanelAnim>>,
    pub miss: Option<Arc<PanelAnim>>,
    pub mine: Option<Arc<PanelAnim>>,
    /// Successful freeze/roll/lift release.
    pub ok: Option<Arc<PanelAnim>>,
    /// Failed (dropped) freeze/roll/lift.
    pub bad: Option<Arc<PanelAnim>>,
    /// Looping sustain while a freeze is engaged.
    pub freeze: Option<Arc<PanelAnim>>,
    /// Looping sustain while a roll is engaged.
    pub roll: Option<Arc<PanelAnim>>,
    /// Generic press feedback: a panel pressed with no note of its own. Drawn
    /// below the judgement and sustain layers, so a real hit overrides it.
    pub press: Option<Arc<PanelAnim>>,
}

impl JudgementGifs {
    /// Whether any animation at all resolved. `false` means no gameplay event on this
    /// pad can ever draw anything (an empty judgement pack), so the pad is left to its
    /// firmware rather than being owned and held black for the whole song.
    #[must_use]
    pub fn has_any(&self) -> bool {
        let Self {
            fantastic_blue,
            fantastic_white,
            excellent,
            great,
            decent,
            way_off,
            miss,
            mine,
            ok,
            bad,
            freeze,
            roll,
            press,
        } = self;
        [
            fantastic_blue,
            fantastic_white,
            excellent,
            great,
            decent,
            way_off,
            miss,
            mine,
            ok,
            bad,
            freeze,
            roll,
            press,
        ]
        .iter()
        .any(|slot| slot.is_some())
    }

    /// Resolve the standard judgement names from a registry through the usual
    /// pack-then-size fallback. `_25` is the baseline both pad layouts render.
    #[must_use]
    pub fn resolve(registry: &GifRegistry, pack: Option<&str>) -> Self {
        let j = |name: &str| registry.judgement(pack, name, PadSize::Leds25);
        Self {
            fantastic_blue: j("fantastic_blue"),
            fantastic_white: j("fantastic_white"),
            excellent: j("excellent"),
            great: j("great"),
            decent: j("decent"),
            way_off: j("way_off"),
            miss: j("miss"),
            mine: j("mine"),
            ok: j("ok"),
            bad: j("bad"),
            freeze: j("freeze"),
            roll: j("roll"),
            press: j("press"),
        }
    }

    #[must_use]
    pub(crate) const fn for_tap_effect(&self, effect: TapLightEffect) -> Option<&Arc<PanelAnim>> {
        match effect {
            TapLightEffect::FantasticBlue => self.fantastic_blue.as_ref(),
            TapLightEffect::FantasticWhite => self.fantastic_white.as_ref(),
            TapLightEffect::Excellent => self.excellent.as_ref(),
            TapLightEffect::Great => self.great.as_ref(),
            TapLightEffect::Decent => self.decent.as_ref(),
            TapLightEffect::WayOff => self.way_off.as_ref(),
            TapLightEffect::Miss => self.miss.as_ref(),
        }
    }

    #[must_use]
    pub(crate) const fn for_hold_effect(&self, effect: HoldLightEffect) -> Option<&Arc<PanelAnim>> {
        match effect {
            HoldLightEffect::Ok => self.ok.as_ref(),
            HoldLightEffect::Bad => self.bad.as_ref(),
        }
    }
}

/// The looping sustain animation for an engaged hold, by its note kind.
#[must_use]
pub const fn sustain_anim(gifs: &JudgementGifs, kind: Option<NoteType>) -> Option<&Arc<PanelAnim>> {
    match kind {
        Some(NoteType::Hold) => gifs.freeze.as_ref(),
        Some(NoteType::Roll) => gifs.roll.as_ref(),
        _ => None,
    }
}

/// The worker drive for a sustained hold's overlay, by note kind: a freeze
/// holds in its loop and plays the outro on release (`Sustain`), a roll runs
/// forward into the outro to show its continuous drain and resets on each step
/// (`Roll`). `resume` starts a re-triggered overlay at the loop region.
#[must_use]
pub const fn sustain_drive(kind: Option<NoteType>, resume: bool) -> OverlayDrive {
    match kind {
        Some(NoteType::Roll) => OverlayDrive::Roll { resume },
        _ => OverlayDrive::Sustain { resume },
    }
}

/// Ordered results-background role candidates for a finished chart, most
/// specific first. Each grade tier tries the difficulty-qualified role before
/// the difficulty-agnostic one (which predates difficulty tagging), then the
/// grade's base letter repeats both, and a difficulty-only role closes the
/// chain; the caller appends the plain `results` and `default` roles itself:
/// `results@{diff}@S+ -> results@S+ -> results@{diff}@S -> results@S ->
/// results@{diff}` (then `results -> default`).
#[must_use]
pub fn results_role_candidates(
    grade_suffix: &str,
    base_grade_suffix: Option<&str>,
    difficulty: Option<&str>,
) -> Vec<String> {
    let mut out = Vec::with_capacity(if base_grade_suffix.is_some() { 5 } else { 3 });
    for suffix in [Some(grade_suffix), base_grade_suffix]
        .into_iter()
        .flatten()
    {
        if let Some(diff) = difficulty {
            out.push(format!("results@{diff}@{suffix}"));
        }
        out.push(format!("results@{suffix}"));
    }
    if let Some(diff) = difficulty {
        out.push(format!("results@{diff}"));
    }
    out
}

/// Return the effect for a new tap judgment in this column.
/// Records the judgement time so the same one is not re-fired, and re-arms (sentinel)
/// when the column currently has no judgement.
pub fn tap_event(judged: Option<TapLightJudgment>, prev: &mut f32) -> Option<TapLightEffect> {
    match judged {
        Some(j) if j.at_screen_s != *prev => {
            *prev = j.at_screen_s;
            Some(j.effect)
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

/// Decide an edge on a boolean tracker (a freeze/roll engage or a physical panel
/// press): `Some(true)` on rise, `Some(false)` on fall, `None` when nothing changed.
pub const fn hold_edge(engaged: bool, prev: &mut bool) -> Option<bool> {
    if engaged == *prev {
        None
    } else {
        *prev = engaged;
        Some(engaged)
    }
}

/// Consume a new hold judgment and return its effect when one was supplied.
pub fn hold_outcome_event(
    judged: Option<HoldLightJudgment>,
    prev: &mut f32,
) -> Option<HoldLightEffect> {
    match judged {
        Some(j) if j.at_screen_s != *prev => {
            *prev = j.at_screen_s;
            j.effect
        }
        None => {
            *prev = NO_EVENT;
            None
        }
        _ => None,
    }
}

/// Decide a new mine hit, keyed by hit time so a second hit on the same column while an
/// earlier explosion is still active is still caught.
pub fn mine_event(hit_at: Option<f32>, prev: &mut f32) -> bool {
    match hit_at {
        Some(ts) if ts != *prev => {
            *prev = ts;
            true
        }
        None => {
            *prev = NO_EVENT;
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tap(effect: TapLightEffect, at: f32) -> TapLightJudgment {
        TapLightJudgment {
            effect,
            at_screen_s: at,
        }
    }

    fn hold(effect: Option<HoldLightEffect>, at: f32) -> HoldLightJudgment {
        HoldLightJudgment {
            effect,
            at_screen_s: at,
        }
    }

    #[test]
    fn tap_flash_fires_once_per_new_judgment() {
        let mut prev = NO_EVENT;
        assert!(tap_event(Some(tap(TapLightEffect::Great, 1.0)), &mut prev).is_some());
        assert_eq!(prev, 1.0);
        assert!(tap_event(Some(tap(TapLightEffect::Great, 1.0)), &mut prev).is_none());
        assert!(tap_event(Some(tap(TapLightEffect::Miss, 2.0)), &mut prev).is_some());
        assert_eq!(prev, 2.0);
    }

    #[test]
    fn tap_flash_none_rearms() {
        let mut prev = 5.0;
        assert!(tap_event(None, &mut prev).is_none());
        assert_eq!(prev, NO_EVENT);
        assert!(tap_event(Some(tap(TapLightEffect::Decent, 0.0)), &mut prev).is_some());
    }

    #[test]
    fn tap_event_returns_the_supplied_effect() {
        let mut prev = NO_EVENT;
        assert_eq!(
            tap_event(Some(tap(TapLightEffect::Miss, 1.0)), &mut prev),
            Some(TapLightEffect::Miss)
        );
        assert_eq!(
            tap_event(Some(tap(TapLightEffect::FantasticBlue, 2.0)), &mut prev),
            Some(TapLightEffect::FantasticBlue)
        );
    }

    #[test]
    fn hold_edge_reports_only_transitions() {
        let mut prev = false;
        assert_eq!(hold_edge(false, &mut prev), None);
        assert_eq!(hold_edge(true, &mut prev), Some(true));
        assert_eq!(hold_edge(true, &mut prev), None);
        assert_eq!(hold_edge(false, &mut prev), Some(false));
    }

    #[test]
    fn hold_outcome_event_consumes_judgments_with_or_without_an_effect() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(Some(HoldLightEffect::Ok), 1.0)), &mut prev),
            Some(HoldLightEffect::Ok)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(Some(HoldLightEffect::Bad), 2.0)), &mut prev),
            Some(HoldLightEffect::Bad)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(None, 3.0)), &mut prev),
            None
        );
        assert_eq!(prev, 3.0);
    }

    #[test]
    fn hold_outcome_event_ignores_repeat_and_rearms() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(Some(HoldLightEffect::Ok), 1.0)), &mut prev),
            Some(HoldLightEffect::Ok)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(Some(HoldLightEffect::Ok), 1.0)), &mut prev),
            None
        );
        assert_eq!(hold_outcome_event(None, &mut prev), None);
        assert_eq!(prev, NO_EVENT);
    }

    #[test]
    fn mine_event_catches_consecutive_hits() {
        let mut prev = NO_EVENT;
        assert!(mine_event(Some(1.0), &mut prev));
        assert!(!mine_event(Some(1.0), &mut prev));
        assert!(mine_event(Some(1.5), &mut prev));
        assert!(!mine_event(None, &mut prev));
        assert_eq!(prev, NO_EVENT);
    }

    fn anim(tag: u8) -> Arc<PanelAnim> {
        Arc::new(PanelAnim {
            frames: vec![[tag; crate::gifs::PANEL_RGB_BYTES]],
            durations: vec![0.1],
            loop_frame: 0,
            loop_end: 0,
        })
    }

    #[test]
    fn judgment_effects_select_the_matching_animation() {
        let gifs = JudgementGifs {
            fantastic_blue: Some(anim(1)),
            fantastic_white: Some(anim(2)),
            miss: Some(anim(3)),
            ok: Some(anim(4)),
            bad: Some(anim(5)),
            ..Default::default()
        };
        let frame0 = |a: Option<&Arc<PanelAnim>>| a.unwrap().frames[0][0];
        assert_eq!(frame0(gifs.for_tap_effect(TapLightEffect::FantasticBlue)), 1);
        assert_eq!(frame0(gifs.for_tap_effect(TapLightEffect::FantasticWhite)), 2);
        assert_eq!(frame0(gifs.for_tap_effect(TapLightEffect::Miss)), 3);
        assert!(gifs.for_tap_effect(TapLightEffect::Great).is_none());
        assert_eq!(frame0(gifs.for_hold_effect(HoldLightEffect::Ok)), 4);
        assert_eq!(frame0(gifs.for_hold_effect(HoldLightEffect::Bad)), 5);
    }

    #[test]
    fn sustain_anim_distinguishes_freeze_and_roll() {
        let gifs = JudgementGifs {
            freeze: Some(anim(1)),
            roll: Some(anim(2)),
            ..Default::default()
        };
        let frame0 = |a: Option<&Arc<PanelAnim>>| a.unwrap().frames[0][0];
        assert_eq!(frame0(sustain_anim(&gifs, Some(NoteType::Hold))), 1);
        assert_eq!(frame0(sustain_anim(&gifs, Some(NoteType::Roll))), 2);
        assert!(sustain_anim(&gifs, Some(NoteType::Tap)).is_none());
        assert!(sustain_anim(&gifs, None).is_none());
        assert!(sustain_anim(&JudgementGifs::default(), Some(NoteType::Hold)).is_none());
    }

    #[test]
    fn results_role_candidates_follow_the_documented_chain() {
        assert_eq!(
            results_role_candidates("S+", Some("S"), Some("hard")),
            [
                "results@hard@S+",
                "results@S+",
                "results@hard@S",
                "results@S",
                "results@hard",
            ]
        );
        assert_eq!(
            results_role_candidates("S+", Some("S"), None),
            ["results@S+", "results@S"]
        );
        assert_eq!(
            results_role_candidates("S", None, Some("edit")),
            ["results@edit@S", "results@S", "results@edit"]
        );
        assert_eq!(
            results_role_candidates("star5", None, None),
            ["results@star5"]
        );
        assert_eq!(
            results_role_candidates("F", None, Some("challenge")),
            ["results@challenge@F", "results@F", "results@challenge"]
        );
    }
}
