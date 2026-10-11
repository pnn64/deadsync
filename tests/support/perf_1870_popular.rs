// Frozen from 1c41dfa1e9cadf9e14fe64051c9c9cfa387a6e3e: fetch_all entry conversion and Entry::best_banner.
fn best_banner(entry: &Entry) -> Option<String> {
    entry
        .sm_banner_url
        .clone()
        .or_else(|| entry.md_banner_url.clone())
        .or_else(|| entry.banner_url.clone())
}

pub fn into_pack(entry: Entry) -> PopularPack {
    PopularPack {
        name: entry.name.clone(),
        popularity: entry.popularity,
        simfile_count: entry.simfile_count,
        banner_url: best_banner(&entry),
    }
}
