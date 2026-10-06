//! Read-only access to production actor math for native ITGmania fixtures.
//!
//! ITGmania serializes matrices by row while glam stores columns. The adapter
//! below transposes storage only; actor coordinates and projected positions are
//! compared in ITGmania's logical top-left screen space without exemptions.

use super::*;

#[derive(Clone, Copy, Debug)]
pub struct EffectSample {
    pub tint: [f32; 4],
    pub glow: [f32; 4],
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct SpriteVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[must_use]
pub fn effect_sample(state: SongLuaOverlayState, time: f32, beat: f32) -> EffectSample {
    let mut sample = EffectSample {
        tint: state.diffuse,
        glow: state.glow,
        position: [state.x, state.y, state.z],
        scale: {
            let [x, y] = song_lua_overlay_axis_scale(state);
            [x, y, song_lua_overlay_z_scale(state)]
        },
        rotation: [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
    };
    song_lua_apply_overlay_effect(
        state,
        time,
        beat,
        0,
        &mut sample.tint,
        &mut sample.glow,
        &mut sample.position,
        &mut sample.scale,
        &mut sample.rotation,
    );
    sample
}

/// Read the deterministic draw transform, keeping the base Lua state unchanged.
#[must_use]
pub fn transform_state(state: SongLuaOverlayState, clock: [f32; 2]) -> SongLuaOverlayState {
    song_lua_pulse_parent(song_lua_pre_draw_state(state, clock), clock)
}

#[must_use]
pub fn vibration_magnitude(state: SongLuaOverlayState) -> [f32; 3] {
    song_lua_overlay_vibrate_magnitude(state)
}

#[must_use]
pub fn vibration_sample(mut position: [f32; 3], magnitude: [f32; 3], jitter: [f32; 3]) -> [f32; 3] {
    song_lua_apply_vibration(&mut position, magnitude, jitter);
    position
}

#[must_use]
pub fn actor_matrix(
    position: [f32; 3],
    rotation: [f32; 3],
    scale: [f32; 3],
    skew: [f32; 2],
) -> [[f32; 4]; 4] {
    matrix_rows(
        Matrix4::from_translation(Vector3::from(position))
            * song_lua_overlay_local_transform(rotation, skew[0], skew[1])
            * Matrix4::from_scale(Vector3::from(scale)),
    )
}

#[must_use]
pub fn sprite_matrix(
    position: [f32; 3],
    rotation: [f32; 3],
    base_rotation: [f32; 3],
    zoom: [f32; 3],
    base_zoom: [f32; 3],
    size: [f32; 2],
    alignment: [f32; 2],
    skew: [f32; 2],
) -> [[f32; 4]; 4] {
    let rotation = std::array::from_fn(|axis| rotation[axis] + base_rotation[axis]);
    let scale = std::array::from_fn(|axis| zoom[axis] * base_zoom[axis]);
    let align = Vector3::new(
        (0.5 - alignment[0]) * size[0],
        (0.5 - alignment[1]) * size[1],
        0.0,
    );
    matrix_rows(
        Matrix4::from_translation(Vector3::from(position))
            * song_lua_overlay_local_transform(rotation, 0.0, 0.0)
            * Matrix4::from_scale(Vector3::from(scale))
            * Matrix4::from_translation(align)
            * song_lua_overlay_local_transform([0.0; 3], skew[0], skew[1]),
    )
}

#[must_use]
pub fn overlay_sprite_matrix(state: SongLuaOverlayState, size: [f32; 2]) -> [[f32; 4]; 4] {
    matrix_rows(song_lua_overlay_sprite_matrix(
        state,
        size,
        [state.x, state.y, state.z],
        [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
        [1.0; 3],
    ))
}

#[must_use]
pub fn multiply_matrices(left: [[f32; 4]; 4], right: [[f32; 4]; 4]) -> [[f32; 4]; 4] {
    matrix_rows(matrix_from_rows(left) * matrix_from_rows(right))
}

#[must_use]
pub fn matrix_rows(matrix: Matrix4) -> [[f32; 4]; 4] {
    let columns = matrix.to_cols_array_2d();
    std::array::from_fn(|row| std::array::from_fn(|column| columns[column][row]))
}

fn matrix_from_rows(rows: [[f32; 4]; 4]) -> Matrix4 {
    Matrix4::from_cols_array_2d(&std::array::from_fn(|column| {
        std::array::from_fn(|row| rows[row][column])
    }))
}

#[must_use]
pub fn view_projection(
    screen: [u32; 2],
    fov: f32,
    vanishpoint: [f32; 2],
) -> ([[f32; 4]; 4], [[f32; 4]; 4]) {
    deadlib_present::space::set_current_window_px(screen[0], screen[1]);
    deadlib_present::space::set_current_metrics(deadlib_present::space::Metrics::centered(
        screen[0] as f32,
        screen[1] as f32,
    ));
    let (view, projection) = song_lua_overlay_view_proj(
        SongLuaOverlayState {
            fov: Some(fov),
            vanishpoint: Some(vanishpoint),
            ..SongLuaOverlayState::default()
        },
        screen[0] as f32,
        screen[1] as f32,
    )
    .expect("valid conformance camera");
    (matrix_rows(view), matrix_rows(projection))
}

#[must_use]
pub fn project_world(view_projection: [[f32; 4]; 4], world: [f32; 4]) -> [f32; 4] {
    (matrix_from_rows(view_projection) * Vector4::from(world)).to_array()
}

#[must_use]
pub fn crop_fade_vertices(state: SongLuaOverlayState, size: [f32; 2]) -> Vec<SpriteVertex> {
    let (center, cropped_size) =
        song_lua_overlay_rect(state, size, 1.0, 1.0, 1.0, 1.0).expect("visible sprite");
    let actor = song_lua_flat_skewed_overlay_actor(
        Arc::from("conformance"),
        state.diffuse,
        BlendMode::Alpha,
        0,
        center,
        cropped_size,
        [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg],
        song_lua_overlay_uvs(state, None, &[], false, false, 0.0, 0.0),
        state,
        false,
        false,
        0.0,
        None,
    )
    .expect("visible conformance sprite");
    let Actor::TexturedMesh { vertices, .. } = actor else {
        panic!("crop/fade conformance actor did not produce a textured mesh");
    };
    vertices
        .iter()
        .map(|vertex| SpriteVertex {
            position: vertex.pos,
            uv: vertex.uv,
            color: vertex.color,
        })
        .collect()
}

#[must_use]
pub fn stable_draw_order(input: &[(String, i32)]) -> Vec<String> {
    let overlays: Vec<
        crate::SongLuaOverlayActor<
            crate::SongLuaOverlayKind<(), TexturedMeshVertex, TextAttribute>,
        >,
    > = input
        .iter()
        .map(|(name, draw_order)| SongLuaOverlayActor {
            kind: SongLuaOverlayKind::Actor,
            name: Some(name.clone()),
            parent_index: None,
            initial_state: SongLuaOverlayState {
                draw_order: *draw_order,
                ..SongLuaOverlayState::default()
            },
            message_commands: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut indices = (0..overlays.len()).collect::<Vec<_>>();
    song_lua_sort_static_children(&overlays, &mut indices);
    indices
        .into_iter()
        .map(|index| overlays[index].name.clone().expect("test actor name"))
        .collect()
}

/// Compose a complete layer through the same parent-inheritance path used by
/// gameplay. Whole-song archive tests feed sampled local states into this
/// adapter; no test-only transform implementation is involved.
#[must_use]
pub fn compose_overlay_states<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    local_states: &[SongLuaOverlayState],
    screen: [f32; 2],
    clock: [f32; 2],
) -> Vec<SongLuaOverlayState> {
    let mut out = Vec::with_capacity(overlays.len());
    song_lua_overlay_states_from_local_all_into(
        overlays,
        local_states,
        screen[0],
        screen[1],
        &mut out,
        clock,
    );
    // Audit the same texture-space states used by the capture renderer.
    // Global composition alone includes placement that BeginRenderingTo resets.
    let topology = SongLuaOverlayTopologyIndex::new(overlays);
    if topology
        .aft_ancestors
        .iter()
        .any(|index| index.get().is_some())
    {
        let order = song_lua_overlay_order_cache_from(overlays, &[]);
        let mut capture = out.clone();
        for (index, overlay) in overlays.iter().enumerate() {
            if !matches!(overlay.kind, SongLuaOverlayKind::ActorFrameTexture { .. }) {
                continue;
            }
            song_lua_fill_capture_overlay_states(
                overlays,
                &out,
                local_states,
                &order,
                index,
                screen[0],
                screen[1],
                &mut capture,
                clock,
            );
            for (child, ancestor) in topology.aft_ancestors.iter().enumerate() {
                if ancestor.get() == Some(index) {
                    out[child] = capture[child];
                }
            }
        }
    }
    out
}

/// Headless production overlay builder used by whole-song archives. Texture
/// dimensions come from the normal registry populated during Lua compilation;
/// one transparent pixel buffer per compiled key is enough because actor
/// composition consumes metadata and handles, not sampled pixels.
pub struct WholeSongComposer {
    assets: AssetManager,
    mesh_scratch: Vec<SongLuaProjectedMeshScratch>,
    topology: SongLuaOverlayTopologyIndex,
    manual: SongLuaManualScratch,
}

impl WholeSongComposer {
    /// Stable resource identity allocated by the production song topology.
    #[must_use]
    pub fn capture_handle(&self, index: usize) -> Option<deadlib_render_core::TextureHandle> {
        self.topology
            .aft_texture_handles
            .get(index)
            .copied()
            .filter(|handle| *handle != 0)
    }

    #[must_use]
    pub fn new<S: NoteskinSlot + Clone>(overlays: &[SongLuaOverlayActor<S>]) -> Self {
        let mut assets = AssetManager::new();
        for overlay in overlays {
            match &overlay.kind {
                SongLuaOverlayKind::Quad => queue_texture(&mut assets, &white_texture_key()),
                SongLuaOverlayKind::Sprite {
                    texture_key,
                    textures,
                    ..
                } => {
                    queue_texture(&mut assets, texture_key);
                    for texture in textures.iter() {
                        queue_texture(&mut assets, &texture.key);
                    }
                }
                SongLuaOverlayKind::ActorMultiVertex {
                    texture_key: Some(texture_key),
                    ..
                } => queue_texture(&mut assets, texture_key),
                SongLuaOverlayKind::NoteskinActor { slots } => {
                    for slot in slots.iter() {
                        queue_texture(&mut assets, slot.texture_key_shared().as_ref());
                    }
                }
                SongLuaOverlayKind::Model { layers } => {
                    for layer in layers.iter() {
                        queue_texture(&mut assets, &layer.texture_key);
                    }
                }
                _ => {}
            }
        }
        Self {
            assets,
            mesh_scratch: song_lua_projected_mesh_scratch_for(overlays),
            topology: SongLuaOverlayTopologyIndex::new(overlays),
            manual: SongLuaManualScratch::new(overlays),
        }
    }

    /// Initialize the same song-entry draw plan as gameplay from compile data.
    pub fn set_draw_frames<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
        frames: &[SongLuaDrawFrame],
    ) {
        self.manual = SongLuaManualScratch::with_frames(overlays, Arc::from(frames));
    }

    /// Materialize every pass together before inspecting leaves. Repeated RGB
    /// draws therefore cannot pass by mutating one shared geometry buffer.
    #[must_use]
    pub fn render_manual_frame<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
        states: &[SongLuaOverlayState],
        screen: [f32; 2],
        seconds: f32,
        beat: f32,
    ) -> Vec<(usize, deadlib_render_core::RenderFrame)> {
        let Some(index) = self.manual.frame_at(seconds) else {
            return Vec::new();
        };
        self.manual.begin_frame();
        let mut actors = Vec::new();
        let mut targets = Vec::new();
        for batch in 0..self.manual.frames[index].owners.len() {
            let owner = self.manual.frames[index].owners[batch];
            song_lua_draw_owner(
                &mut actors,
                &mut targets,
                overlays,
                states,
                &self.topology,
                &mut self.manual,
                index,
                owner,
                &SongLuaScreenProxySources::default(),
                &self.assets,
                screen,
                [seconds, beat],
                seconds,
                0,
            );
        }
        self.manual.frames[index]
            .ops
            .iter()
            .enumerate()
            .filter_map(|(op, draw)| {
                let SongLuaDrawOp::Draw {
                    source: SongLuaDrawSource::Overlay(overlay),
                    ..
                } = draw
                else {
                    return None;
                };
                if !matches!(
                    overlays[*overlay].kind,
                    SongLuaOverlayKind::ActorMultiVertex { .. }
                ) {
                    return None;
                }
                Some((
                    *overlay,
                    deadlib_present::compose::build_screen_with_texture_context(
                        &self.manual.banks[self.manual.bank][op].actors,
                        [0.0; 4],
                        &deadlib_present::space::Metrics::centered(screen[0], screen[1]),
                        &font::FontMap::default(),
                        seconds,
                        self.assets.texture_context(),
                    ),
                ))
            })
            .collect()
    }

    /// Exercise the warmed gameplay builder and final draw-pass composition,
    /// including the inherited camera and noteskin model textures.
    #[must_use]
    pub fn render_overlay<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
        states: &[SongLuaOverlayState],
        index: usize,
        screen: [f32; 2],
        seconds: f32,
        beat: f32,
    ) -> deadlib_render_core::RenderFrame {
        let mut actors = Vec::new();
        if matches!(overlays[index].kind, SongLuaOverlayKind::AftSprite { .. }) {
            if let Some(target) = self.topology.aft_sprite_targets[index].get()
                && let Some(size) = song_lua_aft_size(&overlays[target], states[target])
                && let Some(built) = build_song_lua_aft_sprite_actor(
                    states[index],
                    self.topology.aft_texture_handles[target],
                    size,
                    0,
                    screen[0],
                    screen[1],
                    seconds,
                    beat,
                    seconds,
                    self.mesh_scratch.get_mut(index),
                )
            {
                actors.extend(built);
            }
        } else if append_song_lua_multi_actor_overlay(
            &mut actors,
            &overlays[index],
            states[index],
            &self.assets,
            0,
            screen[0],
            screen[1],
            seconds,
            beat,
            seconds,
            self.mesh_scratch.get_mut(index),
        )
        .is_none()
            && let Some(built) = build_song_lua_overlay_actor_with_scratch(
                &overlays[index],
                states[index],
                song_lua_overlay_camera_state(overlays, states, overlays[index].parent_index),
                &self.assets,
                0,
                screen[0],
                screen[1],
                seconds,
                beat,
                seconds,
                self.topology.texture_handle(index),
                self.mesh_scratch.get_mut(index),
            )
        {
            actors.extend(built);
        }
        deadlib_present::compose::build_screen_with_texture_context(
            &actors,
            [0.0; 4],
            &deadlib_present::space::Metrics::centered(screen[0], screen[1]),
            &font::FontMap::default(),
            seconds,
            self.assets.texture_context(),
        )
    }

    #[must_use]
    pub fn actor_count<S: NoteskinSlot + Clone>(
        &self,
        overlays: &[SongLuaOverlayActor<S>],
        states: &[SongLuaOverlayState],
        screen: [f32; 2],
        seconds: f32,
        beat: f32,
    ) -> usize {
        overlays
            .iter()
            .enumerate()
            .map(|(index, overlay)| {
                let state = states.get(index).copied().unwrap_or_default();
                let camera = song_lua_overlay_camera_state(overlays, states, overlay.parent_index);
                build_song_lua_overlay_actor(
                    overlay,
                    state,
                    camera,
                    &self.assets,
                    i16::try_from(index).unwrap_or(i16::MAX),
                    screen[0],
                    screen[1],
                    seconds,
                    beat,
                    seconds,
                )
                .map_or(0, |actors| actors.len())
            })
            .sum()
    }
}

fn queue_texture(assets: &mut AssetManager, key: &str) {
    if assets.has_texture_key(key) {
        return;
    }
    let dims = deadlib_assets::texture_dims(key);
    let width = dims.map_or(1, |dims| dims.w.max(1));
    let height = dims.map_or(1, |dims| dims.h.max(1));
    assets.queue_texture_upload(key.to_owned(), image::RgbaImage::new(width, height));
}

/// Wrap an already compiled primary fixture without selecting additional layers.
pub fn prepared_primary<S: NoteskinSlot + Clone>(
    compiled: Option<CompiledSongLua<S>>,
) -> PreparedGameplaySongLua<S> {
    PreparedGameplaySongLua {
        primary: compiled.map(|compiled| super::prepare::GameplayCompiledSongLua {
            compiled,
            compile_ms: 0.0,
        }),
        ..Default::default()
    }
}
