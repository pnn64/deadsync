use super::*;

pub(super) fn begin_purchase(
    runtime: &mut RuntimeState,
) -> Option<(u64, ShopSession, SrpgShopSnapshot)> {
    let Some(session) = runtime.session.clone() else {
        return None;
    };
    if runtime.snapshot.phase != SrpgShopPhase::Ready {
        return None;
    }
    runtime.generation = runtime.generation.wrapping_add(1);
    let generation = runtime.generation;
    let mut snapshot = (*runtime.snapshot).clone();
    snapshot.phase = SrpgShopPhase::Purchasing;
    snapshot.message = Some("Confirming purchase with SRPG10...".to_string());
    let previous = snapshot.clone();
    set_runtime_snapshot(runtime, Arc::new(snapshot));
    Some((generation, session, previous))
}

pub(super) fn parse_catalog(
    body: &str,
    shop_id: u32,
    lifetime_balance: u64,
) -> Result<Vec<SrpgShopItem>, SrpgShopError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    let rows = match &value {
        Value::Object(map) => object_array(map, &["data", "aaData", "rows", "items"]),
        Value::Array(rows) => Some(rows),
        _ => None,
    }
    .ok_or_else(|| SrpgShopError::InvalidResponse("SRPG10 catalog has no rows".to_string()))?;
    let mut keyed_rows = Vec::with_capacity(rows.len());
    keyed_rows.extend(rows.iter().map(|row| (catalog_row_key(row), row)));
    keyed_rows.sort_by_key(|(key, _)| *key);
    let mut items = Vec::with_capacity(keyed_rows.len());
    items.extend(
        keyed_rows
            .into_iter()
            .filter_map(|(_, row)| catalog_item(row, shop_id, lifetime_balance)),
    );
    Ok(items)
}

fn catalog_item(row: &Value, shop_id: u32, lifetime_balance: u64) -> Option<SrpgShopItem> {
    let cells = row.as_array()?;
    let cell = |index: usize| value_text_ref(cells.get(index));
    let type_id = catalog_plain_number(cells.get(11))
        .and_then(|number| u8::try_from(number).ok())
        .unwrap_or(0);
    let kind = if type_id == 1 {
        SrpgShopItemKind::Song
    } else {
        SrpgShopItemKind::Relic
    };
    let cost = cells.get(7).and_then(catalog_number_with_commas);
    let censor = shop_id == 2 && cost.is_some_and(|cost| cost > lifetime_balance);
    Some(SrpgShopItem {
        item_id: cell(0).into_owned(),
        kind,
        name: if censor {
            "???".to_string()
        } else {
            clean_cell(cell(2).as_ref()).into_owned()
        },
        description: if censor {
            "Reach the required lifetime Jej total to reveal this song.".to_string()
        } else {
            clean_cell(cell(3).as_ref()).into_owned()
        },
        effect: if censor {
            "Difficulty: ???  •  Speed Tier: ???".to_string()
        } else {
            clean_cell(cell(4).as_ref()).replace('|', "  •  ")
        },
        cost,
        difficulty: (kind == SrpgShopItemKind::Song && !censor)
            .then(|| {
                catalog_plain_number(cells.get(12)).and_then(|number| u32::try_from(number).ok())
            })
            .flatten(),
        bpm: (kind == SrpgShopItemKind::Song && !censor)
            .then(|| {
                catalog_plain_number(cells.get(13)).and_then(|number| u32::try_from(number).ok())
            })
            .flatten(),
        type_id,
        owned: false,
        site_downloaded: false,
        downloaded: false,
        download_url: None,
    })
}

fn object_array<'a>(map: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a Vec<Value>> {
    keys.iter().find_map(|wanted| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .and_then(|(_, value)| value.as_array())
    })
}
