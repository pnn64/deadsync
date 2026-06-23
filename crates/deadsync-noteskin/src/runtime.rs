use crate::{
    ExplosionAnimation, NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartTextureTranslate,
    ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior, ReceptorStepBehavior,
    ReceptorStepBehaviors,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TapExplosionLayer<T> {
    pub slot: T,
    pub animation: ExplosionAnimation,
}

#[derive(Debug, Clone)]
pub struct TapExplosion<T> {
    pub slot: T,
    pub animation: ExplosionAnimation,
    pub layers: Arc<[TapExplosionLayer<T>]>,
}

impl<T: Clone> TapExplosion<T> {
    pub fn from_single(slot: T, animation: ExplosionAnimation) -> Self {
        Self::from_layers(vec![TapExplosionLayer { slot, animation }])
            .expect("single tap explosion layer must build")
    }

    pub fn from_layers(layers: Vec<TapExplosionLayer<T>>) -> Option<Self> {
        let first = layers.first()?.clone();
        Some(Self {
            slot: first.slot,
            animation: first.animation,
            layers: Arc::from(layers),
        })
    }

    pub fn duration(&self) -> f32 {
        self.layers
            .iter()
            .map(|layer| layer.animation.duration())
            .fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone)]
pub struct HoldVisuals<T> {
    pub head_inactive: Option<T>,
    pub head_active: Option<T>,
    pub head_inactive_layers: Option<Arc<[T]>>,
    pub head_active_layers: Option<Arc<[T]>>,
    pub body_inactive: Option<T>,
    pub body_active: Option<T>,
    pub topcap_inactive: Option<T>,
    pub topcap_active: Option<T>,
    pub bottomcap_inactive: Option<T>,
    pub bottomcap_active: Option<T>,
    pub explosion: Option<T>,
}

impl<T> Default for HoldVisuals<T> {
    fn default() -> Self {
        Self {
            head_inactive: None,
            head_active: None,
            head_inactive_layers: None,
            head_active_layers: None,
            body_inactive: None,
            body_active: None,
            topcap_inactive: None,
            topcap_active: None,
            bottomcap_inactive: None,
            bottomcap_active: None,
            explosion: None,
        }
    }
}

#[derive(Debug)]
pub struct NoteskinRuntime<T> {
    pub notes: Vec<T>,
    pub note_layers: Vec<Arc<[T]>>,
    pub lift_note_layers: Vec<Arc<[T]>>,
    pub receptor_off: Vec<T>,
    pub receptor_glow: Vec<Option<T>>,
    pub receptor_off_reverse: Vec<ReceptorReverseBehavior>,
    pub receptor_glow_reverse: Vec<ReceptorReverseBehavior>,
    pub receptor_step_behaviors: Vec<ReceptorStepBehaviors>,
    pub mines: Vec<Option<T>>,
    pub mine_fill_slots: Vec<Option<T>>,
    pub mine_frames: Vec<Option<T>>,
    pub column_xs: Vec<i32>,
    pub tap_explosions: HashMap<String, TapExplosion<T>>,
    pub tap_explosions_by_col: Vec<HashMap<String, TapExplosion<T>>>,
    pub mine_hit_explosion: Option<TapExplosion<T>>,
    pub receptor_glow_behavior: ReceptorGlowBehavior,
    pub receptor_pulse: ReceptorPulse,
    pub hold_let_go_gray_percent: f32,
    pub hold_columns: Vec<HoldVisuals<T>>,
    pub roll_columns: Vec<HoldVisuals<T>>,
    pub hold: HoldVisuals<T>,
    pub roll: HoldVisuals<T>,
    pub animation_is_beat_based: bool,
    pub note_display_metrics: NoteDisplayMetrics,
}

impl<T> NoteskinRuntime<T> {
    #[inline(always)]
    pub fn tap_explosion_for_col(&self, col: usize, window: &str) -> Option<&TapExplosion<T>> {
        self.tap_explosion_for_col_with_bright(col, window, false)
    }

    #[inline(always)]
    pub fn tap_explosion_for_col_with_bright(
        &self,
        col: usize,
        window: &str,
        bright: bool,
    ) -> Option<&TapExplosion<T>> {
        if bright
            && let Some(key) = bright_tap_explosion_key(window)
            && let Some(explosion) = self.tap_explosion_for_col_key(col, key)
        {
            return Some(explosion);
        }
        self.tap_explosion_for_col_key(col, window)
    }

    #[inline(always)]
    fn tap_explosion_for_col_key(&self, col: usize, key: &str) -> Option<&TapExplosion<T>> {
        self.tap_explosions_by_col
            .get(col)
            .and_then(|by_window| by_window.get(key))
            .or_else(|| self.tap_explosions.get(key))
    }

    #[inline(always)]
    pub fn for_each_slot(&self, mut visit: impl FnMut(&T)) {
        for slot in &self.notes {
            visit(slot);
        }
        for layer in &self.note_layers {
            for slot in layer.iter() {
                visit(slot);
            }
        }
        for layer in &self.lift_note_layers {
            for slot in layer.iter() {
                visit(slot);
            }
        }
        for slot in &self.receptor_off {
            visit(slot);
        }
        for slot in &self.receptor_glow {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mines {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mine_fill_slots {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mine_frames {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for explosion in self.tap_explosions.values() {
            for layer in explosion.layers.iter() {
                visit(&layer.slot);
            }
        }
        for by_col in &self.tap_explosions_by_col {
            for explosion in by_col.values() {
                for layer in explosion.layers.iter() {
                    visit(&layer.slot);
                }
            }
        }
        if let Some(explosion) = self.mine_hit_explosion.as_ref() {
            visit(&explosion.slot);
        }
        let mut visit_hold = |h: &HoldVisuals<T>| {
            for slot in [
                h.head_inactive.as_ref(),
                h.head_active.as_ref(),
                h.body_inactive.as_ref(),
                h.body_active.as_ref(),
                h.topcap_inactive.as_ref(),
                h.topcap_active.as_ref(),
                h.bottomcap_inactive.as_ref(),
                h.bottomcap_active.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                visit(slot);
            }
            for layers in [
                h.head_inactive_layers.as_deref(),
                h.head_active_layers.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                for slot in layers {
                    visit(slot);
                }
            }
            if let Some(slot) = h.explosion.as_ref() {
                visit(slot);
            }
        };
        visit_hold(&self.hold);
        visit_hold(&self.roll);
        for col in &self.hold_columns {
            visit_hold(col);
        }
        for col in &self.roll_columns {
            visit_hold(col);
        }
    }

    #[inline(always)]
    pub fn part_uv_phase(
        &self,
        part: NoteAnimPart,
        song_seconds: f32,
        song_beat: f32,
        note_beat: f32,
    ) -> f32 {
        let anim = self.note_display_metrics.part_animation[part as usize];
        part_uv_phase_inner(
            song_seconds,
            song_beat,
            note_beat,
            anim.length,
            anim.vivid,
            self.animation_is_beat_based,
        )
    }

    #[inline(always)]
    pub fn tap_note_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Tap, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn tap_mine_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Mine, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn part_uv_translation(
        &self,
        part: NoteAnimPart,
        note_beat: f32,
        is_addition: bool,
    ) -> [f32; 2] {
        let metrics = self.note_display_metrics.part_texture_translate[part as usize];
        part_uv_translation_inner(note_beat, metrics, is_addition)
    }

    #[inline(always)]
    pub fn hold_visuals_for_col(&self, col: usize, is_roll: bool) -> &HoldVisuals<T> {
        if is_roll {
            self.roll_columns
                .get(col)
                .or_else(|| self.roll_columns.first())
                .unwrap_or(&self.roll)
        } else {
            self.hold_columns
                .get(col)
                .or_else(|| self.hold_columns.first())
                .unwrap_or(&self.hold)
        }
    }

    #[inline(always)]
    pub fn receptor_step_behavior_for_col(
        &self,
        col: usize,
        window: Option<&str>,
    ) -> ReceptorStepBehavior {
        self.receptor_step_behaviors
            .get(col)
            .copied()
            .or_else(|| self.receptor_step_behaviors.first().copied())
            .unwrap_or_default()
            .for_window(window)
    }
}

#[inline(always)]
pub fn bright_tap_explosion_key(window: &str) -> Option<&'static str> {
    match window {
        "W1" => Some("W1Bright"),
        "W2" => Some("W2Bright"),
        "W3" => Some("W3Bright"),
        "W4" => Some("W4Bright"),
        "W5" => Some("W5Bright"),
        "Held" => Some("HeldBright"),
        _ => None,
    }
}

#[inline(always)]
fn part_uv_phase_inner(
    song_seconds: f32,
    song_beat: f32,
    note_beat: f32,
    length: f32,
    vivid: bool,
    beat_based: bool,
) -> f32 {
    let length = length.max(1e-6);
    let clock = if beat_based { song_beat } else { song_seconds };
    let mut phase = clock.rem_euclid(length) / length;
    if vivid {
        let note_fraction = note_beat.rem_euclid(1.0);
        let vivid_interval = 1.0 / length;
        let vivid_offset = (note_fraction / vivid_interval).floor() * vivid_interval;
        phase = (phase + vivid_offset).rem_euclid(1.0);
    }
    phase
}

#[inline(always)]
fn part_uv_translation_inner(
    note_beat: f32,
    metrics: NotePartTextureTranslate,
    is_addition: bool,
) -> [f32; 2] {
    let count = metrics.note_color_count.max(1);
    let countf = count as f32;
    let color = match metrics.note_color_type {
        NoteColorType::Denominator => {
            let note_type = beat_to_note_type_index(note_beat) as f32;
            note_type.clamp(0.0, (count - 1) as f32)
        }
        NoteColorType::Progress => (note_beat * countf).ceil() % countf,
        NoteColorType::ProgressAlternate => {
            let mut scaled = note_beat * countf;
            if scaled - (scaled as i64 as f32) == 0.0 {
                scaled += countf - 1.0;
            }
            scaled.ceil() % countf
        }
    };
    let add = if is_addition {
        metrics.addition_offset
    } else {
        [0.0, 0.0]
    };
    [
        metrics.note_color_spacing[0].mul_add(color, add[0]),
        metrics.note_color_spacing[1].mul_add(color, add[1]),
    ]
}

#[inline(always)]
fn beat_to_note_type_index(beat: f32) -> i32 {
    let row = (beat * 48.0).round() as i32;
    if row.rem_euclid(48) == 0 {
        0
    } else if row.rem_euclid(24) == 0 {
        1
    } else if row.rem_euclid(16) == 0 {
        2
    } else if row.rem_euclid(12) == 0 {
        3
    } else if row.rem_euclid(8) == 0 {
        4
    } else if row.rem_euclid(6) == 0 {
        5
    } else if row.rem_euclid(4) == 0 {
        6
    } else if row.rem_euclid(3) == 0 {
        7
    } else {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HoldVisuals, NoteskinRuntime, TapExplosion, TapExplosionLayer, bright_tap_explosion_key,
    };
    use crate::{
        ExplosionAnimation, ExplosionSegment, ExplosionState, NoteAnimPart, NoteDisplayMetrics,
        NotePartAnimation, NotePartTextureTranslate, ReceptorStepBehavior, ReceptorStepBehaviors,
        TweenType,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Slot(u8);

    #[test]
    fn hold_visuals_default_does_not_require_default_slot() {
        let visuals = HoldVisuals::<Slot>::default();
        assert!(visuals.head_inactive.is_none());
        assert!(visuals.head_active.is_none());
        assert!(visuals.explosion.is_none());
    }

    #[test]
    fn tap_explosion_duration_uses_longest_layer() {
        let short = ExplosionAnimation {
            initial: ExplosionState::default(),
            segments: vec![ExplosionSegment {
                duration: 0.25,
                tween: TweenType::Linear,
                start: ExplosionState::default(),
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_rotation_z: None,
                end_visible: None,
            }],
            glow: None,
            blend_add: false,
        };
        let long = ExplosionAnimation {
            initial: ExplosionState::default(),
            segments: vec![ExplosionSegment {
                duration: 0.75,
                tween: TweenType::Linear,
                start: ExplosionState::default(),
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_rotation_z: None,
                end_visible: None,
            }],
            glow: None,
            blend_add: false,
        };
        let explosion = TapExplosion::from_layers(vec![
            TapExplosionLayer {
                slot: Slot(1),
                animation: short,
            },
            TapExplosionLayer {
                slot: Slot(2),
                animation: long,
            },
        ])
        .expect("layers should build tap explosion");

        assert_eq!(explosion.slot, Slot(1));
        assert!((explosion.duration() - 0.75).abs() <= f32::EPSILON);
    }

    #[test]
    fn noteskin_runtime_prefers_column_bright_tap_explosions() {
        let dim = TapExplosion::from_single(Slot(1), ExplosionAnimation::default());
        let bright = TapExplosion::from_single(Slot(2), ExplosionAnimation::default());
        let mut by_col = HashMap::new();
        by_col.insert("W1Bright".to_string(), bright);
        let runtime = NoteskinRuntime {
            tap_explosions: HashMap::from([("W1".to_string(), dim)]),
            tap_explosions_by_col: vec![by_col],
            ..empty_runtime()
        };

        let explosion = runtime
            .tap_explosion_for_col_with_bright(0, "W1", true)
            .expect("bright column explosion should resolve");

        assert_eq!(explosion.slot, Slot(2));
        assert_eq!(bright_tap_explosion_key("Held"), Some("HeldBright"));
        assert_eq!(bright_tap_explosion_key("Miss"), None);
    }

    #[test]
    fn noteskin_runtime_falls_back_to_default_hold_visuals() {
        let runtime = NoteskinRuntime {
            hold: HoldVisuals {
                body_inactive: Some(Slot(1)),
                ..HoldVisuals::default()
            },
            roll_columns: vec![HoldVisuals {
                body_inactive: Some(Slot(2)),
                ..HoldVisuals::default()
            }],
            ..empty_runtime()
        };

        assert_eq!(
            runtime.hold_visuals_for_col(4, false).body_inactive,
            Some(Slot(1))
        );
        assert_eq!(
            runtime.hold_visuals_for_col(4, true).body_inactive,
            Some(Slot(2))
        );
    }

    #[test]
    fn noteskin_runtime_samples_uv_phase_and_translation() {
        let mut metrics = NoteDisplayMetrics::default();
        metrics.part_animation[NoteAnimPart::Tap as usize] = NotePartAnimation {
            length: 2.0,
            vivid: false,
        };
        metrics.part_texture_translate[NoteAnimPart::Tap as usize] = NotePartTextureTranslate {
            addition_offset: [1.0, 2.0],
            note_color_spacing: [0.25, 0.5],
            note_color_count: 8,
            ..NotePartTextureTranslate::default()
        };
        let runtime = NoteskinRuntime {
            animation_is_beat_based: true,
            note_display_metrics: metrics,
            ..empty_runtime()
        };

        assert!((runtime.tap_note_uv_phase(7.0, 3.0, 0.0) - 0.5).abs() <= f32::EPSILON);
        assert_eq!(
            runtime.part_uv_translation(NoteAnimPart::Tap, 0.25, true),
            [1.75, 3.5]
        );
    }

    #[test]
    fn noteskin_runtime_visits_nested_slots() {
        let runtime = NoteskinRuntime {
            notes: vec![Slot(1)],
            note_layers: vec![Arc::from([Slot(2)])],
            receptor_glow: vec![Some(Slot(3))],
            hold: HoldVisuals {
                head_active_layers: Some(Arc::from([Slot(4), Slot(5)])),
                explosion: Some(Slot(6)),
                ..HoldVisuals::default()
            },
            ..empty_runtime()
        };
        let mut visited = Vec::new();

        runtime.for_each_slot(|slot| visited.push(slot.0));

        assert_eq!(visited, [1, 2, 3, 4, 5, 6]);
    }

    fn empty_runtime() -> NoteskinRuntime<Slot> {
        NoteskinRuntime {
            notes: Vec::new(),
            note_layers: Vec::new(),
            lift_note_layers: Vec::new(),
            receptor_off: Vec::new(),
            receptor_glow: Vec::new(),
            receptor_off_reverse: Vec::new(),
            receptor_glow_reverse: Vec::new(),
            receptor_step_behaviors: vec![ReceptorStepBehaviors::new(
                ReceptorStepBehavior {
                    duration: 0.3,
                    zoom_start: 1.0,
                    zoom_end: 2.0,
                    tween: TweenType::Linear,
                    interrupts: true,
                },
                ReceptorStepBehavior::identity(),
                [ReceptorStepBehavior::identity(); 5],
            )],
            mines: Vec::new(),
            mine_fill_slots: Vec::new(),
            mine_frames: Vec::new(),
            column_xs: Vec::new(),
            tap_explosions: HashMap::new(),
            tap_explosions_by_col: Vec::new(),
            mine_hit_explosion: None,
            receptor_glow_behavior: Default::default(),
            receptor_pulse: Default::default(),
            hold_let_go_gray_percent: 0.25,
            hold_columns: Vec::new(),
            roll_columns: Vec::new(),
            hold: HoldVisuals::default(),
            roll: HoldVisuals::default(),
            animation_is_beat_based: false,
            note_display_metrics: NoteDisplayMetrics::default(),
        }
    }
}
