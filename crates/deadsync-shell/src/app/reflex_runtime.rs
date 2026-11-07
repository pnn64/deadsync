use super::*;
use deadsync_lights::{ButtonLight, Player};
use deadsync_reflex::LightFrame;

impl App {
    pub(super) fn sync_reflex_mappings(&mut self) {
        // Recompiling a keymap clears debounce state. Defer first-use bindings
        // until gameplay ends so hotplug cannot discard another device's holds.
        // Reconnecting an already mapped pad needs no keymap changes.
        if matches!(
            self.state.screens.current_screen,
            CurrentScreen::Gameplay | CurrentScreen::Practice
        ) {
            return;
        }
        for player in 0..2 {
            if deadsync_reflex::needs_default_mapping(player) {
                let mut keymap = deadsync_input::get_keymap();
                if deadsync_reflex::install_default_bindings(player, &mut keymap) {
                    self.input.set_keymap(&keymap);
                    deadsync_input::set_keymap(keymap);
                    config::runtime::save_without_keymaps();
                }
                deadsync_reflex::mark_mapping_initialized(player);
            }
        }
    }

    pub(super) fn sync_reflex_lights(&self, side: profile_data::PlayerSide) {
        if !deadsync_reflex::connected() {
            return;
        }
        let mut frames = [LightFrame::default(); 2];
        if self.state.screens.current_screen == CurrentScreen::TestLights {
            let state = self.lights.state_snapshot();
            for (p, player) in [Player::P1, Player::P2].into_iter().enumerate() {
                frames[p].testing = true;
                for (b, button) in [
                    ButtonLight::Left,
                    ButtonLight::Down,
                    ButtonLight::Up,
                    ButtonLight::Right,
                ]
                .into_iter()
                .enumerate()
                {
                    frames[p].chart[b] = state.button(player, button);
                }
            }
        } else {
            let gs = match self.state.screens.current_screen {
                CurrentScreen::Gameplay => self.state.screens.gameplay_state.as_ref(),
                CurrentScreen::Practice => self
                    .state
                    .screens
                    .practice_state
                    .as_ref()
                    .map(|p| &p.gameplay),
                _ => None,
            };
            if let Some(gs) = gs {
                let chart = self.lights.chart_directions();
                let single_p2 = gs.num_players() == 1
                    && gs.cols_per_player() == 4
                    && side == profile_data::PlayerSide::P2;
                if single_p2 {
                    frames[1].chart = chart[0];
                } else {
                    for i in 0..2 {
                        frames[i].chart = chart[i];
                    }
                }
                let now = gs.total_elapsed_in_screen();
                for col in 0..gs.cols_per_player() * gs.num_players() {
                    let Some((player, panel)) =
                        panel_for_column(gs.cols_per_player(), gs.num_players(), col, single_p2)
                    else {
                        continue;
                    };
                    if let Some(judgment) = gs.last_tap_judgment(col) {
                        let color = judgment_color(
                            judgment.grade,
                            judgment.blue_fantastic,
                            now - judgment.at_screen_s,
                        );
                        for (out, value) in frames[player].judgment[panel].iter_mut().zip(color) {
                            *out = (*out).max(value);
                        }
                    }
                }
            }
        }
        deadsync_reflex::set_lights(frames);
    }
}

fn panel_for_column(
    cpp: usize,
    players: usize,
    column: usize,
    single_p2: bool,
) -> Option<(usize, usize)> {
    if !matches!(cpp, 4 | 8) || column >= cpp * players {
        return None;
    }
    let local = column % cpp;
    let player = if cpp == 8 {
        local / 4
    } else if single_p2 {
        1
    } else {
        column / cpp
    };
    (player < 2).then_some((player, local % 4))
}
fn judgment_color(grade: deadsync_rules::judgment::JudgeGrade, blue: bool, age: f32) -> [u8; 3] {
    if !(0.0..0.25).contains(&age) {
        return [0; 3];
    }
    let index = deadsync_rules::judgment::judge_grade_ix(grade);
    let color = if index == 0 && !blue {
        [1.0; 4]
    } else {
        deadsync_theme_simply_love::color::JUDGMENT_RGBA[index]
    };
    let fade = 1.0 - age / 0.25;
    std::array::from_fn(|i| (color[i] * fade * 255.0).round() as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_layout_covers_p2_single_versus_double_and_routine() {
        assert_eq!(panel_for_column(4, 1, 0, true), Some((1, 0)));
        assert_eq!(panel_for_column(4, 2, 7, false), Some((1, 3)));
        assert_eq!(panel_for_column(8, 1, 4, false), Some((1, 0)));
        assert_eq!(panel_for_column(8, 2, 12, false), Some((1, 0)));
        assert_eq!(panel_for_column(5, 1, 0, false), None);
    }
    #[test]
    fn judgment_fades_and_expired_or_future_events_are_black() {
        use deadsync_rules::judgment::JudgeGrade;
        assert_eq!(judgment_color(JudgeGrade::Fantastic, false, 0.0), [255; 3]);
        assert_eq!(
            judgment_color(JudgeGrade::Fantastic, false, 0.125),
            [128; 3]
        );
        assert_eq!(judgment_color(JudgeGrade::Fantastic, false, 0.25), [0; 3]);
        assert_eq!(judgment_color(JudgeGrade::Miss, false, -1.0), [0; 3]);
    }
}
