use super::*;

pub(super) fn actor_texts(texts: Vec<String>) -> Box<[TextContent]> {
    texts
        .into_iter()
        .map(|text| {
            TextContent::inline_str(&text)
                .unwrap_or_else(|| TextContent::Shared(Arc::<str>::from(text)))
        })
        .collect::<Vec<_>>()
        .into_boxed_slice()
}
