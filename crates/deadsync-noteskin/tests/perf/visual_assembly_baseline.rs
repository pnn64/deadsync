// Frozen from 66b76b17c (0.5.1205). Only imports/visibility adapted.
use super::super::*;
use crate::explosion::parse_itg_tap_explosion_animation;

pub fn itg_hold_visuals_from_parts<T: Clone>(parts: HoldVisualParts<T>) -> HoldVisuals<T> {
    let head_active_layers = if parts.head_active.is_some() {
        parts.head_active_layers.clone()
    } else {
        parts
            .head_active_layers
            .clone()
            .or_else(|| parts.head_inactive_layers.clone())
    };
    HoldVisuals {
        head_inactive: parts.head_inactive.clone(),
        head_active: parts.head_active.or(parts.head_inactive),
        head_inactive_layers: parts.head_inactive_layers,
        head_active_layers,
        body_inactive: parts.body_inactive.clone(),
        body_active: parts.body_active.or(parts.body_inactive),
        topcap_inactive: parts.topcap_inactive.clone(),
        topcap_active: parts.topcap_active.or(parts.topcap_inactive),
        bottomcap_inactive: parts.bottomcap_inactive.clone(),
        bottomcap_active: parts.bottomcap_active.or(parts.bottomcap_inactive),
        explosion: None,
    }
}

pub fn itg_roll_visuals_from_parts<T: Clone>(
    parts: HoldVisualParts<T>,
    hold: &HoldVisuals<T>,
) -> HoldVisuals<T> {
    let head_inactive_has_slot = parts.head_inactive.is_some();
    let head_active_has_slot = parts.head_active.is_some();
    let head_inactive_layers = if head_inactive_has_slot {
        parts.head_inactive_layers.clone()
    } else {
        parts
            .head_inactive_layers
            .clone()
            .or_else(|| hold.head_inactive_layers.clone())
    };
    let head_active_layers = if head_active_has_slot {
        parts.head_active_layers.clone()
    } else if head_inactive_has_slot {
        parts.head_inactive_layers.clone()
    } else {
        parts
            .head_active_layers
            .clone()
            .or_else(|| parts.head_inactive_layers.clone())
            .or_else(|| hold.head_active_layers.clone())
            .or_else(|| hold.head_inactive_layers.clone())
    };
    HoldVisuals {
        head_inactive: parts
            .head_inactive
            .clone()
            .or_else(|| hold.head_inactive.clone()),
        head_active: parts
            .head_active
            .or(parts.head_inactive)
            .or_else(|| hold.head_active.clone())
            .or_else(|| hold.head_inactive.clone()),
        head_inactive_layers,
        head_active_layers,
        body_inactive: parts
            .body_inactive
            .clone()
            .or_else(|| hold.body_inactive.clone()),
        body_active: parts
            .body_active
            .or(parts.body_inactive)
            .or_else(|| hold.body_active.clone())
            .or_else(|| hold.body_inactive.clone()),
        topcap_inactive: parts
            .topcap_inactive
            .clone()
            .or_else(|| hold.topcap_inactive.clone()),
        topcap_active: parts
            .topcap_active
            .or(parts.topcap_inactive)
            .or_else(|| hold.topcap_active.clone())
            .or_else(|| hold.topcap_inactive.clone()),
        bottomcap_inactive: parts
            .bottomcap_inactive
            .clone()
            .or_else(|| hold.bottomcap_inactive.clone()),
        bottomcap_active: parts
            .bottomcap_active
            .or(parts.bottomcap_inactive)
            .or_else(|| hold.bottomcap_active.clone())
            .or_else(|| hold.bottomcap_inactive.clone()),
        explosion: None,
    }
}

#[derive(Clone, Copy)]
struct ItgTapExplosionMatch<'a, T> {
    source: &'a ItgTapExplosionSource<T>,
    direct_command: Option<&'a str>,
}

#[inline]
fn itg_tap_explosion_source_match<'a, T>(
    source: &'a ItgTapExplosionSource<T>,
    window: &str,
    command_key: &str,
) -> Option<ItgTapExplosionMatch<'a, T>> {
    let direct_command = source.commands.get(command_key).map(String::as_str);
    (direct_command.is_some() || source.matches_window(window) || source.is_generic_tap_explosion())
        .then_some(ItgTapExplosionMatch {
            source,
            direct_command,
        })
}

fn itg_tap_explosion_matches<'a, T>(
    sources: &'a [ItgTapExplosionSource<T>],
    window: &str,
    command_key: &str,
) -> SmallVec<[ItgTapExplosionMatch<'a, T>; 4]> {
    sources
        .iter()
        .filter_map(|source| itg_tap_explosion_source_match(source, window, command_key))
        .collect()
}

