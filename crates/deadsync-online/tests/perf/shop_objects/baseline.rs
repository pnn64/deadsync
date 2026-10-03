// Frozen from cafbd9e2a (0.5.1707).
use super::*;

pub(super) fn object_text(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|wanted| {
        map.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(wanted))
            .map(|(_, value)| value_text(value))
            .filter(|value| !value.is_empty())
    })
}

pub(super) fn download_from_object(map: &Map<String, Value>) -> Option<ParsedDownload> {
    let url = object_text(map, &["url", "href", "download_url"])?;
    if !url.contains(".zip") {
        return None;
    }
    Some(ParsedDownload {
        item_id: object_text(map, &["id", "cid", "itemid"]).unwrap_or_default(),
        name: object_text(map, &["song", "title", "name"])
            .map(|name| clean_cell(&name))
            .unwrap_or_else(|| "SRPG10 unlock".to_string()),
        details: String::new(),
        url: absolutize_url(&url),
        site_downloaded: false,
    })
}

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
