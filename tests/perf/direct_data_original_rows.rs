// Frozen from def9a12f131b4dcbc0cc3ed4c3f9fd2f61f54ec1; function names and row-module paths changed.
fn original_init_opted_in_bitmask_rows(
    row_map: &mut RowMap,
    profile: &deadsync_profile::PlayerOptionsData,
    masks: &mut PlayerOptionMasks,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get(id) else {
            continue;
        };
        let RowBehavior::Bitmask(binding) = row.behavior else {
            continue;
        };
        if binding.init().is_none() {
            continue;
        }
        let row = row_map.get_mut(id).expect("row was just observed");
        crate::screens::player_options::row::init_bitmask_row_from_binding(
            row, &binding, profile, masks, player_idx,
        );
    }
}

fn original_init_opted_in_cycle_rows(
    row_map: &mut RowMap,
    profile: &deadsync_profile::PlayerOptionsData,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get_mut(id) else {
            continue;
        };
        match row.behavior {
            RowBehavior::Cycle(crate::screens::player_options::row::CycleBinding::Index(
                binding,
            )) => {
                crate::screens::player_options::row::init_cycle_row_from_binding(
                    row, &binding, profile, player_idx,
                );
            }
            RowBehavior::Cycle(crate::screens::player_options::row::CycleBinding::Bool(
                binding,
            )) => {
                crate::screens::player_options::row::init_cycle_row_from_binding(
                    row, &binding, profile, player_idx,
                );
            }
            _ => {}
        }
    }
}

fn original_init_opted_in_numeric_rows(
    row_map: &mut RowMap,
    profile: &deadsync_profile::PlayerOptionsData,
    player_idx: usize,
) {
    let ids: Vec<RowId> = row_map.display_order().to_vec();
    for id in ids {
        let Some(row) = row_map.get(id) else {
            continue;
        };
        let RowBehavior::Numeric(binding) = row.behavior else {
            continue;
        };
        if binding.init.is_none() {
            continue;
        }
        let row = row_map.get_mut(id).expect("row was just observed");
        crate::screens::player_options::row::init_numeric_row_from_binding(
            row, &binding, profile, player_idx,
        );
    }
}
