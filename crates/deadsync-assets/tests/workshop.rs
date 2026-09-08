//! Optional end-to-end validation of a locally packaged Cel/Metal workshop.
use deadsync_assets::{init_paths, noteskin, textures};
use deadsync_config::dirs::AppDirs;
use deadsync_noteskin::pack::{SLOTS, Selection};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
#[ignore = "requires DEADSYNC_WORKSHOP_PACK pointing to an extracted workshop pack"]
fn installed_workshop_resolves_and_loads_customized_skins() {
    let pack_root = PathBuf::from(
        std::env::var_os("DEADSYNC_WORKSHOP_PACK").expect("set DEADSYNC_WORKSHOP_PACK"),
    )
    .canonicalize()
    .unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dirs = AppDirs {
        data_dir: workspace.clone(),
        exe_dir: workspace.clone(),
        cache_dir: workspace.join("target/workshop/runtime-check"),
        portable: true,
    };
    let mut paths = dirs.asset_paths(Some(&workspace));
    paths.noteskin_pack_roots = vec![pack_root.parent().unwrap().to_path_buf()];
    init_paths(paths.clone()).unwrap();
    let packs = noteskin::pack_catalog();
    let pack = packs
        .iter()
        .find(|pack| pack.root == pack_root)
        .expect("pack discovered");
    let startup = textures::initial_texture_jobs([], &paths, |_| false);
    let pack_jobs: Vec<_> = startup
        .iter()
        .filter(|job| job.path.starts_with(&pack_root))
        .collect();
    assert_eq!(
        pack_jobs.len(),
        pack.manifest.skins.len(),
        "startup should only decode previews"
    );
    for job in &pack_jobs {
        assert!(
            job.path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("-preview.png")
        );
        assert_eq!(image::open(&job.path).unwrap().width(), 2048);
    }
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };
    for skin in &pack.manifest.skins {
        // Every manifest choice must compose with the base and resolve to existing files.
        for choice in &skin.options {
            let raw = format!("{}?{}={}", skin.id, choice.slot, choice.id);
            let data = pack
                .resolve(&Selection::parse(&raw).unwrap(), &paths.noteskin_roots)
                .unwrap();
            assert!(
                data.overrides.iter().all(|(_, source)| source.is_file()),
                "{raw}"
            );
        }
        let base = noteskin::load_itg_skin_cached(&style, &skin.id).unwrap();
        assert!(!base.notes.is_empty());
        assert!(base.mines.iter().any(Option::is_some));
        // Exercise both ends of each part's catalog together, including relocated mine models.
        let mut mine_widths = Vec::new();
        for last in [false, true] {
            let mut selection = Selection::parse(&skin.id).unwrap();
            for slot in SLOTS {
                let choices: Vec<_> = skin
                    .options
                    .iter()
                    .filter(|choice| choice.slot == slot && choice.id != "base")
                    .collect();
                if let Some(choice) = if last {
                    choices.last()
                } else {
                    choices.first()
                } {
                    selection.options.insert(slot.into(), choice.id.clone());
                }
            }
            let raw = selection.to_string();
            let customized = noteskin::load_itg_skin_cached(&style, &raw).unwrap();
            assert!(!Arc::ptr_eq(&base, &customized));
            assert!(Arc::ptr_eq(
                &customized,
                &noteskin::load_itg_skin_cached(&style, &raw).unwrap()
            ));
            let mine = customized
                .mines
                .iter()
                .flatten()
                .next()
                .expect("custom mine");
            assert!(mine.model.is_some(), "mine remains a model");
            mine_widths.push(mine.model.as_ref().unwrap().size()[0]);
            assert!(
                mine.source.texture_key().contains("Customizations/Mines/"),
                "{}",
                mine.source.texture_key()
            );
            assert!(
                customized.notes[0]
                    .source
                    .texture_key()
                    .contains("Customizations/Arrows/"),
                "{}",
                customized.notes[0].source.texture_key()
            );
            assert!(base.notes[0].source.texture_key().contains("Workshop/"));
            for slot in [mine, &customized.notes[0]] {
                let key = slot.source.texture_key();
                let path = textures::texture_key_source_path(key, key, |path| {
                    paths.resolve_asset_path(path)
                });
                image::open(path)
                    .expect("selected texture decodes through the gameplay preload path");
            }
            let doubles = noteskin::load_itg_skin_cached(
                &noteskin::Style {
                    num_cols: 8,
                    num_players: 1,
                },
                &raw,
            )
            .unwrap();
            assert_eq!(doubles.column_xs.len(), 8);
            eprintln!("Loaded {raw}");
        }
        assert!(
            mine_widths[1] < mine_widths[0],
            "Smaller changes the mine geometry"
        );
        eprintln!("Validated {}: {} choices", skin.id, skin.options.len());
    }
    use deadsync_noteskin::runtime::SkinPart;
    for cols in [4, 8] {
        let style = noteskin::Style {
            num_cols: cols,
            num_players: 1,
        };
        let base = noteskin::load_itg_skin_cached(&style, "cel").unwrap();
        let workshop =
            noteskin::load_itg_skin_cached(&style, "metal-workshop?arrows=ddr-vivid").unwrap();
        let mut mixed = (*base).clone();
        mixed.apply_part(&workshop, SkinPart::Arrows);
        mixed.apply_part(&workshop, SkinPart::Receptors);
        mixed.apply_part(&workshop, SkinPart::HoldActive);
        mixed.apply_part(&workshop, SkinPart::Lifts);
        assert_eq!(mixed.column_xs.len(), cols);
        assert_eq!(
            mixed.notes[0].texture_key(),
            workshop.notes[0].texture_key()
        );
        assert_eq!(
            mixed.mines[0].as_ref().unwrap().texture_key(),
            base.mines[0].as_ref().unwrap().texture_key()
        );
        assert_eq!(
            mixed.receptor_off[0].texture_key(),
            workshop.receptor_off[0].texture_key()
        );
        assert_eq!(
            mixed.hold.body_inactive.as_ref().unwrap().texture_key(),
            base.hold.body_inactive.as_ref().unwrap().texture_key()
        );
        let mut keys = Vec::new();
        mixed.for_each_slot(|slot| keys.push(slot.texture_key().to_string()));
        assert!(keys.contains(&workshop.notes[0].texture_key().to_string()));
        assert!(keys.contains(&base.mines[0].as_ref().unwrap().texture_key().to_string()));
    }
}
