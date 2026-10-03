// Frozen from cafbd9e2a (0.5.1707).
use super::*;

pub(super) fn clean_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    let mut pending_space = false;
    let mut rest = text;
    while !rest.is_empty() {
        // The legacy replacement order decodes these two nested forms because
        // `&amp;` is replaced before the later `&lt;`/`&gt;` passes.
        let (ch, consumed) = if rest.starts_with("&amp;lt;") {
            ('<', "&amp;lt;".len())
        } else if rest.starts_with("&amp;gt;") {
            ('>', "&amp;gt;".len())
        } else if rest.starts_with("&apos;") {
            ('\'', "&apos;".len())
        } else if rest.starts_with("&quot;") {
            ('"', "&quot;".len())
        } else if rest.starts_with("&amp;") {
            ('&', "&amp;".len())
        } else if rest.starts_with("&lt;") {
            ('<', "&lt;".len())
        } else if rest.starts_with("&gt;") {
            ('>', "&gt;".len())
        } else {
            let ch = rest.chars().next().expect("rest is non-empty");
            (ch, ch.len_utf8())
        };
        rest = &rest[consumed..];
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                pending_space = !out.is_empty();
            }
            _ if !in_tag && ch.is_whitespace() => pending_space = !out.is_empty(),
            _ if !in_tag => {
                if pending_space {
                    out.push(' ');
                    pending_space = false;
                }
                out.push(ch);
            }
            _ => {}
        }
    }
    out
}

pub(super) fn catalog_item(
    row: &Value,
    shop_id: u32,
    lifetime_balance: u64,
) -> Option<SrpgShopItem> {
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
            clean_cell(cell(2).as_ref())
        },
        description: if censor {
            "Reach the required lifetime Jej total to reveal this song.".to_string()
        } else {
            clean_cell(cell(3).as_ref())
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
