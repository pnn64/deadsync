//! Optional checks against the original third-party 3D skins.
use deadsync_assets::{init_paths, noteskin};
use deadsync_config::dirs::AppDirs;
use std::path::PathBuf;

#[test]
#[ignore = "requires DEADSYNC_3D_SKINS pointing to the directory containing the original 3D skins"]
fn original_skins_keep_model_geometry() {
    let source =
        PathBuf::from(std::env::var_os("DEADSYNC_3D_SKINS").expect("set DEADSYNC_3D_SKINS"))
            .canonicalize()
            .unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dirs = AppDirs {
        data_dir: workspace.clone(),
        exe_dir: workspace.clone(),
        cache_dir: workspace.join("target/three-d-skin-check"),
        portable: true,
    };
    init_paths(dirs.asset_paths(Some(&workspace))).unwrap();
    let style = noteskin::Style {
        num_cols: 4,
        num_players: 1,
    };
    for (name, layer_count) in [
        ("composite-3d", 4),
        ("FNF-3d", 1),
        ("FNF_Quantize-3d", 1),
        ("enchantment-original-3d", 3),
        ("enchantment-original-3d+", 6),
    ] {
        let skin = noteskin::load_song_skin(&style, &source, name, "")
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        for (index, slot) in skin.notes.iter().enumerate() {
            assert!(
                slot.model.is_some(),
                "{name} note {index}: {}",
                slot.texture_key()
            );
            assert!(!slot.model_fallback, "{name} note {index}");
        }
        for (column, layers) in skin.note_layers.iter().enumerate() {
            assert_eq!(layers.len(), layer_count, "{name} column {column}");
            assert!(
                layers.iter().all(|slot| slot.model.is_some()),
                "{name} column {column}"
            );
        }
        assert!(
            skin.lift_note_layers
                .iter()
                .flat_map(|layers| layers.iter())
                .all(|slot| slot.model.is_some()),
            "{name} lifts"
        );
        assert!(
            skin.mines.iter().flatten().all(|slot| slot.model.is_some()),
            "{name} mines"
        );
        for (column, direction) in ["Left", "Down", "Up", "Right"].into_iter().enumerate() {
            let layers = &skin.note_layers[column * noteskin::NUM_QUANTIZATIONS];
            if name == "composite-3d" {
                assert!(
                    layers[0]
                        .texture_key()
                        .ends_with(&format!("texcher_{}.png", direction.to_ascii_lowercase()))
                );
                assert_eq!(layers[0].uv_velocity, [0.25, 0.0]);
            } else if name == "FNF-3d" {
                assert!(
                    layers[0]
                        .texture_key()
                        .ends_with(&format!("{direction} Tap Note parts.png"))
                );
            }
        }
        eprintln!("{name}: {} notes with model geometry", skin.notes.len());
    }
}
