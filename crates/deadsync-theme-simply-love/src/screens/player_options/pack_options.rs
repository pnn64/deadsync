//! Independent noteskin components and previews for Player Options.

use super::*;
use deadsync_noteskin::pack::{InstalledPack, SLOTS, Selection, Skin};
use profile_data::{NoteSkin, PlayerOptionsData};

const ROWS: [(RowId, &str); 11] = [
    (RowId::SkinArrows, "SkinArrows"),
    (RowId::ReceptorSkin, "SkinReceptors"),
    (RowId::SkinHoldActive, "SkinHoldActive"),
    (RowId::SkinHoldInactive, "SkinHoldInactive"),
    (RowId::SkinRollActive, "SkinRollActive"),
    (RowId::SkinRollInactive, "SkinRollInactive"),
    (RowId::TapExplosionSkin, "SkinTapExplosions"),
    (RowId::SkinHoldExplosions, "SkinHoldExplosions"),
    (RowId::MineSkin, "SkinMines"),
    (RowId::SkinMineSize, "SkinMineSize"),
    (RowId::SkinLifts, "SkinLifts"),
];

pub(super) fn slot_for_row(id: RowId) -> Option<usize> {
    ROWS.iter().position(|&(row, _)| row == id)
}

fn part_mut(options: &mut PlayerOptionsData, slot: usize) -> &mut Option<NoteSkin> {
    match slot {
        0 => &mut options.arrow_noteskin,
        1 => &mut options.receptor_noteskin,
        2 => &mut options.hold_active_noteskin,
        3 => &mut options.hold_inactive_noteskin,
        4 => &mut options.roll_active_noteskin,
        5 => &mut options.roll_inactive_noteskin,
        6 => &mut options.tap_explosion_noteskin,
        7 => &mut options.hold_explosion_noteskin,
        8 => &mut options.mine_noteskin,
        10 => &mut options.lift_noteskin,
        _ => panic!("noteskin slot {slot} has no component selection"),
    }
}

fn parts(options: &PlayerOptionsData) -> [Option<&NoteSkin>; 11] {
    [
        options.arrow_noteskin.as_ref(),
        options.receptor_noteskin.as_ref(),
        options.hold_active_noteskin.as_ref(),
        options.hold_inactive_noteskin.as_ref(),
        options.roll_active_noteskin.as_ref(),
        options.roll_inactive_noteskin.as_ref(),
        options.tap_explosion_noteskin.as_ref(),
        options.hold_explosion_noteskin.as_ref(),
        options.mine_noteskin.as_ref(),
        None,
        options.lift_noteskin.as_ref(),
    ]
}

#[derive(Clone, Debug)]
pub(super) enum Thumb {
    Sprite {
        key: Arc<str>,
        uv: [f32; 4],
    },
    Note {
        skin: Arc<Noteskin>,
        part: NoteAnimPart,
    },
}

struct MenuSkin {
    skin: Skin,
    atlas: Arc<str>,
}

/// Screen-owned catalog, built once at entry from bundled skins and at most 16
/// pack skins. Input selects existing IDs and thumbnail handles; no image decode,
/// compilation or catalog walks occur during rendering. The catalog and its fixed
/// player preview slots drop on screen exit. A bounded worker cache loads live
/// mine previews; full gameplay components warm at song load.
pub(super) struct PackMenu {
    pub mines: MinePreviews,
    skins: Vec<MenuSkin>,
    choices: [Vec<Option<NoteSkin>>; 11],
    parts: [[Option<Thumb>; 11]; PLAYER_SLOTS],
    base: [Option<Thumb>; PLAYER_SLOTS],
}

impl PackMenu {
    pub fn new(packs: &[InstalledPack]) -> Self {
        let skins = packs
            .iter()
            .flat_map(|pack| {
                pack.manifest.skins.iter().map(|skin| MenuSkin {
                    atlas: deadsync_assets::textures::canonical_texture_key(
                        pack.root.join(&skin.preview),
                    )
                    .into(),
                    skin: skin.clone(),
                })
            })
            .collect();
        Self {
            mines: MinePreviews::default(),
            skins,
            choices: std::array::from_fn(|_| Vec::new()),
            parts: std::array::from_fn(|_| std::array::from_fn(|_| None)),
            base: std::array::from_fn(|_| None),
        }
    }

