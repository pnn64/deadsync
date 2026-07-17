use crate::{FieldPlacement, MeasureLineMode};
use deadlib_present::actors::{Actor, SizeSpec};
use deadsync_core::input::MAX_COLS;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotefieldFrameFeatures {
    pub measure_line_mode: MeasureLineMode,
    pub measure_cues: bool,
    pub column_cues: bool,
    pub crossover_cues: bool,
    pub crossover_countdown: bool,
    pub column_flash: bool,
    pub error_bar: bool,
    pub error_bar_text: bool,
    pub held_miss_asset: bool,
    pub combo_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NotefieldFramePlanRequest {
    pub placement: FieldPlacement,
    pub num_players: usize,
    pub cols_per_player: usize,
    pub total_cols: usize,
    pub features: NotefieldFrameFeatures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotefieldFramePlan {
    pub player_idx: usize,
    pub col_start: usize,
    pub num_cols: usize,
    pub field_actor_reserve: usize,
    pub hud_actor_reserve: usize,
}

/// Resolve the runtime player/column span and preserve the notefield's bounded
/// actor reserve policy without exposing profile or theme state.
pub(crate) fn notefield_frame_plan(
    request: NotefieldFramePlanRequest,
) -> Option<NotefieldFramePlan> {
    let player_idx = if request.num_players == 1 {
        0
    } else {
        match request.placement {
            FieldPlacement::P1 => 0,
            FieldPlacement::P2 => 1,
        }
    };
    if player_idx >= request.num_players {
        return None;
    }

    let col_start = player_idx * request.cols_per_player;
    let col_end = (col_start + request.cols_per_player)
        .min(request.total_cols)
        .min(MAX_COLS);
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return None;
    }

    let features = request.features;
    let measure_line_extra = match features.measure_line_mode {
        MeasureLineMode::Off => 0,
        MeasureLineMode::Measure => 18,
        MeasureLineMode::Quarter => 30,
        MeasureLineMode::Eighth => 42,
        MeasureLineMode::Edit => 72,
    };
    let field_actor_reserve = (num_cols * 10).max(28)
        + measure_line_extra
        + usize::from(features.measure_cues) * 32
        + usize::from(features.column_cues) * (num_cols + 4)
        + usize::from(features.crossover_cues) * (num_cols + 4)
        + usize::from(features.column_flash) * num_cols
        + usize::from(features.error_bar) * 18;
    let hud_actor_reserve = 8
        + usize::from(features.column_cues)
        + usize::from(features.crossover_cues && features.crossover_countdown)
        + usize::from(features.held_miss_asset) * num_cols
        + usize::from(features.combo_visible) * 2
        + usize::from(features.error_bar_text);

    Some(NotefieldFramePlan {
        player_idx,
        col_start,
        num_cols,
        field_actor_reserve,
        hud_actor_reserve,
    })
}

pub struct BuiltNotefield {
    pub layout_center_x: f32,
    pub field_actors: Option<CapturedActorSource>,
    pub judgment_actors: Option<CapturedActorSource>,
    pub combo_actors: Option<CapturedActorSource>,
}

pub type CapturedActorSource = [Arc<[Actor]>; 1];

impl BuiltNotefield {
    pub fn empty(layout_center_x: f32) -> Self {
        Self {
            layout_center_x,
            field_actors: None,
            judgment_actors: None,
            combo_actors: None,
        }
    }
}

pub(crate) fn actor_with_world_z(mut actor: Actor, world_z: f32) -> Actor {
    match &mut actor {
        Actor::Sprite { world_z: z, .. }
        | Actor::TexturedMesh { world_z: z, .. }
        | Actor::ReusableTexturedMesh { world_z: z, .. } => *z = world_z,
        _ => {}
    }
    actor
}

pub(crate) fn share_actor_range(
    actors: &mut Vec<Actor>,
    start: usize,
) -> Option<CapturedActorSource> {
    if start >= actors.len() {
        return None;
    }
    let shared: Arc<[Actor]> = Arc::from_iter(actors.drain(start..));
    actors.push(Actor::SharedFrame {
        align: [0.0, 0.0],
        offset: [0.0, 0.0],
        size: [SizeSpec::Fill, SizeSpec::Fill],
        children: Arc::clone(&shared),
        background: None,
        z: 0,
        tint: [1.0, 1.0, 1.0, 1.0],
        blend: None,
    });
    Some([shared])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features(measure_line_mode: MeasureLineMode) -> NotefieldFrameFeatures {
        NotefieldFrameFeatures {
            measure_line_mode,
            measure_cues: false,
            column_cues: false,
            crossover_cues: false,
            crossover_countdown: false,
            column_flash: false,
            error_bar: false,
            error_bar_text: false,
            held_miss_asset: false,
            combo_visible: false,
        }
    }

    fn request(
        placement: FieldPlacement,
        measure_line_mode: MeasureLineMode,
    ) -> NotefieldFramePlanRequest {
        NotefieldFramePlanRequest {
            placement,
            num_players: 1,
            cols_per_player: 4,
            total_cols: 4,
            features: features(measure_line_mode),
        }
    }

    #[test]
    fn single_player_p2_uses_runtime_player_zero() {
        let plan = notefield_frame_plan(request(FieldPlacement::P2, MeasureLineMode::Off))
            .expect("single-player P2 field should resolve");
        assert_eq!(plan.player_idx, 0);
        assert_eq!(plan.col_start, 0);
        assert_eq!(plan.num_cols, 4);
    }

    #[test]
    fn versus_p2_uses_second_player_columns() {
        let mut request = request(FieldPlacement::P2, MeasureLineMode::Off);
        request.num_players = 2;
        request.total_cols = 8;
        let plan = notefield_frame_plan(request).expect("versus P2 field should resolve");
        assert_eq!(plan.player_idx, 1);
        assert_eq!(plan.col_start, 4);
        assert_eq!(plan.num_cols, 4);
    }

    #[test]
    fn final_player_span_is_truncated_to_total_and_max_columns() {
        let mut request = request(FieldPlacement::P2, MeasureLineMode::Off);
        request.num_players = 2;
        request.total_cols = 7;
        let total_limited = notefield_frame_plan(request).expect("partial P2 field should resolve");
        assert_eq!(total_limited.col_start, 4);
        assert_eq!(total_limited.num_cols, 3);

        request.cols_per_player = 6;
        request.total_cols = 10;
        let max_limited = notefield_frame_plan(request).expect("capped P2 field should resolve");
        assert_eq!(max_limited.col_start, 6);
        assert_eq!(max_limited.num_cols, 2);
    }

    #[test]
    fn invalid_player_and_empty_column_spans_are_rejected() {
        let mut no_players = request(FieldPlacement::P1, MeasureLineMode::Off);
        no_players.num_players = 0;
        assert!(notefield_frame_plan(no_players).is_none());

        let mut no_columns = request(FieldPlacement::P1, MeasureLineMode::Off);
        no_columns.total_cols = 0;
        assert!(notefield_frame_plan(no_columns).is_none());

        let mut missing_p2 = request(FieldPlacement::P2, MeasureLineMode::Off);
        missing_p2.num_players = 2;
        missing_p2.total_cols = 4;
        assert!(notefield_frame_plan(missing_p2).is_none());
    }

    #[test]
    fn measure_modes_preserve_field_reserve_policy() {
        for (mode, expected) in [
            (MeasureLineMode::Off, 40),
            (MeasureLineMode::Measure, 58),
            (MeasureLineMode::Quarter, 70),
            (MeasureLineMode::Eighth, 82),
            (MeasureLineMode::Edit, 112),
        ] {
            let plan = notefield_frame_plan(request(FieldPlacement::P1, mode))
                .expect("four-column field should resolve");
            assert_eq!(plan.field_actor_reserve, expected);
            assert_eq!(plan.hud_actor_reserve, 8);
        }
    }

    #[test]
    fn enabled_features_preserve_current_reserve_totals() {
        let mut request = request(FieldPlacement::P1, MeasureLineMode::Edit);
        request.features = NotefieldFrameFeatures {
            measure_line_mode: MeasureLineMode::Edit,
            measure_cues: true,
            column_cues: true,
            crossover_cues: true,
            crossover_countdown: true,
            column_flash: true,
            error_bar: true,
            error_bar_text: true,
            held_miss_asset: true,
            combo_visible: true,
        };
        let plan = notefield_frame_plan(request).expect("feature-rich field should resolve");
        assert_eq!(plan.field_actor_reserve, 182);
        assert_eq!(plan.hud_actor_reserve, 17);

        request.features.crossover_cues = false;
        let no_crossover = notefield_frame_plan(request).expect("field should resolve");
        assert_eq!(no_crossover.hud_actor_reserve, 16);

        request.features.crossover_countdown = false;
        let no_countdown = notefield_frame_plan(request).expect("field should resolve");
        assert_eq!(no_countdown.hud_actor_reserve, 16);

        request.features.held_miss_asset = false;
        request.features.combo_visible = false;
        let reduced = notefield_frame_plan(request).expect("field should resolve");
        assert_eq!(reduced.hud_actor_reserve, 10);
    }
}
