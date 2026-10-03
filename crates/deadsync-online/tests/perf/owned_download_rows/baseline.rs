// Frozen from 5d780220d (0.5.1708).
use super::*;

pub(super) fn parse_downloads(body: &str) -> Result<Vec<ParsedDownload>, SrpgShopError> {
    let response: DownloadResponse = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    if !response.errors.is_empty() {
        return Err(SrpgShopError::InvalidResponse(response.errors.join(" ")));
    }
    Ok(response
        .unlocks
        .into_iter()
        .filter(|row| row.url.contains(".zip"))
        .map(|row| ParsedDownload {
            item_id: value_text(&row.id),
            name: clean_cell(&row.song).into_owned(),
            details: clean_cell(&row.data).into_owned(),
            url: absolutize_url(&row.url),
            site_downloaded: row.dled != 0,
        })
        .collect())
}