pub(super) fn itg_tap_explosion_map_from_partitioned_sources<T: Clone>(
    dim_sprites: Vec<ItgTapExplosionSource<T>>,
    bright_sprites: Vec<ItgTapExplosionSource<T>>,
    mut metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> TapExplosionMap<T> {
    if dim_sprites.is_empty() && bright_sprites.is_empty() {
        return TapExplosionMap::new();
    }

    let mut tap_explosions = TapExplosionMap::new();
    for (window, key, metric_key) in [
        ("W1", "w1command", "W1Command"),
        ("W2", "w2command", "W2Command"),
        ("W3", "w3command", "W3Command"),
        ("W4", "w4command", "W4Command"),
        ("W5", "w5command", "W5Command"),
        ("Miss", "misscommand", "MissCommand"),
        ("Held", "heldcommand", "HeldCommand"),
    ] {
        for mode in [ItgTapExplosionMode::Dim, ItgTapExplosionMode::Bright] {
            if mode == ItgTapExplosionMode::Bright && bright_sprites.is_empty() {
                continue;
            }
            let (preferred, fallback_sprites) = match mode {
                ItgTapExplosionMode::Dim => (&dim_sprites, &bright_sprites),
                ItgTapExplosionMode::Bright => (&bright_sprites, &dim_sprites),
            };
            let preferred_matches = itg_tap_explosion_matches(preferred, window, key);
            let has_preferred = !preferred_matches.is_empty();
            if mode == ItgTapExplosionMode::Bright && !has_preferred {
                continue;
            }
            let fallback_matches = itg_tap_explosion_matches(fallback_sprites, window, key);

            let mut layers = SmallVec::new();
            let mut add_source = |matched: &ItgTapExplosionMatch<'_, T>| {
                let fallback;
                let command = if let Some(command) = matched.direct_command {
                    command
                } else {
                    let Some(command) = metric_command(matched.source.mode, metric_key) else {
                        return;
                    };
                    fallback = command;
                    fallback.as_str()
                };
                if command.trim().is_empty() {
                    return;
                }
                layers.push(TapExplosionLayer {
                    slot: matched.source.payload.clone(),
                    animation: parse_itg_tap_explosion_animation(matched.source, mode, command),
                });
            };

            if has_preferred {
                for matched in preferred_matches.iter().chain(&fallback_matches) {
                    add_source(matched);
                }
            } else if fallback_matches.is_empty() {
                if let Some(source) = preferred.first().or_else(|| fallback_sprites.first()) {
                    add_source(&ItgTapExplosionMatch {
                        source,
                        direct_command: source.commands.get(key).map(String::as_str),
                    });
                }
            } else {
                for matched in &fallback_matches {
                    add_source(matched);
                }
            }
            if let Some(explosion) = TapExplosion::from_inline_layers(layers) {
                tap_explosions.insert_window(itg_tap_explosion_key(window, mode), explosion);
            }
        }
    }
    tap_explosions
}

#[inline]
fn push_tap_explosion_source<T>(
    source: ItgTapExplosionSource<T>,
    dim_sources: &mut Vec<ItgTapExplosionSource<T>>,
    bright_sources: &mut Vec<ItgTapExplosionSource<T>>,
) {
    match source.mode {
        ItgTapExplosionMode::Dim => dim_sources.push(source),
        ItgTapExplosionMode::Bright => bright_sources.push(source),
    }
}

fn itg_partition_tap_explosion_layers<L, T>(
    explosion_layers: &[L],
    mut layer_has_tap_command: impl FnMut(&L) -> bool,
    mut direct_layers: impl FnMut(ItgTapExplosionMode) -> Vec<L>,
    mut source_from_layer: impl FnMut(&L) -> ItgTapExplosionSource<T>,
) -> (Vec<ItgTapExplosionSource<T>>, Vec<ItgTapExplosionSource<T>>) {
    let mut dim_sources = Vec::new();
    let mut bright_sources = Vec::new();
    let mut has_actor_sources = false;
    for layer in explosion_layers {
        if layer_has_tap_command(layer) {
            has_actor_sources = true;
            push_tap_explosion_source(
                source_from_layer(layer),
                &mut dim_sources,
                &mut bright_sources,
            );
        }
    }

    if !has_actor_sources {
        for mode in [ItgTapExplosionMode::Dim, ItgTapExplosionMode::Bright] {
            for layer in direct_layers(mode) {
                push_tap_explosion_source(
                    source_from_layer(&layer),
                    &mut dim_sources,
                    &mut bright_sources,
                );
            }
        }
    }
    (dim_sources, bright_sources)
}

pub fn itg_tap_explosion_map_from_layers<L, T: Clone>(
    explosion_layers: &[L],
    mut layer_has_tap_command: impl FnMut(&L) -> bool,
    mut direct_layers: impl FnMut(ItgTapExplosionMode) -> Vec<L>,
    mut source_from_layer: impl FnMut(&L) -> ItgTapExplosionSource<T>,
    metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> TapExplosionMap<T> {
    let (dim_sources, bright_sources) = itg_partition_tap_explosion_layers(
        explosion_layers,
        &mut layer_has_tap_command,
        &mut direct_layers,
        &mut source_from_layer,
    );
    itg_tap_explosion_map_from_partitioned_sources(dim_sources, bright_sources, metric_command)
}

pub fn itg_tap_explosion_map_from_resolved_layers<T: Clone>(
    explosion_layers: &[ItgResolvedSprite<T>],
    mut direct_layers: impl FnMut(&str) -> Vec<ItgResolvedSprite<T>>,
    metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> TapExplosionMap<T> {
    itg_tap_explosion_map_from_layers(
        explosion_layers,
        |sprite| itg_has_tap_explosion_command(&sprite.commands),
        |mode| {
            let base_element = match mode {
                ItgTapExplosionMode::Dim => "Tap Explosion Dim",
                ItgTapExplosionMode::Bright => "Tap Explosion Bright",
            };
            direct_layers(base_element)
        },
        |sprite| {
            ItgTapExplosionSource::new(
                sprite.element.clone(),
                sprite.slot.clone(),
                sprite.commands.clone(),
            )
        },
        metric_command,
    )
}

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
