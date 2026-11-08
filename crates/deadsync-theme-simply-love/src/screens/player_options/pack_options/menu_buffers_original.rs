impl PackMenu {
    pub fn original_add_rows(&mut self, rows: &mut RowMap, players: &[PlayerOptionsData]) {
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
}
