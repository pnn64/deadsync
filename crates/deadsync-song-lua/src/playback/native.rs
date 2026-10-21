use super::*;

// Ordinary charts share the native field/HUD buffers and segment iterator with
// scripted charts, but do not evaluate actor timelines, proxies or Lua cameras.
pub(super) fn compose_frame<P: deadsync_gameplay::GameplayProfileData, S: NoteskinSlot + Clone>(
    actors: &mut Vec<Actor>,
    state: &GameplayCoreState<P, S>,
    media: &SongMedia,
    scratch: &mut FrameScratch,
    view: FrameOptions,
    mut draw_layer: impl FnMut(ScreenLayer, &mut Vec<Actor>, FieldLayout),
    mut draw_field: impl FnMut(
        FieldFrame,
        &mut [NotefieldCameraCache; MAX_PLAYERS],
        &mut Vec<Actor>,
        &mut Vec<FlatDraw>,
        &mut Vec<Actor>,
        &mut Vec<FlatDraw>,
    ) -> deadsync_notefield::BuiltNotefield,
) -> GameplayActorSegments {
    actors.reserve(96);
    if view.show_song_visuals {
        draw_layer(ScreenLayer::Background, actors, FieldLayout::default());
    }
    push_background_cover(actors, state, view);
    if !view.hide_gameplay_hud {
        draw_layer(ScreenLayer::ExitPrompt, actors, FieldLayout::default());
        draw_layer(ScreenLayer::Lobby, actors, FieldLayout::default());
    }
    draw_layer(ScreenLayer::ExitFade, actors, FieldLayout::default());

    let versus = matches!(
        view.play_style,
        deadsync_gameplay::GameplayInputPlayStyle::Versus
            | deadsync_gameplay::GameplayInputPlayStyle::PumpVersus
    );
    let mut layout = FieldLayout {
        per_player_fields: [(usize::MAX, 0.0); MAX_PLAYERS],
        ..FieldLayout::default()
    };
    let mut players = [None; MAX_PLAYERS];
    for player in 0..if versus { 2 } else { 1 } {
        let placement = if (versus && player == 1)
            || (!versus
                && deadsync_gameplay::gameplay_runtime_player_is_p2(
                    view.play_style,
                    view.player_side,
                )) {
            FieldPlacement::P2
        } else {
            FieldPlacement::P1
        };
        let deadsync_notefield::BuiltNotefield {
            layout_center_x,
            mut field_camera,
            field_camera_generation,
            hud_parts,
            ..
        } = draw_field(
            FieldFrame {
                player,
                placement,
                judgment_visible: true,
                combo_visible: true,
                capture: ProxyCaptureRequests::default(),
                wrapper: Matrix4::IDENTITY,
                wrapper_visible: true,
            },
            &mut scratch.notefield_camera_cache,
            &mut scratch.notefield_actor_scratch[player],
            &mut scratch.notefield_flat_draw_scratch[player],
            &mut scratch.notefield_hud_actor_scratch[player],
            &mut scratch.notefield_hud_flat_draw_scratch[player],
        );
        layout.per_player_fields[player] = (player, layout_center_x);
        if player == 0 {
            layout.playfield_center_x = layout_center_x;
        }

        // Preserve the prepared native Player placement when logical viewport
        // and display metrics differ, as well as native chart attack transforms.
        // These are entry-time defaults; no actor timeline is evaluated here.
        let initial = state.song_lua_visuals().player_actors[player].initial_state;
        let (base_x, base_y) = if view.show_song_visuals {
            (initial.x, initial.y)
        } else {
            (layout_center_x, screen_center_y())
        };
        let transform = state.song_lua_player_transform(player);
        let transform = if view.apply_attacks {
            transform
        } else {
            Default::default()
        };
        let target_x =
            song_lua_player_target_x(transform.x, base_x, layout_center_x, view.notefield);
        let target_y = transform.y.unwrap_or(base_y);
        let assembly = if transform == deadsync_gameplay::SongLuaPlayerTransform::default()
            && target_x == layout_center_x
            && target_y == screen_center_y()
        {
            PlayerActorAssembly::DirectZ { z_shift: 0 }
        } else {
            scratch.player_actor_assembly_cache[player].resolve(
                false,
                true,
                SongLuaCaptureTransform {
                    z_shift: 0,
                    tint: [1.0; 4],
                    blend: None,
                    playfield_center_x: layout_center_x,
                    target_x,
                    target_y,
                    rotation_x: transform.rotation_x,
                    rotation_z: transform.rotation_z,
                    rotation_y: transform.rotation_y,
                    skew_x: transform.skew_x,
                    skew_y: transform.skew_y,
                    zoom_x: transform.zoom_x,
                    zoom_y: transform.zoom_y,
                    zoom_z: transform.zoom_z,
                },
            )
        };
        if let PlayerActorAssembly::DirectTransform {
            root_camera,
            field_camera_suffix,
            ..
        } = assembly
        {
            field_camera = Some(scratch.player_field_camera_cache[player].resolve(
                field_camera_generation,
                scratch.player_actor_assembly_cache[player].generation(),
                field_camera,
                root_camera,
                field_camera_suffix,
            ));
        }
        players[1 - player] = Some(PlayerActorSegment {
            player,
            assembly,
            field_camera,
            manual_hud_draw: false,
            hud_parts: deadsync_notefield::NotefieldHudParts {
                underlay_actors: 0,
                underlay_draws: 0,
                ..hud_parts
            },
        });
        scratch.notefield_underlay_actors[player].clear();
        scratch.notefield_underlay_draws[player].clear();
        scratch.notefield_underlay_actors[player]
            .extend(scratch.notefield_hud_actor_scratch[player].drain(..hud_parts.underlay_actors));
        scratch.notefield_underlay_draws[player].extend(
            scratch.notefield_hud_flat_draw_scratch[player].drain(..hud_parts.underlay_draws),
        );
    }
    if !view.hide_gameplay_hud {
        for layer in [
            ScreenLayer::Danger,
            ScreenLayer::Filter,
            ScreenLayer::Header,
            ScreenLayer::Hud,
        ] {
            draw_layer(layer, actors, layout);
        }
    }
    let insert = actors.len();
    if !view.hide_gameplay_hud {
        draw_layer(ScreenLayer::Stage, actors, layout);
    }
    if view.show_song_visuals
        && let Some(key) = media.current_foreground_key.clone()
    {
        actors.push(cover_sprite(
            key,
            screen_center_x(),
            screen_center_y(),
            screen_width(),
            screen_height(),
            1.0,
            1000,
        ));
    }
    if !view.hide_gameplay_hud {
        draw_layer(ScreenLayer::System, actors, layout);
    }
    GameplayActorSegments {
        insert,
        players,
        direct_proxy_len: 0,
        underlay_visible: !view.hide_gameplay_hud,
        screen_projection: Matrix4::IDENTITY,
    }
}

