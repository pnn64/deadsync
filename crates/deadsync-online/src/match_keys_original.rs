// Frozen starting implementations for differential tests and paired benchmarks.
// Matcher callbacks are adapted to main's borrowed metadata API on both sides;
// key normalization and matching rules retain their starting implementations.
use super::*;

pub(super) fn match_key(text: &str) -> String {
    let key: String = text
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|ch| ch.is_alphanumeric())
        .collect();
    if key.is_empty() {
        text.trim().to_lowercase()
    } else {
        key
    }
}

pub(super) fn containment(a: &str, b: &str) -> Option<usize> {
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let chars = short.chars().count();
    (chars >= MIN_CONTAINED_CHARS && long.contains(short)).then_some(chars)
}

pub(super) fn best_scored(scored: impl Iterator<Item = (usize, usize)>) -> Vec<usize> {
    let mut best = 0;
    let mut out = Vec::new();
    for (folder, score) in scored {
        if score > best {
            best = score;
            out.clear();
        }
        if score == best {
            out.push(folder);
        }
    }
    out
}

pub(super) fn settle<'a>(
    candidates: &[usize],
    artist: &str,
    tags: &impl Fn(usize) -> Option<&'a SimfileTags>,
) -> Option<SongMatch> {
    match candidates {
        [] => None,
        [only] => Some(SongMatch::Folder(*only)),
        _ => {
            let by_artist: Vec<usize> = if artist.is_empty() {
                Vec::new()
            } else {
                candidates
                    .iter()
                    .copied()
                    .filter(|&folder| {
                        tags(folder).is_some_and(|found| match_key(&found.artist) == artist)
                    })
                    .collect()
            };
            Some(match by_artist[..] {
                [only] => SongMatch::Folder(only),
                _ => SongMatch::Ambiguous,
            })
        }
    }
}

pub(super) fn match_song<'a>(
    index: &PackIndex,
    title: &str,
    artist: &str,
    tags: impl Fn(usize) -> Option<&'a SimfileTags>,
) -> SongMatch {
    let want = match_key(title);
    if want.is_empty() {
        return SongMatch::NotFound;
    }
    let artist = match_key(artist);
    let names: Vec<String> = index
        .folders
        .iter()
        .map(|folder| match_key(&folder.name))
        .collect();
    let named: Vec<usize> = (0..names.len()).filter(|&ix| names[ix] == want).collect();
    let titled_as = |folder: usize| {
        tags(folder).is_some_and(|tags| {
            match_key(&tags.title) == want
                || !tags.translit.is_empty() && match_key(&tags.translit) == want
        })
    };
    if !artist.is_empty() {
        let by_artist: Vec<usize> = (0..names.len())
            .filter(|&ix| names[ix] == want || titled_as(ix))
            .filter(|&ix| tags(ix).is_some_and(|found| match_key(&found.artist) == artist))
            .collect();
        match by_artist[..] {
            [only] => return SongMatch::Folder(only),
            [] => {}
            _ => return SongMatch::Ambiguous,
        }
    }
    if let [only] = named[..] {
        return SongMatch::Folder(only);
    }
    if !named.is_empty() {
        let titled: Vec<usize> = named.iter().copied().filter(|&ix| titled_as(ix)).collect();
        return settle(&titled, &artist, &tags)
            .or_else(|| settle(&named, &artist, &tags))
            .unwrap_or(SongMatch::Ambiguous);
    }
    let titled: Vec<usize> = (0..names.len()).filter(|&ix| titled_as(ix)).collect();
    if let Some(found) = settle(&titled, &artist, &tags) {
        return found;
    }
    let by_folder = best_scored(
        names
            .iter()
            .enumerate()
            .filter_map(|(ix, name)| Some((ix, containment(name, &want)?))),
    );
    if let Some(found) = settle(&by_folder, &artist, &tags) {
        return found;
    }
    let by_title = best_scored((0..names.len()).filter_map(|ix| {
        let tags = tags(ix)?;
        let title = containment(&match_key(&tags.title), &want);
        let translit = (!tags.translit.is_empty())
            .then(|| containment(&match_key(&tags.translit), &want))
            .flatten();
        Some((ix, title.max(translit)?))
    }));
    settle(&by_title, &artist, &tags).unwrap_or(SongMatch::NotFound)
}

pub(super) fn folders_needing_tags(index: &PackIndex, title: &str, artist: &str) -> Vec<usize> {
    let with_simfile = || {
        index
            .folders
            .iter()
            .enumerate()
            .filter(|(_, folder)| folder.simfile.is_some())
            .map(|(ix, _)| ix)
            .collect()
    };
    if !match_key(artist).is_empty() {
        return with_simfile();
    }
    let want = match_key(title);
    let named: Vec<usize> = index
        .folders
        .iter()
        .enumerate()
        .filter(|(_, folder)| match_key(&folder.name) == want)
        .map(|(ix, _)| ix)
        .collect();
    match named.len() {
        1 => Vec::new(),
        0 => with_simfile(),
        _ => named
            .into_iter()
            .filter(|&ix| index.folders[ix].simfile.is_some())
            .collect(),
    }
}
