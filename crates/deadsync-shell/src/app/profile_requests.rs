use super::{App, CurrentScreen, music_session_view, prepare_music_profile_selection};
use deadsync_config as config;
use deadsync_profile::compat as profile;
use deadsync_profile::requests::{ProfileRequest, ProfileSelection, select_profiles};
use deadsync_profile::{PlayerSide, player_side_index};
use deadsync_theme_simply_love::screens::{
    manage_local_profiles, profile_selection_target, select_music,
};

impl App {
    pub(super) fn handle_profile_request(&mut self, request: ProfileRequest) {
        match request {
            ProfileRequest::Select {
                p1,
                p2,
                p1_joined,
                p2_joined,
                fast_switch,
            } => self.refresh_profile_selection(
                select_profiles(p1, p2, p1_joined, p2_joined),
                fast_switch,
            ),
            ProfileRequest::SetPlayStyle(play_style) => profile::set_session_play_style(play_style),
            ProfileRequest::SetSession {
                play_style,
                player_side,
                joined,
            } => {
                profile::set_session_player_side(player_side);
                profile::set_session_joined(joined[0], joined[1]);
                profile::set_session_play_style(play_style);
            }
            ProfileRequest::SetMusicRate(rate) => profile::set_session_music_rate(rate),
            ProfileRequest::SetPlayMode(play_mode) => profile::set_session_play_mode(play_mode),
            ProfileRequest::SetMachineDefaultScrollSpeed(setting) => {
                profile::update_machine_default_scroll_speed(setting);
            }
            ProfileRequest::SetMachineDefaultScroll(setting) => {
                profile::update_machine_default_scroll(setting);
            }
            ProfileRequest::SetMachineDefaultBackgroundFilter(setting) => {
                profile::update_machine_default_background_filter(setting);
            }
            ProfileRequest::SetMachineDefaultNoteskin(noteskin) => {
                profile::update_machine_default_noteskin(noteskin);
            }
            ProfileRequest::UpdatePlayerOptions {
                side,
                options,
                judgment_palette_id,
                heart_rate_device_id,
                max_heart_rate,
            } => profile::update_player_options_for_side(
                side,
                *options,
                judgment_palette_id,
                heart_rate_device_id,
                max_heart_rate,
            ),
            ProfileRequest::UpdateInitials(updates) => {
                for (side, initials) in [PlayerSide::P1, PlayerSide::P2].into_iter().zip(updates) {
                    if let Some(initials) = initials {
                        profile::update_player_initials_for_side(side, &initials);
                    }
                }
            }
            ProfileRequest::ToggleFavorite { side, chart_hash } => {
                profile::toggle_favorite(side, &chart_hash);
            }
            ProfileRequest::ToggleEvaluationFavorite { side } => {
                if self.state.screens.current_screen == CurrentScreen::Evaluation
                    && let Some(stage) = self.state.screens.evaluation_stage.as_ref()
                    && let Some((_, chart_hash)) = stage.player_chart_queries().into_iter().flatten()
                        .find(|(played_side, _)| *played_side == side)
                {
                    profile::toggle_favorite(side, chart_hash);
                }
            }
            ProfileRequest::TogglePackFavorite { side, pack_name } => {
                profile::toggle_pack_favorite(side, &pack_name);
            }
            ProfileRequest::ToggleSeriesFavorite { side, series_name } => {
                profile::toggle_series_favorite(side, &series_name);
            }
            ProfileRequest::MarkPacksKnown {
                profile_ids,
                pack_names,
            } => profile::mark_packs_known(&profile_ids, pack_names.iter().map(String::as_str)),
            ProfileRequest::CreateLocalProfile { display_name } => {
                let event = crate::local_profiles::create(&display_name);
                manage_local_profiles::apply_local_profile_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    event,
                );
            }
            ProfileRequest::RenameLocalProfile {
                profile_id,
                display_name,
            } => {
                let event = crate::local_profiles::rename(&profile_id, &display_name);
                manage_local_profiles::apply_local_profile_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    event,
                );
            }
            ProfileRequest::SetDefaultLocalProfile { side, profile_id } => {
                let event = crate::local_profiles::set_default(side, profile_id);
                manage_local_profiles::apply_local_profile_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    event,
                );
            }
            ProfileRequest::DeleteLocalProfile { profile_id } => {
                let event = crate::local_profiles::delete(&profile_id);
                manage_local_profiles::apply_local_profile_event(
                    &mut self.state.screens.manage_local_profiles_state,
                    event,
                );
            }
            ProfileRequest::DiscoverItgProfiles => self.profile_import.discover(),
            ProfileRequest::BrowseItgProfiles { title } => self.profile_import.browse(title),
            ProfileRequest::StartItgProfileImport { dir } => self.profile_import.start(dir),
            ProfileRequest::CancelItgProfileImport => self.profile_import.cancel(),
        }
    }

    fn refresh_profile_selection(&mut self, selection: ProfileSelection, fast_switch: bool) {
        let (show_groovestats_login, show_arrowcloud_login) = if fast_switch {
            (false, false)
        } else {
            let cfg = config::runtime::get();
            (
                crate::qr_login::should_auto_show_groovestats(
                    cfg.enable_groovestats,
                    cfg.groovestats_qr_login_when,
                ),
                crate::qr_login::should_auto_show_arrowcloud(
                    cfg.enable_arrowcloud,
                    cfg.arrowcloud_qr_login_when,
                ),
            )
        };
        self.state.session.combo_carry = selection.combo_carry;
        let current_screen = self.state.screens.current_screen;
        if let Some(backend) = self.backend.as_mut() {
            for (side, avatar_path) in [PlayerSide::P1, PlayerSide::P2]
                .into_iter()
                .zip(selection.avatar_paths)
            {
                self.dynamic_media.set_profile_avatar_for_side(
                    &mut self.asset_manager,
                    backend,
                    side,
                    avatar_path,
                );
            }
        }

        if fast_switch && current_screen == CurrentScreen::SelectMusic {
            let preference_updates = prepare_music_profile_selection(
                &mut self.state.screens.select_music_state,
                music_session_view(selection.previous_session, selection.previous_guests),
                music_session_view(profile::get_session_snapshot(), selection.selected_guests),
                selection.changed_profiles,
                selection.preferred_difficulties,
            );
            self.sync_select_music_runtime_view(self.select_music_policy);
            let current_color_index = self.state.screens.select_profile_state.active_color_index;
            let sm = &mut self.state.screens.select_music_state;
            sm.active_color_index = current_color_index;
            select_music::apply_profile_preferences(sm, preference_updates);
            self.state.session.preferred_difficulty_index = sm.preferred_difficulty_index;
        } else {
            self.state.session.preferred_difficulty_index =
                selection.preferred_difficulties[player_side_index(selection.active_side)];
        }
        if let Some(target) = profile_selection_target(
            current_screen,
            fast_switch,
            show_groovestats_login,
            show_arrowcloud_login,
        ) {
            // ProfileLoad prepares SelectMusic and SelectCourse asynchronously.
            self.handle_navigation_action(target);
        }
    }
}
