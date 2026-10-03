// Frozen from e314fc18d (0.5.1709).
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
            item_id: match row.id {
                Value::String(mut id) => {
                    id.shrink_to_fit();
                    id
                }
                value => value_text(&value),
            },
            name: clean_owned_cell(row.song),
            details: clean_owned_cell(row.data),
            url: absolutize_url(&row.url),
            site_downloaded: row.dled != 0,
        })
        .collect())
}

pub(super) fn absolutize_url(url: &str) -> String {
    let url = url.replace("\\/", "/");
    if url.starts_with("https://") || url.starts_with("http://") {
        url
    } else if url.starts_with('/') {
        format!("{BASE_ORIGIN}{url}")
    } else {
        format!("{BASE_ORIGIN}/{}", url.trim_start_matches("./"))
    }
}
