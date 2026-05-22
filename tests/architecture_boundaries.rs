use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const GAME_UPWARD_DEP_BASELINE: &[(&str, &str, usize)] = &[
    ("src/game/course.rs", "config", 1),
    ("src/game/gameplay.rs", "assets", 1),
    ("src/game/gameplay.rs", "config", 8),
    ("src/game/gameplay.rs", "engine", 5),
    ("src/game/gameplay.rs", "screens", 3),
    ("src/game/gameplay/attacks.rs", "config", 2),
    ("src/game/gameplay/attacks.rs", "engine", 1),
    ("src/game/gameplay/autoplay.rs", "engine", 1),
    ("src/game/gameplay/clock.rs", "engine", 1),
    ("src/game/gameplay/input.rs", "config", 1),
    ("src/game/gameplay/input.rs", "engine", 2),
    ("src/game/online/arrowcloud.rs", "config", 1),
    ("src/game/online/arrowcloud.rs", "engine", 1),
    ("src/game/online/downloads.rs", "config", 2),
    ("src/game/online/downloads.rs", "engine", 1),
    ("src/game/online/groovestats.rs", "config", 2),
    ("src/game/online/groovestats.rs", "engine", 1),
    ("src/game/parsing/noteskin/compile.rs", "config", 1),
    ("src/game/parsing/noteskin/mod.rs", "assets", 1),
    ("src/game/parsing/noteskin/mod.rs", "config", 1),
    ("src/game/parsing/noteskin/mod.rs", "engine", 14),
    ("src/game/parsing/noteskin/model_cache.rs", "engine", 3),
    ("src/game/parsing/simfile.rs", "config", 3),
    ("src/game/parsing/simfile.rs", "engine", 1),
    ("src/game/parsing/simfile/cache.rs", "config", 1),
    ("src/game/parsing/simfile/scan.rs", "config", 3),
    ("src/game/parsing/song_lua/actor_host.rs", "assets", 3),
    ("src/game/parsing/song_lua/mod.rs", "engine", 2),
    ("src/game/parsing/song_lua/overlay.rs", "engine", 4),
    ("src/game/parsing/song_lua/tests.rs", "engine", 2),
    ("src/game/profile.rs", "config", 1),
    ("src/game/random_movies.rs", "config", 1),
    ("src/game/scores.rs", "config", 4),
    ("src/game/scores.rs", "engine", 2),
    ("src/game/scores/arrowcloud.rs", "config", 2),
    ("src/game/scores/arrowcloud.rs", "engine", 1),
    ("src/game/scores/groovestats.rs", "config", 6),
    ("src/game/scores/groovestats.rs", "engine", 1),
    ("src/game/scores/itl.rs", "config", 2),
    ("src/game/song.rs", "config", 9),
];

#[test]
fn game_upward_dependencies_do_not_grow() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let game_dir = root.join("src/game");
    let baseline = baseline_map();
    let mut failures = Vec::new();

    for file in rust_files(&game_dir) {
        let text = fs::read_to_string(&file).expect("source file should be readable");
        let rel = rel_path(&root, &file);

        for target in ["assets", "config", "engine", "screens", "app"] {
            let count = count_game_upward_refs(&text, target);
            let allowed = baseline
                .get(&(rel.clone(), target.to_owned()))
                .copied()
                .unwrap_or(0);

            if count > allowed {
                failures.push(format!(
                    "{rel} references crate::{target} {count} times, baseline is {allowed}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "game layer gained upward dependencies:\n{}",
        failures.join("\n")
    );
}

fn baseline_map() -> HashMap<(String, String), usize> {
    GAME_UPWARD_DEP_BASELINE
        .iter()
        .map(|(path, target, count)| (((*path).to_owned(), (*target).to_owned()), *count))
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rust_files(dir, &mut out);
    out.sort();
    out
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("source directory should be readable") {
        let path = entry.expect("source entry should be readable").path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("source file should be under manifest dir")
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn count_game_upward_refs(text: &str, target: &str) -> usize {
    if target == "config" {
        return count_token_refs(text, "crate::config");
    }
    text.match_indices(&format!("crate::{target}::")).count()
}

fn count_token_refs(text: &str, token: &str) -> usize {
    let mut count = 0;
    let mut rest = text;

    while let Some(index) = rest.find(token) {
        let after = &rest[index + token.len()..];
        if after
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
        {
            count += 1;
        }
        rest = after;
    }

    count
}
