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
pub(super) struct Thumb {
    pub name: Arc<str>,
    pub part: usize,
}

/// Screen-owned choice catalog. Providers supply IDs and labels; all components
/// use the same runtime, texture loader, and preview renderer.
pub(super) struct PackMenu {
    skins: Vec<Skin>,
    choices: [Vec<Option<NoteSkin>>; 11],
    parts: [[Option<Thumb>; 11]; PLAYER_SLOTS],
}

impl PackMenu {
    pub fn new(packs: &[InstalledPack]) -> Self {
        Self {
            skins: packs
                .iter()
                .flat_map(|pack| pack.manifest.skins.iter().cloned())
                .collect(),
            choices: std::array::from_fn(|_| Vec::new()),
            parts: std::array::from_fn(|_| std::array::from_fn(|_| None)),
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
                    if self.skins.iter().any(|skin| skin.id == *name) {
                        continue;
                    }
                    choices.push(Some(NoteSkin::new(name)));
                    labels.push(format!("{} / {name}", tr("PlayerOptions", "SkinBundled")));
                }
                for skin in self.skins.iter().filter(|skin| names.contains(&skin.id)) {
                    let family = family_label(&skin.id);
                    for choice in skin
                        .options
                        .iter()
                        .filter(|choice| choice.slot == SLOTS[slot])
                    {
                        let value = if choice.id == "base" {
                            skin.id.clone()
                        } else {
                            format!("{}?{}={}", skin.id, SLOTS[slot], choice.id)
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

    pub fn preview(&self, player: usize, row: RowId) -> Option<&Thumb> {
        self.parts[player][slot_for_row(row)?].as_ref()
    }

    pub fn update_previews(
        &self,
        players: &[PlayerOptionsData; PLAYER_SLOTS],
        active: [bool; PLAYER_SLOTS],
        focused: [Option<RowId>; PLAYER_SLOTS],
        search: &search::SettingSearchState,
        noteskin: &mut NoteskinState,
        cols: usize,
        assets: &mut AssetManager,
    ) {
        let mut wanted = smallvec::SmallVec::<[(&str, usize); 40]>::new();
        for player in 0..PLAYER_SLOTS {
            if active[player]
                && let Some(thumb) = focused[player].and_then(|row| self.preview(player, row))
            {
                wanted.push((thumb.name.as_ref(), thumb.part));
            }
        }
        if let search::SettingSearchState::Open(open) = search {
            let range = search::visible_range(open);
            // The highlighted result loads before the rest of its page.
            for offset in 0..range.len() {
                let index = range.start
                    + (open.selected_index.saturating_sub(range.start) + offset) % range.len();
                if let Some(thumb) = &open.matches[index].thumb {
                    wanted.push((thumb.name.as_ref(), thumb.part));
                }
            }
        }
        for (player, options) in players.iter().enumerate() {
            if active[player] {
                wanted.push((options.noteskin.as_str(), 0));
                for thumb in self.parts[player].iter().flatten() {
                    let entry = (thumb.name.as_ref(), thumb.part);
                    if !wanted.contains(&entry) {
                        wanted.push(entry);
                    }
                }
            }
        }
        // Preload adjacent choices while the current arrow is on screen.
        for player in 0..PLAYER_SLOTS {
            if !active[player] {
                continue;
            }
            let Some(thumb) = focused[player].and_then(|row| self.preview(player, row)) else {
                continue;
            };
            let choices = &self.choices[thumb.part];
            let index = choices
                .iter()
                .position(|choice| {
                    choice
                        .as_ref()
                        .is_some_and(|choice| choice.as_str() == thumb.name.as_ref())
                })
                .unwrap_or(0);
            for delta in [-1isize, 1, -2, 2] {
                let index =
                    (index as isize + delta).rem_euclid(choices.len().max(1) as isize) as usize;
                if let Some(Some(choice)) = choices.get(index)
                    && !choice.is_none_choice()
                    && !wanted.contains(&(choice.as_str(), thumb.part))
                {
                    wanted.push((choice.as_str(), thumb.part));
                }
            }
        }
        noteskin
            .components
            .update(&wanted, &noteskin.cache, cols, assets);
    }

    pub fn choice_thumb(
        &self,
        state: &State,
        player: usize,
        row: RowId,
        index: usize,
    ) -> Option<Thumb> {
        let part = slot_for_row(row)?;
        let choice = self.choices[part]
            .get(index)?
            .as_ref()
            .unwrap_or(&state.player_options[player].noteskin);
        (!choice.is_none_choice()).then(|| Thumb {
            name: Arc::from(choice.as_str()),
            part,
        })
    }
}

fn stored_label(skins: &[Skin], raw: &str) -> Option<String> {
    let selection = Selection::parse(raw).ok()?;
    let skin = skins.iter().find(|skin| skin.id == selection.skin)?;
    let mut label = family_label(&selection.skin);
    for (slot, id) in &selection.options {
        let choice = skin
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
    for slot in 0..SLOTS.len() {
        let preview_slot = if slot == 9 { 8 } else { slot };
        let choice = selected[preview_slot].unwrap_or(&options.noteskin);
        state.pack_menu.parts[player][slot] =
            (slot != 9 && !choice.is_none_choice()).then(|| Thumb {
                name: Arc::from(choice.as_str()),
                part: preview_slot,
            });
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
    }
    sync_player(state, player);
    Outcome::persisted_with_visibility()
}
