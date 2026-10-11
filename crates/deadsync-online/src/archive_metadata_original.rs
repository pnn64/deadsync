// Frozen from 38890f14b70cc7aee1db6078a2ebd872e577195f; unchanged helpers and types are shared.
use super::*;

pub(super) fn group_folders(entries: &[ZipEntry]) -> Vec<SongFolder> {
    let mut folders: Vec<SongFolder> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (ix, entry) in entries.iter().enumerate() {
        let mut parts = entry
            .name
            .split(['/', '\\'])
            .filter(|part| !part.is_empty());
        let (Some(root), Some(folder)) = (parts.next(), parts.next()) else {
            continue;
        };
        if (parts.next().is_none() && !entry.is_dir()) || root.eq_ignore_ascii_case("__MACOSX") {
            continue;
        }
        let key = format!("{}/{}", root.to_lowercase(), folder.to_lowercase());
        let slot = *by_key.entry(key).or_insert_with(|| {
            folders.push(SongFolder {
                name: folder.to_owned(),
                entries: Vec::new(),
                simfile: None,
                audio: Vec::new(),
            });
            folders.len() - 1
        });
        folders[slot].entries.push(ix);
    }
    for folder in &mut folders {
        let (mut ssc, mut sm) = (None, None);
        for &ix in &folder.entries {
            let entry = &entries[ix];
            if entry.is_dir() {
                continue;
            }
            let mut parts = entry
                .name
                .split(['/', '\\'])
                .filter(|part| !part.is_empty());
            let depth = parts.clone().count();
            let Some(file) = parts.next_back() else {
                continue;
            };
            // AppleDouble companions: `._song.sm` is metadata, not a chart.
            if file.starts_with("._") {
                continue;
            }
            let extension = extension(file);
            if depth == 3 {
                if extension.eq_ignore_ascii_case("ssc") {
                    ssc = ssc.or(Some(ix));
                } else if extension.eq_ignore_ascii_case("sm") {
                    sm = sm.or(Some(ix));
                }
            }
            if AUDIO_EXTENSIONS
                .iter()
                .any(|audio| extension.eq_ignore_ascii_case(audio))
            {
                folder.audio.push(ix);
            }
        }
        folder.simfile = ssc.or(sm);
    }
    folders
}

pub(super) fn settle(
    candidates: &[usize],
    artist: &str,
    tags: &impl Fn(usize) -> Option<SimfileTags>,
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

pub(super) fn match_song(
    index: &PackIndex,
    title: &str,
    artist: &str,
    tags: impl Fn(usize) -> Option<SimfileTags>,
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
