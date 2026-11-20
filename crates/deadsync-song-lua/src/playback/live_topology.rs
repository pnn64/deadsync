//! Live wrappers change ancestry without moving existing visual/resource slots.
//! Refresh the affected layout when parents change, and AFT bindings when their
//! dependencies change. Retain capture handles, banks and manual draw history.
use super::*;

impl SongLuaOverlayOrderCache {
    fn has_live_parents<S: NoteskinSlot + Clone>(
        &self,
        overlays: &[SongLuaOverlayActor<S>],
    ) -> bool {
        self.last_draw_orders.len() == overlays.len()
            && self.child_lists.iter().enumerate().all(|(list, children)| {
                children.iter().all(|&index| {
                    let parent = overlays[index].parent_index.filter(|&parent| parent < overlays.len());
                    song_lua_overlay_child_list_index(parent) == list
                })
            })
    }

    pub(super) fn enable_dynamic(&mut self) {
        self.dynamic_local_indices = (0..self.last_draw_orders.len()).collect();
        let mut composed = Vec::with_capacity(self.last_draw_orders.len());
        song_lua_push_static_order(&self.child_lists, None, &mut composed);
        self.dynamic_composed_indices = composed.into_boxed_slice();
        self.dynamic_draw_order.fill(true);
        self.static_root_order = None;
    }
}

impl SongLuaOverlayTopologyIndex {
    pub(super) fn refresh_texture_bindings<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
    ) -> bool {
        debug_assert_eq!(self.aft_texture_dependencies.len(), overlays.len());
        let changed = overlays.iter().zip(&self.aft_texture_dependencies).any(|(overlay, cached)| {
            Self::texture_dependency(overlay) != (cached.0.as_deref(), cached.1.as_deref())
        });
        if !changed {
            return false;
        }
        for (overlay, cached) in overlays.iter().zip(&mut self.aft_texture_dependencies) {
            let (allocated, sampled) = Self::texture_dependency(overlay);
            if (cached.0.as_deref(), cached.1.as_deref()) != (allocated, sampled) {
                *cached = (allocated.map(str::to_owned), sampled.map(str::to_owned));
            }
        }
        self.aft_sprite_indices = self.aft_texture_dependencies.iter().enumerate()
            .filter_map(|(index, (_, sampled))| sampled.is_some().then_some(index))
            .collect();
        self.aft_sprite_targets.fill(SongLuaOverlayIndex::default());
        for &index in &self.aft_sprite_indices {
            let name = self.aft_texture_dependencies[index].1.as_deref()
                .expect("sampling index was built from these dependencies");
            self.aft_sprite_targets[index] = SongLuaOverlayIndex::new(
                song_lua_overlay_capture_index_by_name(overlays, name),
            );
        }
        true
    }

    fn retain_capture_handles(&mut self, previous: &Self) {
        self.aft_texture_handles[..previous.aft_texture_handles.len()]
            .copy_from_slice(&previous.aft_texture_handles);
        self.dynamic_camera_scope = previous.dynamic_camera_scope;
    }

    pub(super) fn refresh_wrappers<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
    ) {
        debug_assert!(
            overlays[self.aft_ancestors.len()..]
                .iter()
                .all(|overlay| matches!(overlay.kind, SongLuaOverlayKind::WrapperState))
        );
        let mut next = Self::new(overlays);
        next.retain_capture_handles(self);
        *self = next;
    }
}

impl SongLuaProxyRequestIndex {
    fn refresh_wrappers<S: NoteskinSlot + Clone>(&mut self, overlays: &[SongLuaOverlayActor<S>]) {
        let mut next = Self::new(overlays);
        next.topology.retain_capture_handles(&self.topology);
        *self = next;
    }
}

impl SongLuaManualScratch {
    pub(super) fn refresh_owners<S: NoteskinSlot + Clone>(
        &mut self,
        overlays: &[SongLuaOverlayActor<S>],
    ) {
        let mut owner_flags = vec![false; overlays.len()];
        for frame in self.frames.iter().filter(|frame| !frame.immediate) {
            for &owner in &frame.owners {
                owner_flags[owner] = true;
            }
        }
        self.owners = (0..overlays.len())
            .map(|index| {
                let mut current = Some(index);
                while let Some(index) = current {
                    if owner_flags[index] {
                        return Some(index);
                    }
                    current = overlays[index].parent_index;
                }
                None
            })
            .collect();
    }
}

