use crate::bgchanges::split_bgchange_sets_like_itg;
use crate::bgchanges::{
    bgchange_field_rejects_non_media, parse_bgchange_color, parse_bgchange_effect,
    parse_bgchange_rate, parse_bgchange_transition,
};
use crate::cache::{
    SerializableSongBackgroundLuaChange, SerializableSongForegroundChange,
    SerializableSongForegroundLuaChange,
};
use crate::media::{
    is_bgchange_movie_path, is_mac_resource_fork, list_song_dir_rel_entries,
    path_uses_lua_like_itg, resolve_foreground_media_path, resolve_song_path_like_itg,
    song_lua_entry_path_like_itg,
};
use crate::tags::extract_named_tag_values;
use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget};
use rssp::parse::{decode_bytes, extract_bgchanges_values, unescape_tag};
use std::path::{Path, PathBuf};

pub fn simfile_uses_lua(song_dir: &Path, simfile_data: &[u8], background_tag: &str) -> bool {
    if resolve_song_path_like_itg(song_dir, background_tag)
        .is_some_and(|path| path_uses_lua_like_itg(&path))
    {
        return true;
    }
    let entries = list_song_dir_rel_entries(song_dir);
    bgchange_values_use_lua(song_dir, &extract_bgchanges_values(simfile_data), &entries)
        || bgchange_values_use_lua(
            song_dir,
            &extract_named_tag_values(simfile_data, &[b"#FGCHANGES:"]),
            &entries,
        )
}

