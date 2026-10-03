// Frozen from 5d780220d (0.5.1708).
use super::*;

pub(super) fn parse_purchase(body: &str) -> Result<PurchaseResult, SrpgShopError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| SrpgShopError::InvalidResponse(error.to_string()))?;
    let errors = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let download = value
        .get("unlocks")
        .and_then(Value::as_object)
        .and_then(download_from_object)
        .map(|download| PurchaseDownload {
            name: download.name,
        });
    Ok(PurchaseResult { errors, download })
}

pub(super) fn download_from_object(map: &Map<String, Value>) -> Option<ParsedDownload> {
    let url = value_text_ref(Some(object_text_value(
        map,
        &["url", "href", "download_url"],
    )?));
    if !url.contains(".zip") {
        return None;
    }
    Some(ParsedDownload {
        item_id: object_text_value(map, &["id", "cid", "itemid"])
            .map(value_text)
            .unwrap_or_default(),
        name: object_text_value(map, &["song", "title", "name"])
            .map(|name| clean_cell(value_text_ref(Some(name)).as_ref()).into_owned())
            .unwrap_or_else(|| "SRPG10 unlock".to_string()),
        details: String::new(),
        url: absolutize_url(&url),
        site_downloaded: false,
    })
}

fn absolutize_url(url: &str) -> String {
    let url = url.replace("\\/", "/");
    if url.starts_with("https://") || url.starts_with("http://") {
        url
    } else if url.starts_with('/') {
        format!("{BASE_ORIGIN}{url}")
    } else {
        format!("{BASE_ORIGIN}/{}", url.trim_start_matches("./"))
    }
}
