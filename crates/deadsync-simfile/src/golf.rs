//! Golf.ini is pack metadata, independent of the simfile/song cache.
use std::path::Path;

pub fn load_pack(pack_dir: &Path) {
    let path = std::fs::read_dir(pack_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .find(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("Golf.ini"))
        })
        .map(|e| e.path());
    let data = path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| deadsync_chart::golf::GolfPars::parse(&s))
        .unwrap_or_default();
    deadsync_chart::golf::replace_pack(pack_dir, data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golf_pack_reload_replaces_changed_and_removed_metadata() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("deadsync-golf-{}-{unique}", std::process::id()));
        let song = root.join("Song");
        std::fs::create_dir_all(&song).unwrap();
        let simfile = song.join("song.ssc");
        let ini = root.join("gOlF.iNi");
        std::fs::write(&ini, "[Song]\nPar=2.1\n[Song:Challenge]\nPar=4\n").unwrap();
        load_pack(&root);
        assert_eq!(deadsync_chart::golf::par(&simfile, "Hard"), Some(2.1));
        assert_eq!(deadsync_chart::golf::par(&simfile, "Challenge"), Some(4.0));
        std::fs::write(&ini, "[Song]\nPar=3\n").unwrap();
        load_pack(&root);
        assert_eq!(deadsync_chart::golf::par(&simfile, "Challenge"), Some(3.0));
        std::fs::remove_file(ini).unwrap();
        load_pack(&root);
        assert_eq!(deadsync_chart::golf::par(&simfile, "Hard"), None);
        std::fs::remove_dir(song).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
