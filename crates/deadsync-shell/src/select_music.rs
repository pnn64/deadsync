use deadsync_profile::{PlayerSide, compat as profile};
use deadsync_theme_simply_love::views::{SelectMusicInitView, SelectMusicPlaylistView};
use log::warn;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[inline(always)]
fn path_key(path: &Path) -> String {
    let mut key = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.make_ascii_lowercase();
    }
    key
}

fn find_child_dir(root: &Path, name: &str) -> Option<PathBuf> {
    let exact = root.join(name);
    if exact.is_dir() {
        return Some(exact);
    }
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    std::fs::read_dir(root).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|found| found.eq_ignore_ascii_case(name)))
        .then_some(path)
    })
}

fn playlist_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
        })
        .collect();
    files.sort_by_cached_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| path.to_string_lossy().to_ascii_lowercase())
    });
    files
}

fn playlist_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn read_playlist(path: PathBuf, owner: Option<String>) -> Option<SelectMusicPlaylistView> {
    let name = playlist_name(&path)?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Some(SelectMusicPlaylistView {
            id: path_key(&path),
            owner,
            name,
            text,
        }),
        Err(error) => {
            warn!("Failed to read playlist '{}': {error}", path.display());
            None
        }
    }
}

fn machine_playlists() -> Vec<SelectMusicPlaylistView> {
    let dirs = deadlib_platform::dirs::app_dirs();
    let mut roots = Vec::with_capacity(2);
    if let Some(root) = find_child_dir(&dirs.data_dir, "playlists") {
        roots.push(root);
    }
    if !dirs.portable
        && let Some(root) = find_child_dir(&dirs.exe_dir, "playlists")
        && !roots.iter().any(|known| path_key(known) == path_key(&root))
    {
        roots.push(root);
    }

    let mut seen = HashSet::new();
    roots
        .into_iter()
        .flat_map(|root| playlist_files(&root))
        .filter_map(|path| {
            let name = playlist_name(&path)?;
            seen.insert(name.to_ascii_lowercase())
                .then(|| read_playlist(path, None))?
        })
        .collect()
}

fn profile_playlists() -> Vec<SelectMusicPlaylistView> {
    let mut seen_profiles = HashSet::new();
    let mut playlists = Vec::new();
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        if !seen_profiles.insert(profile_id.clone()) {
            continue;
        }
        let Some(root) =
            find_child_dir(&profile::local_profile_dir_for_id(&profile_id), "playlists")
        else {
            continue;
        };
        let display_name = profile::get_for_side(side).display_name;
        let owner = if display_name.trim().is_empty() {
            profile_id
        } else {
            display_name
        };
        playlists.extend(
            playlist_files(&root)
                .into_iter()
                .filter_map(|path| read_playlist(path, Some(owner.clone()))),
        );
    }
    playlists
}

pub(crate) fn init_view() -> SelectMusicInitView {
    let dirs = deadlib_platform::dirs::app_dirs();
    let mut playlists = machine_playlists();
    playlists.extend(profile_playlists());
    SelectMusicInitView {
        songs_root: dirs.songs_dir(),
        courses_root: dirs.courses_dir(),
        playlists,
    }
}