    pub fn add_rows(&mut self, rows: &mut RowMap, players: &[PlayerOptionsData]) {
        let Some(parent) = rows
            .display_order
            .iter()
            .position(|id| *id == RowId::NoteSkin)
        else {
            return;
        };
        let names: Vec<_> = rows
            .row(RowId::NoteSkin)
            .choices
            .iter()
            .map(|name| name.as_str().to_string())
            .collect();
        rows.display_order.retain(|id| slot_for_row(*id).is_none());
        for (slot, &(id, title)) in ROWS.iter().enumerate() {
            let mut labels = Vec::new();
            let choices = &mut self.choices[slot];
            choices.clear();
            if slot == 9 {
                labels.extend((10..=200).map(|size| format!("{size}%")));
            } else {
                choices.push(None);
                labels.push(tr("PlayerOptions", "MatchNoteSkinLabel").to_string());
                if slot == 6 {
                    choices.push(Some(NoteSkin::none_choice()));
                    labels.push(tr("PlayerOptions", "NoTapExplosionLabel").to_string());
                }
                for name in &names {
                    if self.skins.iter().any(|skin| skin.skin.id == *name) {
                        continue;
                    }
                    choices.push(Some(NoteSkin::new(name)));
                    labels.push(format!("{} / {name}", tr("PlayerOptions", "SkinBundled")));
                }
                for skin in self
                    .skins
                    .iter()
                    .filter(|skin| names.contains(&skin.skin.id))
                {
                    let family = family_label(&skin.skin.id);
                    for choice in skin
                        .skin
                        .options
                        .iter()
                        .filter(|choice| choice.slot == SLOTS[slot])
                    {
                        let value = if choice.id == "base" {
                            skin.skin.id.clone()
                        } else {
                            format!("{}?{}={}", skin.skin.id, SLOTS[slot], choice.id)
                        };
                        choices.push(Some(NoteSkin::new(&value)));
                        let label = if choice.id == "base" {
                            tr("PlayerOptions", "SkinOriginal").to_string()
                        } else {
                            choice.label.clone()
                        };
                        labels.push(format!("{family} / {label}"));
                    }
                }
                // Keep missing providers/variants visible and saved so reinstalling
                // restores them; cycling away is an explicit user choice.
                for player in players {
                    if let Some(saved) = parts(player)[slot]
                        && !choices.iter().any(|choice| choice.as_ref() == Some(saved))
                    {
                        choices.push(Some(saved.clone()));
                        labels.push(stored_label(&self.skins, saved.as_str()).unwrap_or_else(
                            || {
                                format!(
                                    "{} / {}",
                                    tr("PlayerOptions", "SkinUnavailable"),
                                    saved.as_str()
                                )
                            },
                        ));
                    }
                }
            }
            let row = Row::custom(
                id,
                lookup_key("PlayerOptions", title),
                lookup_key(
                    "PlayerOptionsHelp",
                    if slot == 9 {
                        "SkinMineSizeHelp"
                    } else {
                        "SkinPartHelp"
                    },
                ),
                CustomBinding { apply: apply_part },
                labels,
            );
            if let Some(existing) = rows.get_mut(id) {
                *existing = row;
            } else {
                rows.insert(row);
            }
            rows.display_order.insert(parent + slot + 1, id);
        }
    }

    fn thumb(&self, raw: &str, slot: usize) -> Option<Thumb> {
        let selection = Selection::parse(raw).ok()?;
        let skin = self
            .skins
            .iter()
            .find(|skin| skin.skin.id == selection.skin)?;
        let id = selection
            .options
            .get(SLOTS[slot])
            .map_or("base", String::as_str);
        let choice = skin
            .skin
            .options
            .iter()
            .find(|choice| choice.slot == SLOTS[slot] && choice.id == id)?;
        let x = f32::from(choice.cell % 32) / 32.0;
        let y = f32::from(choice.cell / 32) / 32.0;
        Some(Thumb::Sprite {
            key: Arc::clone(&skin.atlas),
            uv: [x, y, x + 1.0 / 32.0, y + 1.0 / 32.0],
        })
    }

    pub fn preview(&self, player: usize, row: RowId) -> Option<&Thumb> {
        if row == RowId::NoteSkin {
            return self.base[player].as_ref();
        }
        self.parts[player][slot_for_row(row)?].as_ref()
    }

    pub fn update_mines(
        &mut self,
        players: &[PlayerOptionsData; PLAYER_SLOTS],
        active: [bool; PLAYER_SLOTS],
        search: &search::SettingSearchState,
        bundled: &HashMap<String, Arc<Noteskin>>,
        cols: usize,
    ) {
        let mut wanted =
            smallvec::SmallVec::<[&str; PLAYER_SLOTS + search::SEARCH_MAX_RESULTS]>::new();
        for (player, options) in players.iter().enumerate() {
            if active[player] {
                wanted.push(
                    options
                        .mine_noteskin
                        .as_ref()
                        .unwrap_or(&options.noteskin)
                        .as_str(),
                );
            }
        }
        if let search::SettingSearchState::Open(open) = search
            && open.component == Some(RowId::MineSkin)
        {
            for item in &open.matches[search::visible_range(open)] {
                if let Some(choice) = item.choice_index.and_then(|i| self.choices[8].get(i)) {
                    wanted.push(
                        choice
                            .as_ref()
                            .unwrap_or(&players[open.opener_player].noteskin)
                            .as_str(),
                    );
                }
            }
        }
        self.mines.update(&wanted, bundled, cols);
    }