pub fn extract_foreground_changes(
    song_dir: &Path,
    simfile_data: &[u8],
) -> Vec<SerializableSongForegroundChange> {
    let entries = list_song_dir_rel_entries(song_dir);
    let mut out = Vec::new();
    for raw in extract_named_tag_values(simfile_data, &[b"#FGCHANGES:"]) {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        for fields in split_bgchange_sets_like_itg(&text, &entries) {
            let Some(start_beat) = parse_start_beat(&fields) else {
                continue;
            };
            let Some(target) = fields.get(1) else {
                continue;
            };
            let Some(path) = resolve_foreground_media_path(song_dir, target) else {
                continue;
            };
            out.push(SerializableSongForegroundChange {
                start_beat,
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    sort_dedup_path_changes(&mut out, |change| change.start_beat, |change| &change.path);
    out
}

pub fn extract_foreground_lua_changes(
    song_dir: &Path,
    simfile_data: &[u8],
) -> Vec<SerializableSongForegroundLuaChange> {
    let entries = list_song_dir_rel_entries(song_dir);
    let mut out = Vec::new();
    for raw in extract_named_tag_values(simfile_data, &[b"#FGCHANGES:"]) {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        for fields in split_bgchange_sets_like_itg(&text, &entries) {
            let Some(start_beat) = parse_start_beat(&fields) else {
                continue;
            };
            let Some(target) = fields.get(1) else {
                continue;
            };
            let Some(path) = resolve_song_path_like_itg(song_dir, target)
                .filter(|path| path_uses_lua_like_itg(path))
            else {
                continue;
            };
            let path = song_lua_entry_path_like_itg(path);
            out.push(SerializableSongForegroundLuaChange {
                start_beat,
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    sort_dedup_path_changes(&mut out, |change| change.start_beat, |change| &change.path);
    out
}

pub fn extract_background_lua_changes(
    song_dir: &Path,
    simfile_data: &[u8],
    background_tag: &str,
) -> Vec<SerializableSongBackgroundLuaChange> {
    let entries = list_song_dir_rel_entries(song_dir);
    let mut out = Vec::new();
    let mut push_change = |start_beat: f32, path: PathBuf| {
        let path = song_lua_entry_path_like_itg(path);
        out.push(SerializableSongBackgroundLuaChange {
            start_beat,
            path: path.to_string_lossy().into_owned(),
        });
    };

    if let Some(path) = resolve_song_path_like_itg(song_dir, background_tag)
        .filter(|path| path_uses_lua_like_itg(path))
    {
        push_change(0.0, path);
    }

    for raw in extract_bgchanges_values(simfile_data) {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        for fields in split_bgchange_sets_like_itg(&text, &entries) {
            let Some(start_beat) = parse_start_beat(&fields) else {
                continue;
            };
            let Some(target) = fields.get(1) else {
                continue;
            };
            let Some(path) = resolve_song_path_like_itg(song_dir, target)
                .filter(|path| path_uses_lua_like_itg(path))
            else {
                continue;
            };
            push_change(start_beat, path);
        }
    }

    sort_dedup_path_changes(&mut out, |change| change.start_beat, |change| &change.path);
    out
}

pub fn resolve_background_changes_from_roots(
    song_dir: &Path,
    simfile_data: &[u8],
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
) -> Vec<SongBackgroundChange> {
    resolve_background_changes_from_values(
        song_dir,
        extract_bgchanges_values(simfile_data),
        song_movie_roots,
        random_movie_roots,
        &[],
        false,
        true,
    )
}

pub fn resolve_background_layer2_changes_from_roots(
    song_dir: &Path,
    simfile_data: &[u8],
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
    bg_animation_roots: &[PathBuf],
) -> Vec<SongBackgroundChange> {
    resolve_background_changes_from_values(
        song_dir,
        extract_named_tag_values(simfile_data, &[b"#BGCHANGES2:"]),
        song_movie_roots,
        random_movie_roots,
        bg_animation_roots,
        true,
        false,
    )
}

fn resolve_background_changes_from_values(
    song_dir: &Path,
    values: Vec<&[u8]>,
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
    bg_animation_roots: &[PathBuf],
    allow_bg_animation: bool,
    apply_song_movie_fallback: bool,
) -> Vec<SongBackgroundChange> {
    let entries = list_song_dir_rel_entries(song_dir);
    let mut out: Vec<SongBackgroundChange> = Vec::new();
    let mut saw_no_song_bg = false;
    for raw in values {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        for fields in split_bgchange_sets_like_itg(&text, &entries) {
            let Some(change) = parse_background_change_set(
                song_dir,
                &fields,
                song_movie_roots,
                random_movie_roots,
                bg_animation_roots,
                allow_bg_animation,
            ) else {
                continue;
            };
            if matches!(change.target, SongBackgroundChangeTarget::NoSongBg) {
                saw_no_song_bg = true;
                continue;
            }
            upsert_background_change(&mut out, change);
        }
    }

    if !apply_song_movie_fallback {
        out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
        return out;
    }

    let has_explicit_movie = out.iter().any(|change| {
        matches!(
            change.target,
            SongBackgroundChangeTarget::File(ref path) if is_bgchange_movie_path(path)
        )
    });
    let beat_zero_still_ix = out
        .iter()
        .enumerate()
        .filter(|(_, change)| {
            change.start_beat <= 0.0
                && matches!(
                    change.target,
                    SongBackgroundChangeTarget::File(ref path) if !is_bgchange_movie_path(path)
                )
        })
        .map(|(ix, _)| ix)
        .last();
    let blocks_beat_zero = out.iter().any(|change| {
        change.start_beat <= 0.0 && !matches!(change.target, SongBackgroundChangeTarget::File(_))
    });
    let has_any_file = out
        .iter()
        .any(|change| matches!(change.target, SongBackgroundChangeTarget::File(_)));
    let movies = list_bgchange_song_movies(song_dir);
    if movies.len() == 1 && !has_explicit_movie {
        let movie = movies[0].clone();
        if saw_no_song_bg {
            if let Some(ix) = beat_zero_still_ix {
                out[ix].target = SongBackgroundChangeTarget::File(movie);
            } else if !blocks_beat_zero {
                out.push(SongBackgroundChange::new(
                    0.0,
                    SongBackgroundChangeTarget::File(movie),
                ));
            }
        } else if !has_any_file && !blocks_beat_zero {
            out.push(SongBackgroundChange::new(
                0.0,
                SongBackgroundChangeTarget::File(movie),
            ));
        }
    }
    out.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
    out
}

fn parse_background_change_set(
    song_dir: &Path,
    fields: &[String],
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
    bg_animation_roots: &[PathBuf],
    allow_bg_animation: bool,
) -> Option<SongBackgroundChange> {
    let start_beat = fields.first()?.trim().parse::<f32>().unwrap_or(0.0);
    if bgchange_field_rejects_non_media(fields.get(1)?) {
        return None;
    }
    let target = resolve_bgchange_target_like_itg(
        song_dir,
        fields.get(1)?,
        song_movie_roots,
        random_movie_roots,
        bg_animation_roots,
        allow_bg_animation,
    )?;
    if fields
        .get(7)
        .is_some_and(|field| bgchange_field_rejects_non_media(field))
    {
        return None;
    }
    let mut change = SongBackgroundChange::new(start_beat, target);
    change.rate = parse_bgchange_rate(fields.get(2).map(String::as_str));
    change.transition = parse_bgchange_transition(
        fields.get(3).map(String::as_str),
        fields.get(8).map(String::as_str),
    );
    change.effect = parse_bgchange_effect(
        fields.get(4).map(String::as_str),
        fields.get(5).map(String::as_str),
        fields.get(6).map(String::as_str),
    );
    change.file2 = fields.get(7).and_then(|field| {
        resolve_bgchange_file_like_itg(song_dir, field, song_movie_roots, random_movie_roots)
    });
    change.color1 = fields.get(9).and_then(|field| parse_bgchange_color(field));
    change.color2 = fields.get(10).and_then(|field| parse_bgchange_color(field));
    Some(change)
}

fn resolve_bgchange_target_like_itg(
    song_dir: &Path,
    file1: &str,
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
    bg_animation_roots: &[PathBuf],
    allow_bg_animation: bool,
) -> Option<SongBackgroundChangeTarget> {
    let file1 = file1.trim();
    if file1.is_empty() {
        return None;
    }
    if file1.eq_ignore_ascii_case("-nosongbg-") {
        return Some(SongBackgroundChangeTarget::NoSongBg);
    }
    if file1.eq_ignore_ascii_case("-random-") {
        return Some(SongBackgroundChangeTarget::Random);
    }

    resolve_bgchange_file_like_itg(song_dir, file1, song_movie_roots, random_movie_roots)
        .map(SongBackgroundChangeTarget::File)
        .or_else(|| {
            (allow_bg_animation && bgchange_animation_exists(file1, bg_animation_roots))
                .then(|| SongBackgroundChangeTarget::Animation(file1.to_string()))
        })
}

fn resolve_bgchange_file_like_itg(
    song_dir: &Path,
    target: &str,
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
) -> Option<PathBuf> {
    let target = target.trim();
    if target.is_empty()
        || target.eq_ignore_ascii_case("-nosongbg-")
        || target.eq_ignore_ascii_case("-random-")
    {
        return None;
    }
    resolve_song_path_like_itg(song_dir, target)
        .filter(|path| path.exists())
        .or_else(|| {
            resolve_global_bgchange_movie_like_itg(
                song_dir,
                target,
                song_movie_roots,
                random_movie_roots,
            )
        })
}

fn resolve_global_bgchange_movie_like_itg(
    song_dir: &Path,
    target: &str,
    song_movie_roots: &[PathBuf],
    random_movie_roots: &[PathBuf],
) -> Option<PathBuf> {
    if target.eq_ignore_ascii_case("-nosongbg-") {
        return None;
    }
    if let Some(group) = song_group_name(song_dir) {
        let grouped = format!("{group}/{target}");
        if let Some(path) = resolve_first_root_file(song_movie_roots, &grouped) {
            return Some(path);
        }
    }
    resolve_first_root_file(song_movie_roots, target)
        .or_else(|| resolve_first_root_file(random_movie_roots, target))
}

fn resolve_first_root_file(roots: &[PathBuf], target: &str) -> Option<PathBuf> {
    roots
        .iter()
        .find_map(|root| resolve_song_path_like_itg(root, target).filter(|path| path.is_file()))
}

fn bgchange_animation_exists(target: &str, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let Some(path) = resolve_song_path_like_itg(root, target) else {
            return false;
        };
        if path.is_file() {
            return true;
        }
        path.is_dir()
            && (resolve_song_path_like_itg(&path, "default.lua").is_some_and(|p| p.is_file())
                || resolve_song_path_like_itg(&path, "default.xml").is_some_and(|p| p.is_file()))
    })
}

fn song_group_name(song_dir: &Path) -> Option<String> {
    song_dir
        .parent()?
        .file_name()?
        .to_str()
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn list_bgchange_song_movies(song_dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = std::fs::read_dir(song_dir) else {
        return Vec::new();
    };
    let mut files = read_dir
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            !is_mac_resource_fork(path) && path.is_file() && is_bgchange_movie_path(path)
        })
        .collect::<Vec<_>>();
    files.sort_by_cached_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    });
    files
}

fn upsert_background_change(out: &mut Vec<SongBackgroundChange>, change: SongBackgroundChange) {
    if let Some(slot) = out
        .iter_mut()
        .find(|existing| existing.start_beat == change.start_beat)
    {
        *slot = change;
    } else {
        out.push(change);
    }
}

fn bgchange_target_uses_lua(song_dir: &Path, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty()
        || target.eq_ignore_ascii_case("-nosongbg-")
        || target.eq_ignore_ascii_case("-random-")
    {
        return false;
    }
    resolve_song_path_like_itg(song_dir, target).is_some_and(|path| path_uses_lua_like_itg(&path))
}

fn bgchange_values_use_lua(song_dir: &Path, values: &[&[u8]], entries: &[String]) -> bool {
    values.iter().copied().any(|raw| {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        split_bgchange_sets_like_itg(&text, entries)
            .into_iter()
            .any(|fields| {
                fields
                    .get(1)
                    .is_some_and(|target| bgchange_target_uses_lua(song_dir, target))
            })
    })
}

fn parse_start_beat(fields: &[String]) -> Option<f32> {
    fields
        .first()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|value| value.is_finite())
}

fn sort_dedup_path_changes<T>(
    out: &mut Vec<T>,
    start_beat: impl Fn(&T) -> f32,
    path: impl Fn(&T) -> &str,
) {
    out.sort_by(|left, right| {
        start_beat(left)
            .total_cmp(&start_beat(right))
            .then_with(|| path(left).cmp(path(right)))
    });
    out.dedup_by(|left, right| {
        start_beat(left).to_bits() == start_beat(right).to_bits() && path(left) == path(right)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_chart::SongBackgroundChangeTarget;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn simfile_uses_lua_detects_background_and_fgchanges() {
        let root = test_dir("uses-lua");
        let song_dir = root.join("Song");
        let fg_dir = song_dir.join("Visuals");
        fs::create_dir_all(&fg_dir).unwrap();
        fs::write(song_dir.join("modchart.lua"), b"lua").unwrap();
        fs::write(fg_dir.join("default.lua"), b"lua").unwrap();

        assert!(simfile_uses_lua(
            &song_dir,
            b"#TITLE:Lua Test;#BACKGROUND:modchart.lua;",
            "modchart.lua",
        ));
        assert!(simfile_uses_lua(
            &song_dir,
            b"#TITLE:Lua Test;#FGCHANGES:0=Visuals=1=0=0=0=0;",
            "",
        ));
    }

    #[test]
    fn extracts_foreground_media_and_lua_changes() {
        let root = test_dir("fgchanges");
        let song_dir = root.join("Song");
        let media_dir = song_dir.join("animations");
        let lua_dir = song_dir.join("scripts");
        fs::create_dir_all(&media_dir).unwrap();
        fs::create_dir_all(&lua_dir).unwrap();
        let movie = media_dir.join("badapple.avi");
        let default_lua = lua_dir.join("Default.lua");
        fs::write(&movie, b"avi").unwrap();
        fs::write(&default_lua, b"lua").unwrap();

        let media = extract_foreground_changes(
            &song_dir,
            b"#FGCHANGES:4=animations=1=0=0=0=0,8=scripts=1=0=0=0=0;",
        );
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].start_beat, 4.0);
        assert_eq!(PathBuf::from(&media[0].path), movie);

        let lua = extract_foreground_lua_changes(
            &song_dir,
            b"#FGCHANGES:4=animations=1=0=0=0=0,8=scripts=1=0=0=0=0;",
        );
        assert_eq!(lua.len(), 1);
        assert_eq!(lua[0].start_beat, 8.0);
        assert_eq!(PathBuf::from(&lua[0].path), default_lua);
    }

    #[test]
    fn extracts_background_lua_changes() {
        let root = test_dir("bgchanges");
        let song_dir = root.join("Song");
        let bg_dir = song_dir.join("BG");
        let tagged_lua = song_dir.join("modchart.lua");
        fs::create_dir_all(&bg_dir).unwrap();
        let bg_default = bg_dir.join("default.lua");
        fs::write(&tagged_lua, b"lua").unwrap();
        fs::write(&bg_default, b"lua").unwrap();

        let changes = extract_background_lua_changes(
            &song_dir,
            b"#BGCHANGES:2=BG=1=0=0=0=0;",
            "modchart.lua",
        );

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].start_beat, 0.0);
        assert_eq!(PathBuf::from(&changes[0].path), tagged_lua);
        assert_eq!(changes[1].start_beat, 2.0);
        assert_eq!(PathBuf::from(&changes[1].path), bg_default);
    }

    #[test]
    fn resolves_root_random_movie_file() {
        let root = test_dir("bgchange-root-random-movie");
        let song_dir = root.join("songs").join("In The Groove").join("Anubis");
        let random_root = root.join("RandomMovies");
        fs::create_dir_all(&song_dir).unwrap();
        fs::create_dir_all(&random_root).unwrap();
        let movie = random_root.join("EV01439N.mpg");
        fs::write(&movie, b"mpg").unwrap();

        let changes = resolve_background_changes_from_roots(
            &song_dir,
            b"#BGCHANGES:8.000=EV01439N.mpg=1.000=0=0=1,;",
            &[],
            &[random_root],
        );

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].start_beat, 8.0);
        assert!(matches!(
            &changes[0].target,
            SongBackgroundChangeTarget::File(path) if path == &movie
        ));
    }

    #[test]
    fn keeps_named_random_movie_after_random_marker() {
        let root = test_dir("bgchange-named-after-random");
        let song_dir = root.join("songs").join("In The Groove 2").join("Bloodrush");
        let random_root = root.join("RandomMovies");
        let random_group = random_root.join("In The Groove 2");
        fs::create_dir_all(&song_dir).unwrap();
        fs::create_dir_all(&random_group).unwrap();
        let still = song_dir.join("Bloodrush-bg.png");
        let first_movie = random_group.join("628_JumpBack.mpg");
        let later_movie = random_group.join("963_JumpBack.mpg");
        fs::write(&still, b"png").unwrap();
        fs::write(&first_movie, b"mpg").unwrap();
        fs::write(&later_movie, b"mpg").unwrap();

        let changes = resolve_background_changes_from_roots(
            &song_dir,
            b"#BGCHANGES:0.000=Bloodrush-bg.png=1.000=0=0=1=====,\
              12.000=In The Groove 2/628_JumpBack.mpg=0.000=0=0=1===FadeRight==,\
              29.000=-random-=1.000=0=0=1=====,\
              61.000=In The Groove 2/963_JumpBack.mpg=1.000=0=0=1=====,;",
            &[],
            &[random_root],
        );

        assert_eq!(changes.len(), 4);
        assert_eq!(
            changes
                .iter()
                .map(|change| change.start_beat)
                .collect::<Vec<_>>(),
            vec![0.0, 12.0, 29.0, 61.0]
        );
        assert!(matches!(
            &changes[1].target,
            SongBackgroundChangeTarget::File(path) if path == &first_movie
        ));
        assert!(matches!(
            &changes[2].target,
            SongBackgroundChangeTarget::Random
        ));
        assert!(matches!(
            &changes[3].target,
            SongBackgroundChangeTarget::File(path) if path == &later_movie
        ));
        assert_eq!(changes[1].rate, 0.0);
        assert_eq!(changes[1].transition, "FadeRight");
        assert_eq!(changes[2].rate, 1.0);
        assert!(changes[2].transition.is_empty());
    }

    #[test]
    fn parses_effect_and_color_fields() {
        let root = test_dir("bgchange-effect-color");
        let song_dir = root
            .join("songs")
            .join("In The Groove 2")
            .join("Agent Blatant");
        let random_root = root.join("RandomMovies");
        let random_group = random_root.join("In The Groove 2");
        fs::create_dir_all(&song_dir).unwrap();
        fs::create_dir_all(&random_group).unwrap();
        let movie = random_group.join("429_JumpBack.mpg");
        fs::write(&movie, b"mpg").unwrap();

        let changes = resolve_background_changes_from_roots(
            &song_dir,
            b"#BGCHANGES:120.000=In The Groove 2/429_JumpBack.mpg=1.000=0=0=1=SongBgWithMovieViz====0.5^0.5^0.5^1,;",
            &[],
            &[random_root],
        );

        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0].target,
            SongBackgroundChangeTarget::File(path) if path == &movie
        ));
        assert_eq!(changes[0].effect, "SongBgWithMovieViz");
        assert_eq!(changes[0].color2, Some([0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn resolves_layer2_global_flash_animation() {
        let root = test_dir("bgchange-layer2-flash");
        let song_dir = root.join("songs").join("Pack").join("Song");
        let bg_anim_root = root.join("BGAnimations");
        let white_flash = bg_anim_root.join("white flash");
        fs::create_dir_all(&song_dir).unwrap();
        fs::create_dir_all(&white_flash).unwrap();
        fs::write(white_flash.join("default.lua"), "return Def.Quad{}").unwrap();

        let changes = resolve_background_layer2_changes_from_roots(
            &song_dir,
            b"#BGCHANGES2:32.000=white flash=1.000=0=0=1=====,;",
            &[],
            &[],
            &[bg_anim_root],
        );

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].start_beat, 32.0);
        assert!(matches!(
            &changes[0].target,
            SongBackgroundChangeTarget::Animation(name) if name == "white flash"
        ));
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-changes-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
