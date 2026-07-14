use super::App;
use crate::navigation::TransitionState;
use crate::pad_config::{
    PadConfigFsrTarget, apply_pad_commands, pad_config_fsr_plan, pad_config_profile_cursor,
    pad_config_profile_entries,
};
use crate::smx_config::{
    SmxAssignmentSource, resolve_smx_pad_config, smx_autoprompt_plan, smx_light_brightness_plan,
    smx_light_preview_restore_auto, smx_options_light_preview_active,
    smx_player_options_light_preview_allowed, smx_runtime_assignment_plan,
};
use deadsync_config::prelude as config;
use deadsync_profile::{compat as profile, pad_config as pad_profile_data, pad_config_sync};
use deadsync_theme_simply_love::screens::SimplyLoveScreen as CurrentScreen;
use deadsync_theme_simply_love::screens::{self, options, player_options};

impl App {
    /// Drive the Configure Pads screen: enable live FSR reads while it's open,
    /// apply queued threshold edits, and refresh the pad snapshot.
    #[inline(always)]
    pub(super) fn sync_pad_config_fsr(&mut self) {
        use screens::pad_config;
        let screen = self.state.screens.current_screen;
        let cfg = config::get();
        let Some(plan) = pad_config_fsr_plan(
            screen,
            cfg.use_fsrs,
            self.state
                .screens
                .select_music_state
                .pad_config_overlay_visible,
            self.fsr_pads_active,
            cfg.smx_manages_pad_config,
        ) else {
            return;
        };
        match plan.monitor_active {
            Some(true) => {
                self.fsr_monitor.set_active(true);
                self.fsr_pads_active = true;
            }
            Some(false) => {
                self.fsr_monitor.set_active(false);
                self.fsr_pads_active = false;
            }
            None => {}
        }
        let Some(target_kind) = plan.target else {
            return;
        };

        // `target` is the screen state we're driving: the standalone Configure
        // Pads screen, or the Song Select overlay. A macro (not a method) so each
        // use re-borrows inline — the borrow has to release between phases so the
        // disjoint `self.pad_config_sync` access in between is allowed.
        macro_rules! target {
            () => {
                if target_kind == PadConfigFsrTarget::Screen {
                    &mut self.state.screens.pad_config_state
                } else {
                    &mut self.state.screens.select_music_state.pad_config_overlay
                }
            };
        }

        {
            let pads = self.fsr_monitor.poll_pads();
            // Drain queued edits in a short-lived borrow so we can touch
            // `smx_applied` (a sibling of `target`) below without a borrow clash.
            let commands = {
                let target = target!();
                pad_config::take_commands(target)
            };
            apply_pad_commands(&mut self.fsr_monitor, &mut self.pad_config_sync, commands);
            let target = target!();
            pad_config::set_pads(target, pads);
            pad_config::set_managed_active(target, plan.managed_active);

            // Saving / profile management is only offered in-session, for a cursor
            // pad that is an SMX pad mapped to a joined local profile (the Options
            // screen never has a profile). Resolve it once and reuse for both the
            // save gate and the management list. Capture the cursor device (Copy)
            // so the later `smx_applied` read doesn't alias the `target` borrow.
            let cursor_dev =
                pad_config_profile_cursor(target_kind, pad_config::selected_device(target));
            let cursor_profile = cursor_dev.and_then(|dev| {
                // Slot is the source of truth for player side (the SDK orders
                // slot 0 = P1, slot 1 = P2 per the pad→player assignment), so map
                // the config by slot, not the raw jumper bit.
                profile::active_local_profile_id_for_pad(dev.index == 1)
            });
            pad_config::set_save_available(target, cursor_profile.is_some());
            // Mark the config currently applied to the cursor pad's slot, read
            // straight from the authoritative controller (no screen-state alias).
            let active_name = cursor_dev.and_then(|dev| {
                self.pad_config_sync.applied[dev.index]
                    .as_ref()
                    .filter(|a| !a.preset)
                    .map(|a| a.name.clone())
            });
            // Cursor pad identity (Copy device → safe to read alongside the
            // controller borrow below). The config *list* only depends on the
            // profile + sensor type; `is_default` is per-serial, computed per entry.
            let cursor_pad_type = cursor_dev
                .and_then(|dev| deadsync_smx::pad_sensor_type(dev.index))
                .map(|t| t.as_str().to_owned());
            let cursor_serial = cursor_dev.map(|dev| deadsync_smx::get_info(dev.index).serial);
            // Cache + markers are keyed by pad slot (always unambiguous, unlike the
            // player side, which two pads can share).
            let cursor_slot = cursor_dev.map(|dev| dev.index);

            // Refresh the cached config list only when its inputs changed — no
            // per-frame `padconfig.ini` read. Management edits clear the cache via an
            // Invalidate intent (drained in `apply_smx_managed_preset`).
            if let (Some(pid), Some(pad)) = (cursor_profile.as_deref(), cursor_slot)
                && self
                    .pad_config_sync
                    .profiles_stale(pad, Some(pid), cursor_pad_type.as_deref())
            {
                let list = deadsync_profile::compat::load_pad_configs(pid)
                    .into_iter()
                    .filter(|c| {
                        pad_profile_data::config_matches(
                            c,
                            deadsync_smx::BACKEND_ID,
                            cursor_pad_type.as_deref(),
                        )
                    })
                    .collect();
                self.pad_config_sync.store_profiles(
                    pad,
                    Some(pid.to_owned()),
                    cursor_pad_type.clone(),
                    list,
                );
            }

            // Build the overlay list from the cache; active/default are derived live
            // (cheap, no I/O) since they depend on the marker / this pad's serial.
            let profiles = match cursor_slot {
                Some(pad) if cursor_profile.is_some() => pad_config_profile_entries(
                    self.pad_config_sync.profiles_for(pad),
                    active_name.as_deref(),
                    cursor_serial.as_deref(),
                ),
                _ => Vec::new(),
            };
            // Re-borrow target (released for the controller access above).
            let target = target!();
            pad_config::set_profiles(target, profiles);
        }
    }

