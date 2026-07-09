use std::sync::Arc;

use deadsync_core::note::NoteType;
use deadsync_gameplay::{ColumnTapJudgment, HoldJudgmentRenderInfo};
use deadsync_rules::judgment::JudgeGrade;
use deadsync_rules::note::HoldResult;
use deadsync_score::Grade;

use crate::gifs::{GifRegistry, PadSize, PanelAnim};
use crate::panels::OverlayDrive;

/// Sentinel `*_at_screen_s` meaning "nothing seen yet for this column".
pub const NO_EVENT: f32 = f32::NEG_INFINITY;

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
    /// Resolve the standard judgement names from a registry through the usual
    /// pack-then-size fallback. `_25` is the baseline both pad layouts render.
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

    /// The one-shot animation for a tap grade, honouring the FA+ white/blue split.
    pub fn for_grade(&self, grade: JudgeGrade, blue_fantastic: bool) -> Option<&Arc<PanelAnim>> {
        match grade {
            JudgeGrade::Fantastic if blue_fantastic => self.fantastic_blue.as_ref(),
            JudgeGrade::Fantastic => self.fantastic_white.as_ref(),
            JudgeGrade::Excellent => self.excellent.as_ref(),
            JudgeGrade::Great => self.great.as_ref(),
            JudgeGrade::Decent => self.decent.as_ref(),
            JudgeGrade::WayOff => self.way_off.as_ref(),
            JudgeGrade::Miss => self.miss.as_ref(),
        }
    }
}

/// The looping sustain animation for an engaged hold, by its note kind.
pub fn sustain_anim(gifs: &JudgementGifs, kind: Option<NoteType>) -> Option<&Arc<PanelAnim>> {
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
pub fn sustain_drive(kind: Option<NoteType>, resume: bool) -> OverlayDrive {
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
pub fn results_role_candidates(grade: Grade, difficulty: Option<&str>) -> Vec<String> {
    let mut suffixes = vec![grade.gif_suffix()];
    if let Some(base) = grade.gif_base() {
        suffixes.push(base.gif_suffix());
    }
    let mut out = Vec::with_capacity(2 * suffixes.len() + 1);
    for suffix in suffixes {
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

/// Decide a new tap judgement for a column (the grade and its FA+ white/blue flag).
/// Records the judgement time so the same one is not re-fired, and re-arms (sentinel)
/// when the column currently has no judgement.
pub fn tap_event(judged: Option<ColumnTapJudgment>, prev: &mut f32) -> Option<(JudgeGrade, bool)> {
    match judged {
        Some(j) if j.at_screen_s != *prev => {
            *prev = j.at_screen_s;
            Some((j.grade, j.blue_fantastic))
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
pub fn hold_edge(engaged: bool, prev: &mut bool) -> Option<bool> {
    if engaged == *prev {
        None
    } else {
        *prev = engaged;
        Some(engaged)
    }
}

/// Decide a new freeze/roll outcome for a column. Held shows OK, dropped shows the
/// failure effect, missed consumes the event but shows nothing.
pub fn hold_outcome_event(
    judged: Option<HoldJudgmentRenderInfo>,
    prev: &mut f32,
) -> Option<HoldResult> {
    match judged {
        Some(j) if j.started_at_screen_s != *prev => {
            *prev = j.started_at_screen_s;
            match j.result {
                HoldResult::Held | HoldResult::LetGo => Some(j.result),
                HoldResult::Missed => None,
            }
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

    fn tap(grade: JudgeGrade, blue_fantastic: bool, at: f32) -> ColumnTapJudgment {
        ColumnTapJudgment {
            grade,
            blue_fantastic,
            at_screen_s: at,
        }
    }

    fn hold(result: HoldResult, at: f32) -> HoldJudgmentRenderInfo {
        HoldJudgmentRenderInfo {
            result,
            started_at_screen_s: at,
        }
    }

    #[test]
    fn tap_flash_fires_once_per_new_judgment() {
        let mut prev = NO_EVENT;
        assert!(tap_event(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_some());
        assert_eq!(prev, 1.0);
        assert!(tap_event(Some(tap(JudgeGrade::Great, false, 1.0)), &mut prev).is_none());
        assert!(tap_event(Some(tap(JudgeGrade::Miss, false, 2.0)), &mut prev).is_some());
        assert_eq!(prev, 2.0);
    }

    #[test]
    fn tap_flash_none_rearms() {
        let mut prev = 5.0;
        assert!(tap_event(None, &mut prev).is_none());
        assert_eq!(prev, NO_EVENT);
        assert!(tap_event(Some(tap(JudgeGrade::Decent, false, 0.0)), &mut prev).is_some());
    }

    #[test]
    fn tap_event_carries_grade_and_fa_plus_flag() {
        let mut prev = NO_EVENT;
        assert_eq!(
            tap_event(Some(tap(JudgeGrade::Miss, false, 1.0)), &mut prev),
            Some((JudgeGrade::Miss, false))
        );
        assert_eq!(
            tap_event(Some(tap(JudgeGrade::Fantastic, true, 2.0)), &mut prev),
            Some((JudgeGrade::Fantastic, true))
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
    fn hold_outcome_event_maps_result() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HoldResult::Held)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::LetGo, 2.0)), &mut prev),
            Some(HoldResult::LetGo)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Missed, 3.0)), &mut prev),
            None
        );
        assert_eq!(prev, 3.0);
    }

    #[test]
    fn hold_outcome_event_ignores_repeat_and_rearms() {
        let mut prev = NO_EVENT;
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
            Some(HoldResult::Held)
        );
        assert_eq!(
            hold_outcome_event(Some(hold(HoldResult::Held, 1.0)), &mut prev),
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
    fn for_grade_picks_the_right_animation_and_falls_back() {
        let gifs = JudgementGifs {
            fantastic_blue: Some(anim(1)),
            fantastic_white: Some(anim(2)),
            miss: Some(anim(3)),
            ..Default::default()
        };
        let frame0 = |a: Option<&Arc<PanelAnim>>| a.unwrap().frames[0][0];
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Fantastic, true)), 1);
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Fantastic, false)), 2);
        assert_eq!(frame0(gifs.for_grade(JudgeGrade::Miss, false)), 3);
        assert!(gifs.for_grade(JudgeGrade::Great, false).is_none());
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
            results_role_candidates(Grade::Tier05, Some("hard")),
            [
                "results@hard@S+",
                "results@S+",
                "results@hard@S",
                "results@S",
                "results@hard",
            ]
        );
        assert_eq!(
            results_role_candidates(Grade::Tier05, None),
            ["results@S+", "results@S"]
        );
        assert_eq!(
            results_role_candidates(Grade::Tier06, Some("edit")),
            ["results@edit@S", "results@S", "results@edit"]
        );
        assert_eq!(
            results_role_candidates(Grade::Quint, None),
            ["results@star5"]
        );
        assert_eq!(
            results_role_candidates(Grade::Failed, Some("challenge")),
            ["results@challenge@F", "results@F", "results@challenge"]
        );
    }
}
