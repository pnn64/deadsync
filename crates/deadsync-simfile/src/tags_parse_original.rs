pub fn latest_simfile_tag_values<const N: usize>(
    simfile_data: &[u8],
    tags: [&[u8]; N],
) -> [String; N] {
    let mut latest = [None; N];
    let mut i = 0usize;
    while i < simfile_data.len() {
        let Some(pos) = find_byte(&simfile_data[i..], b'#') else {
            break;
        };
        i += pos;
        let slice = &simfile_data[i..];
        let Some((tag_index, tag)) = tags
            .iter()
            .copied()
            .enumerate()
            .find(|(_, tag)| starts_with_ci(slice, tag))
        else {
            i += 1;
            continue;
        };
        if let Some((value, adv)) = parse_tag_val(slice, tag.len(), true) {
            latest[tag_index] = Some(value);
            i += adv;
        } else {
            i += 1;
        }
    }

    std::array::from_fn(|index| {
        latest[index]
            .map(|raw| unescape_tag(decode_bytes(raw).as_ref()).into_owned())
            .unwrap_or_default()
    })
}
