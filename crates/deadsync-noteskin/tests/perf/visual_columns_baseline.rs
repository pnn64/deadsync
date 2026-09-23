// Frozen column/runtime assembly from 66b76b17c; unchanged callees shared.
use super::super::*;

pub fn itg_tap_explosions_by_col_compiled<T: Clone>(
    data: &itg::NoteskinData,
    style: crate::Style,
    compiled: &compiled::CompiledLoader,
    down_explosion_sprites: &[ItgResolvedSprite<T>],
    mut resolve_sprites: impl FnMut(&str, &str) -> Vec<ItgResolvedSprite<T>>,
) -> Vec<TapExplosionMap<T>> {
    let mut out = Vec::with_capacity(style.num_cols);
    for col in 0..style.num_cols {
        let button = itg::button_for_col(style.num_cols, col);
        let column_explosion_sprites = if button.eq_ignore_ascii_case("Down") {
            down_explosion_sprites.to_vec()
        } else {
            resolve_sprites(button, "Explosion")
        };
        out.push(itg_tap_explosion_map_from_resolved_layers(
            &column_explosion_sprites,
            |base_element| {
                let base_request = compiled.load_request_ref(button, base_element);
                itg_direct_tap_explosion_resolved_layers(
                    base_element,
                    base_request.blank,
                    |element| compiled.load_request_ref(button, element).blank,
                    |element| resolve_sprites(button, element),
                )
            },
            |mode, metric_key| {
                data.metrics
                    .get(mode.metric_section(), metric_key)
                    .map(str::to_string)
            },
        ));
    }
    out
}

