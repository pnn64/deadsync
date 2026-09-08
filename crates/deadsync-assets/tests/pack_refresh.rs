use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use std::{fs, path::PathBuf, sync::Arc};

struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn downloaded_pack_appears_after_startup_and_queues_previews() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "deadsync-pack-refresh-{}-{stamp}",
        std::process::id()
    )));
    let dirs = AppDirs {
        data_dir: fixture.0.clone(),
        cache_dir: fixture.0.join("cache"),
        exe_dir: fixture.0.clone(),
        portable: true,
    };
    let paths = dirs.asset_paths(Some(&fixture.0));
    init_paths(paths).unwrap();
    let before = noteskin::pack_catalog();
    assert!(before.is_empty());

    let target = fixture.0.join("assets/noteskins/hurg");
    fs::create_dir_all(target.join("base")).unwrap();
    fs::write(
        target.join("base/metrics.ini"),
        "[Global]\nFallbackNoteSkin=common\n",
    )
    .unwrap();
    fs::write(target.join("base/NoteSkin.lua"), "return {}\n").unwrap();
    image::RgbaImage::new(2048, 2048)
        .save(target.join("preview.png"))
        .unwrap();
    fs::write(target.join("pack.json"), r#"{"schema":1,"id":"example","version":"1","source":"fixture","skins":[{"id":"new-skin","base":"base","preview":"preview.png","options":[]}]}"#).unwrap();
    assert!(
        !noteskin::is_pack_skin("new-skin"),
        "startup snapshot is still retained"
    );

    noteskin::refresh_packs(&target).unwrap();
    let after = noteskin::pack_catalog();
    assert!(noteskin::is_pack_skin("new-skin"));
    assert!(
        before.is_empty(),
        "readers retain their immutable previous snapshot"
    );
    assert!(!Arc::ptr_eq(&before, &after));
    let key = deadsync_assets::textures::canonical_texture_key(
        target.canonicalize().unwrap().join("preview.png"),
    );
    let texture =
        deadlib_assets::generated_texture(&key).expect("preview queued for render upload");
    assert_eq!(texture.image.dimensions(), (2048, 2048));
}
