// Frozen from 1c41dfa1e9cadf9e14fe64051c9c9cfa387a6e3e: SongRow::subline.
pub fn subline(row: &SongRow) -> String {
    let bpm = if row.bpm.is_empty() {
        String::new()
    } else {
        format!("{} bpm", row.bpm)
    };
    let mut bits: Vec<&str> = Vec::with_capacity(4);
    if !row.artist.is_empty() {
        bits.push(row.artist.as_str());
    }
    if !bpm.is_empty() {
        bits.push(bpm.as_str());
    }
    if !row.length.is_empty() {
        bits.push(row.length.as_str());
    }
    if !row.credit.is_empty() {
        bits.push(row.credit.as_str());
    }
    bits.join("  -  ")
}