pub(super) fn push_background_cover<
    P: deadsync_gameplay::GameplayProfileData,
    S: NoteskinSlot + Clone,
>(
    actors: &mut Vec<Actor>,
    state: &GameplayCoreState<P, S>,
    view: FrameOptions,
) {
    let cover_alpha = |player_idx: usize| -> f32 {
        if player_idx >= state.num_players() {
            return 0.0;
        }
        let profile_cover = f32::from(view.hide_song_bg[player_idx]);
        let attack_cover = if view.apply_attacks {
            state
                .effective_visibility_effects_for_player(player_idx)
                .cover
        } else {
            0.0
        };
        profile_cover.max(attack_cover).clamp(0.0, 1.0)
    };
    let left_cover = cover_alpha(0);
    let right_cover = if state.num_players() > 1 {
        cover_alpha(1)
    } else {
        left_cover
    };
    let sw = screen_width();
    let sh = screen_height();
    let cx = screen_center_x();
    if left_cover > 0.0 || right_cover > 0.0 {
        if (left_cover - right_cover).abs() <= 0.001 {
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(sw, sh):
                diffuse(0.0, 0.0, 0.0, left_cover.max(right_cover)):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
        } else {
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(0.0, 0.0):
                zoomto(cx, sh):
                faderight(0.1):
                diffuse(0.0, 0.0, 0.0, left_cover):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
            actors.push(deadlib_present::__act_from_builder!((
                align(0.0, 0.0): xy(cx, 0.0):
                zoomto(sw - cx, sh):
                fadeleft(0.1):
                diffuse(0.0, 0.0, 0.0, right_cover):
                z(-99)
            ) deadlib_assets::SpriteBuilder::solid()));
        }
    }
}
