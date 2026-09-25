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
                let base = &skin.receptor_off[column];
                let idle = skin.receptor_idle_glow_layers[column]
                    .as_ref()
                    .expect("composite keeps its idle overlay separate from its base");
                let press = skin.receptor_glow[column].as_ref().unwrap();
                assert_eq!(
                    skin.receptor_idle_glow,
                    noteskin::ReceptorIdleGlow::ActorEffect
                );
                assert_eq!(base.texture_key(), idle.texture_key());
                assert!(press.texture_key().contains("Tap Flash"));
                assert_eq!(base.model_effect.mode, noteskin::ModelEffectMode::None);
                assert_eq!(
                    idle.model_effect.mode,
                    noteskin::ModelEffectMode::DiffuseRamp
                );
                assert_eq!(idle.model_effect.clock, noteskin::ModelEffectClock::Beat);
                for (beat, alpha) in [(0.0, 0.875), (0.15, 0.5), (0.55, 0.25), (0.95, 1.0)] {
                    assert_eq!(skin.receptor_pulse.color_for_beat(beat), [1.0; 4]);
                    assert_eq!(base.model_draw_at(1.0, beat).tint, [1.0; 4]);
                    let glow = idle.model_draw_at(1.0, beat);
                    assert!(glow.blend_add);
                    assert_eq!(glow.tint[..3], [1.0; 3]);
                    assert!((glow.tint[3] - alpha).abs() < 1e-5, "beat {beat}: {glow:?}");
                }
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
