// Frozen from 4ca2c55fba8006d83b9ea427b1922a4dc6448b31; test/benchmark oracle.
use super::{AUDIO_EXTENSIONS, PackIndex, song_preview};

pub(super) fn extension_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(super) fn is_audio(name: &str) -> bool {
    let extension = extension_of(name);
    AUDIO_EXTENSIONS.contains(&extension.as_str())
}

pub(super) fn is_chart_simfile(name: &str) -> bool {
    matches!(extension_of(name).as_str(), "sm" | "ssc")
}

pub(super) fn within_song(name: &str) -> &str {
    name.splitn(3, '/').nth(2).unwrap_or_default()
}

pub(super) fn audio_entry(
    index: &PackIndex,
    folder: usize,
    data: &song_preview::SongPreviewData,
) -> Option<(usize, bool)> {
    let song = &index.folders[folder];
    let named = |wanted: &str| -> Option<usize> {
        let wanted = wanted.trim().replace('\\', "/");
        if wanted.is_empty() {
            return None;
        }
        let wanted_file = wanted.rsplit('/').next().unwrap_or_default().to_lowercase();
        song.entries.iter().copied().find(|&entry| {
            let name = &index.entries[entry].name;
            if !is_audio(name) {
                return false;
            }
            let inside = within_song(name);
            inside.eq_ignore_ascii_case(&wanted)
                || inside
                    .rsplit('/')
                    .next()
                    .is_some_and(|file| file.to_lowercase() == wanted_file)
        })
    };
    if let Some(entry) = named(&data.preview_clip) {
        return Some((entry, true));
    }
    if let Some(entry) = named(&data.music) {
        return Some((entry, false));
    }
    song.audio.first().map(|&entry| (entry, false))
}