    /// Drain UI intents, then (when "DeadSync manages pad config" is on) resolve
    /// and apply the right pad config to each connected StepManiaX pad: this pad's
    /// per-pad default → a global default → the machine built-in preset (also the
    /// fallback for Guest / no-config players). Reactive: when the active player
    /// changes, a no-config/guest player resets the pad to the machine preset. A
    /// cheap per-pad signature avoids loading config files or rewriting the pad
    /// unless something relevant changed (so manual edits aren't clobbered).
    /// Finally mirror the markers to the screen. Off → DeadSync writes nothing.
    /// Auto-save the pad→player assignment when none is saved yet:
    /// - **Two pads, distinct jumpers:** persist the jumper-derived P1/P2 map.
    /// - **Single pad:** persist its hardware jumper side.
    /// The ambiguous same-jumper-two-pad case is left for the user to assign.
    pub(super) fn reconcile_smx_assignment(&mut self) {
        let screen = self.state.screens.current_screen;
        let smx_input = config::get().smx_input;
        if matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice) || !smx_input {
            return;
        }
        let (p1, p2) = config::smx_pad_assignment();
        if p1.is_some() || p2.is_some() {
            return;
        }
        let a = deadsync_smx::get_info(0);
        let b = deadsync_smx::get_info(1);
        let Some(plan) =
            smx_runtime_assignment_plan(screen, smx_input, p1.as_deref(), p2.as_deref(), &a, &b)
        else {
            return;
        };
        match plan.source {
            SmxAssignmentSource::DistinctJumpers => log::info!(
                "SMX: auto-saving pad assignment from jumpers (P1={}, P2={})",
                plan.p1_serial.as_deref().unwrap_or_default(),
                plan.p2_serial.as_deref().unwrap_or_default(),
            ),
            SmxAssignmentSource::SingleP1 | SmxAssignmentSource::SingleP2 => log::info!(
                "SMX: single pad connected, auto-assigning to its jumper side P{} (serial={})",
                if plan.source == SmxAssignmentSource::SingleP2 {
                    2
                } else {
                    1
                },
                plan.p1_serial
                    .as_deref()
                    .or(plan.p2_serial.as_deref())
                    .unwrap_or_default(),
            ),
        }
        crate::smx_config::set_smx_assignment(plan.p1_serial, plan.p2_serial);
    }

    /// From the main Menu, if two pads share a P1/P2 jumper and no assignment
    /// resolves them, open the assignment screen automatically (once per conflict
    /// episode). Cancelling won't re-prompt until the conflict clears and returns.
    pub(super) fn maybe_autoprompt_smx_assign(&mut self) {
        let plan = smx_autoprompt_plan(
            self.state.screens.current_screen,
            matches!(self.state.shell.transition, TransitionState::Idle),
            config::get().smx_input,
            deadsync_smx::conflict_warning_active(),
            self.state.screens.smx_autoprompt_latched,
        );
        if plan.unlatch {
            self.state.screens.smx_autoprompt_latched = false;
        }
        if !plan.navigate_to_assign {
            return;
        }
        if plan.latch {
            self.state.screens.smx_autoprompt_latched = true;
        }
        screens::smx_assign::set_pending_return(CurrentScreen::Menu);
        self.handle_navigation_action(CurrentScreen::SmxAssignPads);
    }

    /// While the StepManiaX options page is open, light the pads blue (P1) / red
    /// (P2), white when ambiguous, so the user can see the assignment, and so a
    /// live Swap is reflected on the pads immediately. Also holds the underglow
    /// strips on a test colour (red with Theme Underglow on, blue with it off)
    /// so the GRB wire-order switch can be judged by eye. Restores auto-lighting
    /// and the theme underglow on leaving the page, unless the assignment screen
    /// is taking the lights over. (Driven from the app loop so the lifecycle is
    /// in one place.)
    pub(super) fn drive_smx_options_lights(&mut self, dt: f32) {
        let active = smx_options_light_preview_active(
            self.state.screens.current_screen,
            config::get().smx_input,
            options::is_smx_config_view(&self.state.screens.options_state),
        );

        let cfg = config::get();
        let restore_underglow = self.state.screens.smx_options_light_preview.update(
            active,
            dt,
            deadsync_smx::player_indicator_colors(),
            cfg.smx_default_light_brightness,
            (cfg.smx_underglow_theme, cfg.smx_underglow_grb),
            self.state.screens.current_screen == CurrentScreen::SmxAssignPads,
        );
        // Put the strips back on the theme colour (no-op when underglow is off;
        // auto-lighting above restores the firmware default there).
        if restore_underglow {
            crate::smx_config::apply_smx_underglow();
        }
    }

    /// While a side's cursor is on the Player Options "Pad Light Brightness" row,
    /// drive that side's pad with a slow rainbow scaled by the live percent, so
    /// the user previews the brightness they're picking. Restores auto-lighting
    /// once no side is previewing (or on leaving the page). Sent every frame; the
    /// SDK coalesces light writes to the pad's refresh rate.
    pub(super) fn drive_smx_player_options_lights(&mut self, dt: f32) {
        let preview = smx_player_options_light_preview_allowed(
            self.state.screens.current_screen,
            config::get().smx_input,
        )
        .then(|| {
            self.state
                .screens
                .player_options_state
                .as_ref()
                .map(player_options::pad_light_brightness_preview)
        })
        .flatten()
        .filter(|p| p.iter().any(Option::is_some));

        self.state.screens.smx_po_light_preview.update(
            preview,
            dt,
            smx_light_preview_restore_auto(self.state.screens.current_screen),
        );
    }

    pub(super) fn apply_smx_managed_preset(&mut self) {
        use pad_config_sync::PadConfigSignature;

        // Skip entirely on the gameplay hot path. Pad config can't change mid-song
        // (the UI that touches it isn't reachable here, so no intents queue up), and
        // rewriting pad thresholds while a chart is playing would be disruptive — a
        // mid-song hot-plug just re-resolves on the first non-gameplay frame via the
        // signature compare. The marker mirror is for the song-select UI, which is
        // hidden during gameplay, so there's nothing to refresh either.
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }

        // Drain UI requests (manual recall/apply/save → Override; default edit /
        // overwrite / delete / style switch → Invalidate) into the controller.
        let intents = std::mem::take(&mut self.state.screens.select_music_state.pad_config_intents);
        for intent in intents {
            self.pad_config_sync.apply_intent(intent);
        }

        let cfg = config::get();
        // Only query the SMX manager when the managed-config feature is actually on.
        // With it off (or SMX input disabled) there is nothing to resolve or write, so
        // skip the per-pad `get_info` lock entirely and just clear the cached signature.
        // The marker mirror below still runs so a screen rebuild can't lose stale markers.
        let managing = cfg.smx_input && cfg.smx_manages_pad_config;
        for pad in 0..2 {
            if !managing {
                self.pad_config_sync.signature[pad] = None;
                continue;
            }
            let info = deadsync_smx::get_info(pad);
            if !info.connected {
                self.pad_config_sync.signature[pad] = None;
                continue;
            }
            // In Doubles both pads belong to the one joined player; otherwise the
            // pad maps to its own side. Side is the slot (the SDK orders slot 0 =
            // P1, slot 1 = P2 per the pad→player assignment), not the raw jumper.
            let profile_id = profile::active_local_profile_id_for_pad(pad == 1);
            let pad_type = deadsync_smx::pad_sensor_type(pad).map(|t| t.as_str().to_owned());
            // Compare against the cached signature by borrow: the steady-state
            // path allocates nothing just to find that nothing changed. The owned
            // `Sig` is built (by moving these values) only when we re-resolve.
            if self.pad_config_sync.signature_matches(
                pad,
                cfg.smx_default_pad_config,
                &info.serial,
                profile_id.as_deref(),
                pad_type.as_deref(),
            ) {
                continue; // nothing relevant changed — no file I/O, no rewrite
            }
            let (applied, label) = resolve_smx_pad_config(
                pad,
                profile_id.as_deref(),
                pad_type.as_deref(),
                &info.serial,
                cfg.smx_default_pad_config,
            );
            // One line per actual (re)resolve — fires only past the signature
            // short-circuit above (connect, profile/style switch, preset change,
            // pad type becoming known), not every frame. The primary diagnostic for
            // "why did this pad get this config" on hardware we can't test here.
            log::debug!(
                "SMX: pad {pad} resolved {} '{}' (serial={}, fw={}, type={}, profile={:?}, applied={applied})",
                if label.preset { "preset" } else { "config" },
                label.name,
                info.serial,
                info.firmware_version,
                pad_type.as_deref().unwrap_or("unknown"),
                profile_id.as_deref(),
            );
            // Record what deadsync resolved so the UI can flag the active
            // preset/config. NOT gated on the write ACK: the resolution is what we
            // intend for the pad; gating it on a momentarily-unavailable config
            // (right after connect) would leave the marker blank. The write itself
            // retries until it lands (signature only saved on success).
            self.pad_config_sync.applied[pad] = Some(label);
            if applied {
                // Move (don't clone) the resolved inputs into the cached signature.
                self.pad_config_sync.signature[pad] = Some(PadConfigSignature {
                    preset: cfg.smx_default_pad_config,
                    serial: info.serial,
                    profile_id,
                    pad_type,
                });
            }
        }

        // Mirror the authoritative markers to the screen for display. Checked every
        // frame so a screen rebuild (which resets the mirror to None) can't lose them,
        // but only cloned when they actually differ — the equality check is a couple of
        // small string compares, whereas the clone heap-allocates the config name(s)
        // every frame an SMX pad is connected. Steady state: compare, no allocation.
        if self.state.screens.select_music_state.smx_applied != self.pad_config_sync.applied {
            self.state.screens.select_music_state.smx_applied = self.pad_config_sync.snapshot();
        }
    }

    /// Resolve each pad slot's user brightness from the player on that side and push
    /// it to the SMX crate, which scales every outgoing light frame by it. Cached so
    /// the push only fires on change. Skipped on the gameplay hot path: brightness is
    /// a per-player profile value that can't change mid-song, so the value resolved on
    /// the last non-gameplay frame stays valid and the profile lock stays off the
    /// gameplay loop. With SMX input off there are no light sends, so hold at full.
    pub(super) fn drive_smx_light_brightness(&mut self) {
        let screen = self.state.screens.current_screen;
        if matches!(screen, CurrentScreen::Gameplay | CurrentScreen::Practice) {
            return;
        }
        let smx_input = config::get().smx_input;
        let profile_resolved = if smx_input {
            profile::pad_light_brightness()
        } else {
            [100, 100]
        };
        let Some(plan) = smx_light_brightness_plan(
            screen,
            smx_input,
            self.smx_light_brightness,
            profile_resolved,
        ) else {
            return;
        };
        if plan.apply {
            self.smx_light_brightness = plan.resolved;
            deadsync_smx::set_light_brightness(plan.resolved);
        }
    }
}
