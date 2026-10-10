// Frozen from 735a994c75240ae043dc153204572d6952ed14fc; methods wrapped as functions on the same input types.
use super::*;

pub(super) fn low_meter(row: &SongRow) -> Option<u32> {
    let digits: String = row
        .meters
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

pub(super) fn is_beginner_friendly(page: &PackPage) -> bool {
    /// The easiest block a beginner chart sits in...
    const FLOOR: u32 = 1;
    /// ...and the hardest.
    const CEIL: u32 = 4;

    let mut easy = 0usize;
    let mut total = 0usize;
    for song in &page.songs {
        let Some(low) = low_meter(song) else {
            continue;
        };
        total += 1;
        if (FLOOR..=CEIL).contains(&low) {
            easy += 1;
        }
    }
    total > 0 && easy * 2 > total
}