fn refresh_layer<S: NoteskinSlot + Clone>(
    overlays: &[SongLuaOverlayActor<S>],
    eases: &[SongLuaOverlayEaseWindowRuntime],
    order: &mut SongLuaOverlayOrderCache,
    aft: &mut SongLuaAftCaptureScratch,
    meshes: &mut Vec<SongLuaProjectedMeshScratch>,
    messages: &mut Vec<SongLuaMessageStateCache>,
    local: &mut Vec<SongLuaOverlayState>,
    composed: &mut Vec<SongLuaOverlayState>,
) {
    let mut next = song_lua_overlay_order_cache_from(overlays, eases);
    // Existing tracks and command identities have not moved. Keep their clocks.
    if next.update_cursors.len() == order.update_cursors.len() {
        next.update_cursors.copy_from_slice(&order.update_cursors);
    }
    for command in &mut next.tap_commands {
        if let Some(previous) = order.tap_commands.iter().find(|previous| {
            previous.overlay == command.overlay && previous.command == command.command
        }) {
            command.glow_started_at = previous.glow_started_at;
        }
    }
    *order = next;
    aft.slots.resize_with(overlays.len(), || None);
    aft.manual.refresh_owners(overlays);
    meshes.extend(song_lua_projected_mesh_scratch_for(
        &overlays[meshes.len()..],
    ));
    messages.resize(overlays.len(), SongLuaMessageStateCache::default());
    local.extend(
        overlays[local.len()..]
            .iter()
            .map(|overlay| overlay.initial_state),
    );
    // Recompute every existing child's new ancestry on the next composition.
    composed.clear();
}

impl FrameScratch {
    pub(super) fn refresh_live_wrappers<S: NoteskinSlot + Clone>(
        &mut self,
        visuals: &SongLuaRuntimeVisuals<
            SongLuaOverlayActor<S>,
            SongLuaCapturedActor,
            SongLuaRuntimeOverlayStateDelta,
        >,
    ) {
        if !self.dynamic_scene {
            return;
        }
        let mut changed = false;
        if !self.song_lua_overlay_order.has_live_parents(&visuals.overlays) {
            self.song_lua_proxy_request_index
                .refresh_wrappers(&visuals.overlays);
            refresh_layer(
                &visuals.overlays,
                &visuals.overlay_eases,
                &mut self.song_lua_overlay_order,
                &mut self.song_lua_aft_capture_scratch,
                &mut self.song_lua_projected_mesh_scratch,
                &mut self.song_lua_message_state_cache,
                &mut self.song_lua_local_state_scratch,
                &mut self.song_lua_overlay_state_scratch,
            );
            changed = true;
        }
        for (index, layer) in visuals.background_visual_layers.iter().enumerate() {
            if self.song_lua_background_visual_layer_orders[index]
                .has_live_parents(&layer.overlays)
            {
                continue;
            }
            self.song_lua_background_overlay_topology_indices[index]
                .refresh_wrappers(&layer.overlays);
            refresh_layer(
                &layer.overlays,
                &layer.overlay_eases,
                &mut self.song_lua_background_visual_layer_orders[index],
                &mut self.song_lua_background_aft_capture_scratch[index],
                &mut self.song_lua_background_projected_mesh_scratch[index],
                &mut self.song_lua_background_layer_message_state_cache[index],
                &mut self.song_lua_background_layer_local_state_scratch[index],
                &mut self.song_lua_background_layer_state_scratch[index],
            );
            changed = true;
        }
        for (index, layer) in visuals.foreground_visual_layers.iter().enumerate() {
            if self.song_lua_foreground_visual_layer_orders[index]
                .has_live_parents(&layer.overlays)
            {
                continue;
            }
            self.song_lua_foreground_proxy_request_indices[index].refresh_wrappers(&layer.overlays);
            refresh_layer(
                &layer.overlays,
                &layer.overlay_eases,
                &mut self.song_lua_foreground_visual_layer_orders[index],
                &mut self.song_lua_foreground_aft_capture_scratch[index],
                &mut self.song_lua_foreground_projected_mesh_scratch[index],
                &mut self.song_lua_foreground_layer_message_state_cache[index],
                &mut self.song_lua_foreground_layer_local_state_scratch[index],
                &mut self.song_lua_foreground_layer_state_scratch[index],
            );
            changed = true;
        }
        // SetTexture and delayed Create keep the same visual slots. Refresh
        // just their dependencies even when no wrapper was appended this frame.
        self.song_lua_proxy_request_index.topology.refresh_texture_bindings(&visuals.overlays);
        for (index, layer) in visuals.background_visual_layers.iter().enumerate() {
            self.song_lua_background_overlay_topology_indices[index]
                .refresh_texture_bindings(&layer.overlays);
        }
        for (index, layer) in visuals.foreground_visual_layers.iter().enumerate() {
            self.song_lua_foreground_proxy_request_indices[index].topology
                .refresh_texture_bindings(&layer.overlays);
        }
        if !changed {
            return;
        }
        self.enable_dynamic_scene();
        let counts = std::iter::once(visuals.overlays.len())
            .chain(
                visuals
                    .background_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            )
            .chain(
                visuals
                    .foreground_visual_layers
                    .iter()
                    .map(|layer| layer.overlays.len()),
            );
        let max = counts.clone().max().unwrap_or(0);
        let total = counts.sum();
        self.song_lua_capture_visit_scratch.marks.resize(max, 0);
        reserve(&mut self.song_lua_capture_state_scratch, max);
        reserve(&mut self.song_lua_order_scratch, max);
        reserve(&mut self.song_lua_capture_order_scratch, max);
        reserve(&mut self.song_lua_screen_projection_ranges, total);
        reserve(&mut self.screen_capture_originals.ranges, total);
    }
}

fn reserve<T>(values: &mut Vec<T>, capacity: usize) {
    if values.capacity() < capacity {
        values.reserve_exact(capacity.saturating_sub(values.len()));
    }
}