pub(super) fn itg_noteskin_runtime_selected<T: Clone>(
    data: &itg::NoteskinData,
    style: crate::Style,
    compiled: &compiled::CompiledLoader,
    note_display_metrics: NoteDisplayMetrics,
    animation_is_beat_based: bool,
    columns: ItgRuntimeColumns<T>,
    mut resolve_sprites: impl FnMut(&str, &str) -> Vec<ItgResolvedSprite<T>>,
    mut resolve_hold_explosion: impl FnMut(
        &[ItgResolvedSprite<T>],
        &[ItgResolvedSprite<T>],
        &str,
        &str,
        &str,
        bool,
        Option<&str>,
        Option<&str>,
        Option<&T>,
    ) -> Option<T>,
    mut resolve_direct_slot: impl FnMut(&str, &str) -> Option<T>,
    mut resolve_actor_first_sprite: impl FnMut(&str, &str) -> Option<T>,
    mut mine_fill_slots: impl FnMut(&[Option<T>]) -> Vec<Option<T>>,
    mut texture_key: impl FnMut(&T) -> String,
    mut apply_active_cmd: impl FnMut(&T, &HashMap<String, String>, &str) -> T,
    load: RuntimeLoad,
) -> NoteskinRuntime<T> {
    let ItgRuntimeColumns {
        notes,
        note_layers,
        lift_note_layers,
        receptor_off,
        receptor_glow,
        receptor_idle_glow_layers,
        receptor_off_reverse,
        receptor_glow_reverse,
        receptor_idle_glow_reverse,
        receptor_step_behaviors,
        receptor_idle_glow,
        mines,
        mine_frames,
        mut hold_columns,
        mut roll_columns,
        receptor_pulse_command,
    } = columns;
    let down_col = itg::down_col(style.num_cols);
    let base_button = if style.is_pump() { "Center" } else { "Down" };
    let (mut hold, mut roll) = default_hold_visuals(&hold_columns, &roll_columns, down_col);

    let explosion_sprites =
        if !load.preview || load.has(SkinPart::TapExplosions) || load.has(SkinPart::HoldExplosions)
        {
            resolve_sprites(base_button, "Explosion")
        } else {
            Vec::new()
        };
    if !load.preview || load.has(SkinPart::HoldExplosions) {
        let hold_explosion_request = compiled.load_request_ref(base_button, "Hold Explosion");
        let hold_explosion_blank = hold_explosion_request.blank;
        let hold_explosion_sprites = resolve_sprites(base_button, "Hold Explosion");
        hold.explosion = resolve_hold_explosion(
            &explosion_sprites,
            &hold_explosion_sprites,
            base_button,
            "holdingoncommand",
            "hold explosion",
            hold_explosion_blank,
            Some("Hold Explosion"),
            Some("_down hold explosion"),
            None,
        );
        if !load.preview {
            let roll_explosion_blank = compiled
                .load_request_ref(base_button, "Roll Explosion")
                .blank;
            let roll_explosion_sprites = resolve_sprites(base_button, "Roll Explosion");
            let roll_explosion = resolve_hold_explosion(
                &explosion_sprites,
                &roll_explosion_sprites,
                base_button,
                "rolloncommand",
                "roll explosion",
                roll_explosion_blank,
                Some("Roll Explosion"),
                Some("_down hold explosion"),
                None,
            );
            roll.explosion = itg_roll_explosion_from_resolved_layers(
                &explosion_sprites,
                roll_explosion_blank,
                roll_explosion,
                hold.explosion.clone(),
                &mut texture_key,
                |key| {
                    data.metrics
                        .get("HoldGhostArrow", key)
                        .map(ToString::to_string)
                },
                &mut apply_active_cmd,
            );

            {
                let mut resolve_hold_explosion_for_button =
                    |button: &str,
                     active_key: &str,
                     element_hint: &str,
                     request_element: &str,
                     fallback: Option<&T>| {
                        let column_explosion_sprites = if button.eq_ignore_ascii_case("Down") {
                            explosion_sprites.clone()
                        } else {
                            resolve_sprites(button, "Explosion")
                        };
                        let request = compiled.load_request_ref(button, request_element);
                        let source_sprites = if request.blank {
                            Vec::new()
                        } else {
                            resolve_sprites(button, request_element)
                        };
                        resolve_hold_explosion(
                            &column_explosion_sprites,
                            &source_sprites,
                            button,
                            active_key,
                            element_hint,
                            request.blank,
                            None,
                            None,
                            fallback,
                        )
                    };
                itg_apply_hold_explosions_by_col(
                    style.num_cols,
                    &mut hold_columns,
                    &mut roll_columns,
                    hold.explosion.as_ref(),
                    roll.explosion.as_ref(),
                    &mut resolve_hold_explosion_for_button,
                );
            }
        }
    }
    let tap_explosions_by_col = if load.has(SkinPart::TapExplosions) {
        itg_tap_explosions_by_col_compiled(
            data,
            style,
            compiled,
            &explosion_sprites,
            |button, element| resolve_sprites(button, element),
        )
    } else {
        Vec::new()
    };
    let mine_hit_explosion = if !load.preview {
        itg_hit_mine_explosion_from_layers(
            &explosion_sprites,
            || resolve_direct_slot(base_button, "HitMine Explosion"),
            || resolve_actor_first_sprite(base_button, "HitMine Explosion"),
            data.metrics
                .get("GhostArrowBright", "HitMineCommand")
                .map(str::to_string),
        )
    } else {
        None
    };
    let tap_explosions = default_tap_explosions(&tap_explosions_by_col, down_col);
    let hold_let_go_gray_percent =
        crate::parts::clamped_hold_let_go_gray_percent(&note_display_metrics);
    let receptor = if load.has(SkinPart::Receptors) {
        resolve_sprites(base_button, "Receptor")
    } else {
        Vec::new()
    };
    let receptor_glow_behavior = itg_receptor_glow_behavior_from_layers(&receptor, |metric_key| {
        data.metrics
            .get("ReceptorOverlay", metric_key)
            .map(str::to_string)
    });
    let receptor_pulse = itg_receptor_pulse_from_command(receptor_pulse_command.as_deref());
    let mine_fill_slots = if !load.preview {
        mine_fill_slots(&mines)
    } else {
        Vec::new()
    };
    let column_xs = crate::parts::itg_column_xs(style.num_cols);

    NoteskinRuntime {
        notes,
        note_layers,
        lift_note_layers,
        receptor_off,
        receptor_glow,
        receptor_idle_glow_layers,
        receptor_off_reverse,
        receptor_glow_reverse,
        receptor_idle_glow_reverse,
        receptor_step_behaviors,
        tap_explosions,
        tap_explosions_by_col,
        mine_hit_explosion,
        hold,
        roll,
        mine_fill_slots,
        mines,
        mine_frames,
        hold_columns,
        roll_columns,
        receptor_glow_behavior,
        receptor_idle_glow,
        receptor_pulse,
        column_xs,
        note_display_metrics,
        custom_parts: Default::default(),
        part_animation_is_beat_based: [animation_is_beat_based; crate::NOTE_ANIM_PART_COUNT],
        hold_let_go_gray_percent,
    }
}
