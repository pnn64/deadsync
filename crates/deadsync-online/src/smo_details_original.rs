// Frozen from 4ca2c55fba8006d83b9ea427b1922a4dc6448b31; test/benchmark oracle.
use super::{MEDIA_BASE, PackDetails, column};

pub(super) fn parse_page(body: &str) -> Result<Vec<(u64, PackDetails)>, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("details response: {error}"))?;
    let rows = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "details response has no data array".to_owned())?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(cells) = row.as_array() else {
            continue;
        };
        if cells.len() < column::COUNT {
            continue;
        }
        let cell = |index: usize| cells.get(index).and_then(serde_json::Value::as_str);
        let Some(name_cell) = cell(column::NAME) else {
            continue;
        };
        let Some(id) = pack_id(name_cell) else {
            continue;
        };
        out.push((
            id,
            PackDetails {
                banner_url: cell(column::BANNER).and_then(banner_url),
                date_added: cell(column::DATE).and_then(inner_text).filter(is_iso_date),
                chart_types: cell(column::TYPES).map(chart_types).unwrap_or_default(),
            },
        ));
    }
    Ok(out)
}

pub(super) fn pack_id(cell: &str) -> Option<u64> {
    let start = cell.find("/pack/")? + "/pack/".len();
    let rest = &cell[start..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

pub(super) fn banner_url(cell: &str) -> Option<String> {
    let path = attribute(cell, "data-src")?;
    // The site's stand-in for a pack with no banner is an image that says
    // "NO BANNER". It is not the pack's art, so it is no art -- as the
    // original reads it.
    if path.is_empty() || path.contains("nobanner") {
        return None;
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return Some(path.to_owned());
    }
    Some(format!("{MEDIA_BASE}{path}"))
}

pub(super) fn chart_types(cell: &str) -> Vec<String> {
    let Some(raw) = attribute(cell, "data-sort") else {
        return Vec::new();
    };
    let decoded = decode_entities(raw);
    decoded
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|part| part.trim().trim_matches('\'').trim_matches('"').to_owned())
        .filter(|part| !part.is_empty())
        .collect()
}

pub(super) fn attribute<'a>(cell: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = cell.find(needle.as_str())? + needle.len();
    let rest = &cell[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

pub(super) fn inner_text(cell: &str) -> Option<String> {
    let start = cell.find('>')? + 1;
    let rest = &cell[start..];
    let end = rest.find('<')?;
    let text = rest[..end].trim();
    (!text.is_empty()).then(|| text.to_owned())
}

pub(super) fn is_iso_date(value: &String) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

pub(super) fn decode_entities(raw: &str) -> String {
    raw.replace("&#x27;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}
