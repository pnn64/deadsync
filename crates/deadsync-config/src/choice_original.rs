// Frozen from 227c4b435 for strict grammar and allocation comparisons.
use super::{SelectMusicSeriesSource, SrpgShopFolder, TournamentScoringSystem};
pub fn series(value: &str) -> Result<SelectMusicSeriesSource, ()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "packini" | "pack.ini" | "metadata" => Ok(SelectMusicSeriesSource::PackIni),
        "folder" | "folders" | "directory" | "filesystem" => Ok(SelectMusicSeriesSource::Folder),
        _ => Err(()),
    }
}
pub fn shop(value: &str) -> Result<SrpgShopFolder, ()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "unlocks" | "srpg10 unlocks" => Ok(SrpgShopFolder::Unlocks),
        "shops" | "srpg10 - shops" => Ok(SrpgShopFolder::Shops),
        "faction" | "factions" | "by faction" => Ok(SrpgShopFolder::Faction),
        _ => Err(()),
    }
}
pub fn scoring(value: &str) -> Result<TournamentScoringSystem, ()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ex" => Ok(TournamentScoringSystem::Ex),
        "itg" => Ok(TournamentScoringSystem::Itg),
        _ => Err(()),
    }
}
