// Frozen from e314fc18d (0.5.1709).
use super::*;

pub(super) fn pack_search_index(
    id: u64,
    name: &str,
    metadata: [Option<&str>; 4],
) -> (usize, String) {
    let capacity = name.len()
        + metadata
            .iter()
            .flatten()
            .map(|value| value.len() + 1)
            .sum::<usize>()
        + 21;
    let mut search_text = String::with_capacity(capacity);
    search_text.extend(name.chars().flat_map(char::to_lowercase));
    let normalized_name_len = search_text.len();
    for value in metadata.into_iter().flatten() {
        search_text.push(' ');
        search_text.extend(value.chars().flat_map(char::to_lowercase));
    }
    search_text.push(' ');
    write!(&mut search_text, "{id}").expect("writing to a String cannot fail");
    (normalized_name_len, search_text)
}

pub(super) fn parse_catalog_line(
    line: &str,
    line_number: usize,
) -> Result<PackInfo, StepManiaOnlineError> {
    // The upstream endpoint wraps names in quotes but does not CSV-escape
    // embedded quotes. Its six trailing columns have fixed, comma-free value
    // domains, so parsing from the right preserves names containing commas.
    let mut fields = [""; 7];
    let mut split = line.rsplitn(fields.len(), ", ");
    for field in &mut fields {
        *field = split
            .next()
            .ok_or_else(|| catalog_line_error(line_number, "expected eight columns"))?;
    }
    let (id_text, quoted_name) = fields[6]
        .split_once(", ")
        .ok_or_else(|| catalog_line_error(line_number, "missing pack name"))?;
    let name = quoted_name
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| catalog_line_error(line_number, "pack name is not quoted"))?;
    if name.trim().is_empty() {
        return Err(catalog_line_error(line_number, "pack name is empty"));
    }
    let id = parse_catalog_number(id_text, line_number, "ID")?;
    let song_count = parse_catalog_number(fields[5], line_number, "song count")?;
    let size_bytes = parse_catalog_number(fields[4], line_number, "size")?;
    Ok(new_pack(
        id,
        name.to_string(),
        song_count,
        size_bytes,
        optional_catalog_value(fields[3]),
        optional_catalog_value(fields[2]),
        optional_catalog_value(fields[1]),
        optional_catalog_value(fields[0]),
    ))
}

#[allow(clippy::too_many_arguments)]
fn new_pack(
    id: u64,
    name: String,
    song_count: u32,
    size_bytes: u64,
    sync: Option<String>,
    pack_type: Option<String>,
    substyle: Option<String>,
    min_version: Option<String>,
) -> PackInfo {
    let (normalized_name_len, search_text) = pack_search_index(
        id,
        name.as_str(),
        [
            sync.as_deref(),
            pack_type.as_deref(),
            substyle.as_deref(),
            min_version.as_deref(),
        ],
    );
    PackInfo {
        id,
        name,
        song_count,
        size_bytes,
        sync,
        pack_type,
        substyle,
        min_version,
        normalized_name_len,
        search_text,
    }
}
