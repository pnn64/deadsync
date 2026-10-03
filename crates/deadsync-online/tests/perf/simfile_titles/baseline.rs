// Frozen from e314fc18d (0.5.1709).
use super::*;

pub(super) fn collect_simfile_titles(song_dir: &Path, keys: &mut HashSet<String>) {
    let Ok(entries) = fs::read_dir(song_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_simfile = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("sm") || ext.eq_ignore_ascii_case("ssc"));
        if !is_simfile {
            continue;
        }
        let Ok(file) = File::open(path) else {
            continue;
        };
        for line in BufReader::new(file).lines().take(64).map_while(Result::ok) {
            let Some(title) = line.trim_start().strip_prefix("#TITLE:") else {
                continue;
            };
            keys.insert(song_key(title.trim_end_matches(';')));
            break;
        }
    }
}