    pub fn mine_choice(&self, index: usize) -> Option<&NoteSkin> {
        self.choices[8].get(index)?.as_ref()
    }

    pub fn choice_thumb(
        &self,
        state: &State,
        player: usize,
        row: RowId,
        index: usize,
    ) -> Option<Thumb> {
        let slot = slot_for_row(row)?;
        if slot == 8 {
            return None;
        }
        let choice = self.choices[slot]
            .get(index)?
            .as_ref()
            .unwrap_or(&state.player_options[player].noteskin);
        self.thumb(choice.as_str(), slot)
            .or_else(|| bundled_thumb(state.noteskin.cache.get(choice.as_str())?, slot))
    }
}

fn bundled_thumb(skin: &Arc<Noteskin>, slot: usize) -> Option<Thumb> {
    if matches!(slot, 0 | 10) {
        return Some(Thumb::Note {
            skin: Arc::clone(skin),
            part: if slot == 10 {
                NoteAnimPart::Lift
            } else {
                NoteAnimPart::Tap
            },
        });
    }
    let sprite = match slot {
        1 => skin.receptor_off.first(),
        2 => skin.hold.body_active.as_ref(),
        3 => skin.hold.body_inactive.as_ref(),
        4 => skin.roll.body_active.as_ref(),
        5 => skin.roll.body_inactive.as_ref(),
        6 => skin
            .tap_explosions
            .values()
            .next()
            .map(|explosion| &explosion.slot),
        7 => skin.hold.explosion.as_ref(),
        _ => None,
    }?;
    Some(Thumb::Sprite {
        key: sprite.texture_key_shared(),
        uv: sprite.uv_for_frame_at(0, 0.0),
    })
}

fn stored_label(skins: &[MenuSkin], raw: &str) -> Option<String> {
    let selection = Selection::parse(raw).ok()?;
    let skin = skins.iter().find(|skin| skin.skin.id == selection.skin)?;
    let mut label = family_label(&selection.skin);
    for (slot, id) in &selection.options {
        let choice = skin
            .skin
            .options
            .iter()
            .find(|choice| choice.slot == *slot && choice.id == *id)?;
        label.push_str(" / ");
        label.push_str(&choice.label);
    }
    Some(label)
}

fn family_label(id: &str) -> String {
    match id {
        "cel-workshop" | "metal-workshop" => "HURG".to_string(),
        _ => id.replace('-', " "),
    }
}

pub(super) fn sync_player(state: &mut State, player: usize) {
    let options = &state.player_options[player];
    let selected = parts(options);
    state.pack_menu.base[player] = state.pack_menu.thumb(options.noteskin.as_str(), 0);
    for slot in 0..SLOTS.len() {
        let preview_slot = if slot == 9 { 8 } else { slot };
        let raw = selected[preview_slot].unwrap_or(&options.noteskin).as_str();
        state.pack_menu.parts[player][slot] = if matches!(slot, 8 | 9) {
            None
        } else {
            state.pack_menu.thumb(raw, preview_slot)
        };
        // Existing bundled receptor/mine/explosion rows retain their live previews.
        if !matches!(slot, 1 | 6 | 8 | 9) && state.pack_menu.parts[player][slot].is_none() {
            state.pack_menu.parts[player][slot] = state
                .noteskin
                .cache
                .get(raw)
                .and_then(|skin| bundled_thumb(skin, preview_slot));
        }
        let index = if slot == 9 {
            (options.mine_size_percent.clamp(10, 200) - 10) as usize
        } else {
            state.pack_menu.choices[slot]
                .iter()
                .position(|choice| choice.as_ref() == selected[slot])
                .unwrap_or(0)
        };
        for pane in &mut state.panes {
            if let Some(row) = pane.row_map.get_mut(ROWS[slot].0) {
                row.selected_choice_index[player] = index;
            }
        }
    }
}

pub(super) fn apply_part(
    state: &mut State,
    player: usize,
    row: RowId,
    delta: isize,
    wrap: NavWrap,
) -> Outcome {
    let Some(slot) = slot_for_row(row) else {
        return Outcome::NONE;
    };
    let old = state.pane().row_map.row(row).selected_choice_index[player];
    let Some(index) = choice::cycle_choice_index(state, player, row, delta, wrap) else {
        return Outcome::NONE;
    };
    if old == index {
        return Outcome::NONE;
    }
    if slot == 9 {
        state.player_options[player].mine_size_percent = 10 + index as i32;
    } else {
        *part_mut(&mut state.player_options[player], slot) =
            state.pack_menu.choices[slot][index].clone();
        sync_noteskin_previews_for_player(
            &mut state.noteskin,
            &state.player_options[player],
            player,
            state.cols_per_player,
        );
    }
    sync_player(state, player);
    Outcome::persisted_with_visibility()
}
