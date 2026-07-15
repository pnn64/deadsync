use deadlib_platform::dirs;
mod texture;

pub use self::texture::{
    SpriteSlot, SpriteSource, build_model_geometry, load_itg_model_slots_from_path, test_model_slot,
};
use self::texture::{
    apply_model_slot_plan, itg_apply_frame_override, itg_apply_state_properties_from_commands,
    itg_slot_from_path, itg_slot_from_path_all_frames, itg_slot_from_path_animated,
    itg_slot_from_path_with_frame, mine_fill_slots,
};
#[cfg(test)]
use self::texture::{itg_apply_state_properties_from_script, itg_register_texture_dims_for_path};
pub use deadsync_noteskin::{
    AnimationRate, ExplosionAnimation, ExplosionSegment, ExplosionState, ExplosionVisualState,
    GlowEffect, ModelAutoRotKey, ModelDrawState, ModelEffectClock, ModelEffectMode,
    ModelEffectState, ModelMesh, ModelTweenSegment, ModelVertex, NOTE_ANIM_PART_COUNT,
    NUM_QUANTIZATIONS, NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartAnimation,
    NotePartTextureTranslate, Quantization, ReceptorGlowBehavior, ReceptorPulse,
    ReceptorReverseBehavior, ReceptorReverseState, ReceptorStepBehavior, ReceptorStepBehaviors,
    SpriteDefinition, Style, TweenType,
};
use deadsync_noteskin::{
    compiled as noteskin_compiled, compiler as noteskin_compiler, itg as noteskin_itg,
    script as noteskin_script,
};
use log::warn;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

pub type TapExplosion = deadsync_noteskin::TapExplosion<SpriteSlot>;
pub type HoldVisuals = deadsync_noteskin::HoldVisuals<SpriteSlot>;
pub type Noteskin = deadsync_noteskin::NoteskinRuntime<SpriteSlot>;

static ITG_SKIN_CACHE: OnceLock<noteskin_itg::ItgSkinRuntimeCache<Noteskin>> = OnceLock::new();

pub fn clear_itg_runtime_caches() {
    if let Some(cache) = ITG_SKIN_CACHE.get() {
        cache.clear();
    }
    noteskin_itg::clear_data_cache();
    noteskin_itg::clear_lookup_caches();
}

fn noteskin_roots() -> Vec<PathBuf> {
    let mut roots = dirs::app_dirs().noteskin_roots();
    if let Ok(cwd) = std::env::current_dir() {
        add_workspace_noteskin_roots(&mut roots, &cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        add_workspace_noteskin_roots(&mut roots, exe_dir);
    }
    add_workspace_noteskin_roots(&mut roots, Path::new(env!("CARGO_MANIFEST_DIR")));
    roots
}

fn add_workspace_noteskin_roots(roots: &mut Vec<PathBuf>, base: &Path) {
    for ancestor in base.ancestors() {
        for root in [
            ancestor.join("assets").join("noteskins"),
            ancestor.join("deadsync").join("assets").join("noteskins"),
        ] {
            if root.is_dir() && !roots.iter().any(|existing| existing == &root) {
                roots.push(root);
            }
        }
    }
}

pub fn song_lua_noteskin_resolve_path(skin: &str, button: &str, element: &str) -> Option<PathBuf> {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_resolve_path_from_roots(&roots, "dance", skin, button, element)
}

pub fn song_lua_noteskin_metric(skin: &str, element: &str, value: &str) -> Option<String> {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_metric_from_roots(&roots, "dance", skin, element, value)
}

pub fn song_lua_noteskin_metric_f(skin: &str, element: &str, value: &str) -> Option<f32> {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_metric_f_from_roots(&roots, "dance", skin, element, value)
}

pub fn song_lua_noteskin_metric_b(skin: &str, element: &str, value: &str) -> Option<bool> {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_metric_b_from_roots(&roots, "dance", skin, element, value)
}

pub fn song_lua_noteskin_exists(skin: &str) -> bool {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_exists_from_roots(&roots, "dance", skin)
}

pub fn song_lua_noteskin_names() -> Vec<String> {
    let roots = noteskin_roots();
    noteskin_itg::song_lua_noteskin_names_from_roots(&roots, "dance")
}

fn noteskin_cache_dir() -> PathBuf {
    dirs::app_dirs().noteskin_cache_dir()
}

pub fn load_itg_skin_cached(style: &Style, skin: &str) -> Result<Arc<Noteskin>, String> {
    ITG_SKIN_CACHE
        .get_or_init(noteskin_itg::ItgSkinRuntimeCache::default)
        .get_or_load(style, skin, || load_itg_skin(style, skin))
}

pub fn prewarm_itg_preview_cache() {
    let _ = compile_all_itg_caches_with_progress(|_, _, _, _| {});
    let roots = noteskin_roots();
    let skins = noteskin_itg::discover_skins(&roots, "dance");
    let styles = [
        Style {
            num_cols: 4,
            num_players: 1,
        },
        Style {
            num_cols: 8,
            num_players: 1,
        },
    ];

    for style in styles {
        for skin in &skins {
            if let Err(err) = load_itg_skin_cached(&style, skin) {
                warn!(
                    "noteskin prewarm failed for '{}' ({} columns): {}",
                    skin, style.num_cols, err
                );
            }
        }
    }
}

pub type CompileAllItgSummary = noteskin_compiler::CompileAllItgSummary;

pub fn compile_all_itg_caches_with_progress<F>(mut on_progress: F) -> CompileAllItgSummary
where
    F: FnMut(usize, usize, &str, &str),
{
    clear_itg_runtime_caches();
    let roots = noteskin_roots();
    let cache_dir = noteskin_cache_dir();
    noteskin_compiler::compile_all_itg_caches_with_progress(
        &cache_dir,
        &roots,
        "dance",
        &mut on_progress,
    )
}

pub fn load_itg_default(style: &Style) -> Result<Noteskin, String> {
    let roots = noteskin_roots();
    let loaded = noteskin_itg::load_itg_default_from_roots(&roots, "dance", |root, game, skin| {
        load_itg(root, game, skin, style)
    })?;
    if loaded.used_default_fallback {
        warn!(
            "ITG default noteskin load failed; using dance/{} fallback",
            loaded.skin
        );
    }
    Ok(loaded.value)
}

pub fn load_itg_skin(style: &Style, skin: &str) -> Result<Noteskin, String> {
    let roots = noteskin_roots();
    let loaded =
        noteskin_itg::load_itg_skin_from_roots(&roots, "dance", skin, |root, game, skin| {
            load_itg(root, game, skin, style)
        })?;
    if loaded.used_default_fallback {
        warn!(
            "ITG default noteskin load failed; using dance/{} fallback",
            loaded.skin
        );
    }
    Ok(loaded.value)
}

pub fn load_itg(root: &Path, game: &str, skin: &str, style: &Style) -> Result<Noteskin, String> {
    let data = noteskin_itg::load_noteskin_data_cached(root, game, skin)?;
    let cache_dir = noteskin_cache_dir();
    let bundle = noteskin_compiler::load_or_compile(&cache_dir, game, &data)?;
    load_itg_sprite_noteskin_compiled(&data, style, &bundle.loader, &bundle.actors).map_err(|err| {
        format!(
            "failed to load compiled noteskin '{}/{}': {}",
            game, data.name, err
        )
    })
}

fn load_itg_sprite_noteskin_compiled(
    data: &noteskin_itg::NoteskinData,
    style: &Style,
    compiled: &noteskin_compiled::CompiledLoader,
    compiled_actors: &noteskin_compiled::CompiledActors,
) -> Result<Noteskin, String> {
    deadsync_noteskin::itg_noteskin_runtime_with_ops_compiled(
        data,
        *style,
        compiled,
        compiled_actors,
        NUM_QUANTIZATIONS,
        itg_compiled_sprite_ops(),
    )
}

fn itg_apply_loader_command(sprites: &mut [ItgLuaResolvedSprite], command: Option<&str>) {
    deadsync_noteskin::itg_apply_loader_command(sprites, command, |slot, command| {
        noteskin_script::itg_apply_parent_command(&mut slot.def, &mut slot.model_draw, command);
    });
}

type ItgLuaResolvedSprite = deadsync_noteskin::ItgResolvedSprite<SpriteSlot>;

fn itg_slot_with_active_cmd(
    slot: &SpriteSlot,
    commands: &HashMap<String, String>,
    active_key: &str,
) -> SpriteSlot {
    deadsync_noteskin::itg_slot_with_active_model_draw(
        slot,
        commands,
        active_key,
        itg_apply_model_draw,
    )
}

fn itg_compiled_sprite_ops() -> deadsync_noteskin::ItgCompiledSpriteOps<SpriteSlot> {
    deadsync_noteskin::ItgCompiledSpriteOps {
        load_texture: itg_slot_from_path,
        load_frame: itg_slot_from_path_with_frame,
        load_animated: itg_slot_from_path_animated,
        load_all_frames: itg_slot_from_path_all_frames,
        apply_model: apply_model_slot_plan,
        apply_model_draw: itg_apply_model_draw,
        apply_parent_command: itg_apply_parent_command,
        apply_rotation: itg_apply_rotation,
        apply_frame: itg_apply_frame_override,
        apply_state: itg_apply_state_properties_from_commands,
        apply_loader_command: itg_apply_loader_command,
        apply_active_cmd: itg_slot_with_active_cmd,
        mine_fill_slots,
        base_zoom: itg_slot_base_zoom,
        model_info: itg_slot_model_info,
        texture_key: itg_slot_texture_key,
    }
}

fn itg_apply_model_draw(
    slot: &mut SpriteSlot,
    draw: ModelDrawState,
    timeline: Arc<[ModelTweenSegment]>,
    effect: ModelEffectState,
) {
    slot.model_draw = draw;
    slot.model_timeline = timeline;
    slot.model_effect = effect;
}

fn itg_apply_parent_command(slot: &mut SpriteSlot, command: &str) {
    noteskin_script::itg_apply_parent_command(&mut slot.def, &mut slot.model_draw, command);
}

fn itg_apply_rotation(slot: &mut SpriteSlot, rotation_z: i32) {
    slot.set_rotation_deg(rotation_z);
}

fn itg_slot_base_zoom(slot: &SpriteSlot) -> f32 {
    slot.model_draw.zoom[0]
}

fn itg_slot_model_info(slot: &SpriteSlot) -> (bool, [f32; 2]) {
    (slot.model.is_some(), slot.uv_velocity)
}

fn itg_slot_texture_key(slot: &SpriteSlot) -> String {
    slot.texture_key().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        AnimationRate, ModelEffectClock, ModelEffectMode, NUM_QUANTIZATIONS, NoteAnimPart,
        NoteColorType, Quantization, SpriteSlot, SpriteSource, Style, clear_itg_runtime_caches,
        itg_apply_state_properties_from_script, itg_register_texture_dims_for_path, load_itg,
        load_itg_model_slots_from_path, load_itg_skin, noteskin_itg,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_noteskin_root(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-mod-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_noteskin_png(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        image::RgbaImage::from_pixel(64, 64, image::Rgba([255, 255, 255, 255]))
            .save(path)
            .unwrap();
        itg_register_texture_dims_for_path(path);
    }

    fn load_fixture_itg_skin(
        style: &Style,
        skin: &str,
        textures: &[&str],
    ) -> (super::Noteskin, PathBuf) {
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/noteskins/dance")
            .join(skin);
        let root = temp_noteskin_root(skin);
        let target = root.join("dance").join(skin);
        fs::create_dir_all(&target).unwrap();
        for entry in fs::read_dir(&source).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
        }
        for texture in textures {
            write_noteskin_png(&target.join(texture));
        }
        let noteskin = load_itg(&root, "dance", skin, style)
            .unwrap_or_else(|err| panic!("fixture dance/{skin} should load: {err}"));
        (noteskin, root)
    }

    #[test]
    fn loads_default_and_cel_itg_noteskins() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        assert!(load_itg_skin(&style, "default").is_ok());
        assert!(load_itg_skin(&style, "cel").is_ok());
    }

    #[test]
    fn cel_exposes_model_and_uv_motion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(!ns.notes.is_empty());
        assert!(ns.notes.iter().any(|slot| slot.model.is_some()));
        assert!(ns.notes.iter().any(|slot| {
            slot.uv_velocity[0].abs() > f32::EPSILON || slot.uv_velocity[1].abs() > f32::EPSILON
        }));
    }

    #[test]
    fn shared_background_arrow_model_loads_with_texture_scroll() {
        let slots = load_itg_model_slots_from_path(Path::new(
            "assets/graphics/menu_bg_technique/arrow_model.txt",
        ))
        .expect("technique arrow model should load");
        assert_eq!(slots.len(), 1, "expected one arrow model layer");
        let slot = &slots[0];
        assert!(
            slot.model.is_some(),
            "shared model slot should contain geometry"
        );
        assert_eq!(
            slot.texture_key(),
            "graphics/menu_bg_technique/arrow_tex.png"
        );
        assert!(
            slot.uv_velocity[1] < -0.9 && slot.uv_velocity[1] > -1.1,
            "expected AnimatedTexture TexVelocityY to carry through, got {:?}",
            slot.uv_velocity
        );
        assert_eq!(slot.uv_cycle_seconds, Some(10.0));
    }

    #[test]
    fn shared_background_arrow_model_uv_scroll_uses_animation_cycle() {
        let slots = load_itg_model_slots_from_path(Path::new(
            "assets/graphics/menu_bg_technique/arrow_model.txt",
        ))
        .expect("technique arrow model should load");
        let slot = &slots[0];
        let uv_0 = slot.uv_for_frame_at(0, 0.0);
        let uv_5 = slot.uv_for_frame_at(0, 5.0);
        let uv_10 = slot.uv_for_frame_at(0, 10.0);
        assert!(
            (uv_5[1] - (uv_0[1] - 0.5)).abs() <= 1e-6 && (uv_5[3] - (uv_0[3] - 0.5)).abs() <= 1e-6,
            "expected half-cycle UV shift after 5 seconds, got {uv_0:?} -> {uv_5:?}"
        );
        assert!(
            (uv_10[1] - uv_0[1]).abs() <= 1e-6 && (uv_10[3] - uv_0[3]).abs() <= 1e-6,
            "expected UVs to wrap after one 10-second cycle, got {uv_0:?} -> {uv_10:?}"
        );
    }

    #[test]
    fn cel_model_tap_note_uses_multiple_material_layers() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let model_layers = layers
            .iter()
            .filter(|slot| slot.model.is_some())
            .collect::<Vec<_>>();
        assert!(
            model_layers.len() >= 2,
            "expected cel tap note model to expose multiple material layers; got {}",
            model_layers.len()
        );
        let textures = model_layers
            .iter()
            .map(|slot| slot.texture_key().to_string())
            .collect::<HashSet<_>>();
        assert!(
            textures.contains("noteskins/dance/cel/textures/Tap Note parts (mipmaps).png"),
            "expected cel model tap note layers to resolve Tap Note parts texture; got {:?}",
            textures
        );
    }

    #[test]
    fn cel_model_tap_note_honors_nomove_material() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("cel should expose at least one tap note layer set");
        let mut saw_model = false;
        let mut saw_moving = false;
        let mut saw_nomove = false;
        for layer in layers.iter().filter(|slot| slot.model.is_some()) {
            saw_model = true;
            let moving = layer.uv_velocity[0].abs() > f32::EPSILON
                || layer.uv_velocity[1].abs() > f32::EPSILON;
            if moving {
                saw_moving = true;
            } else {
                saw_nomove = true;
            }
        }
        assert!(
            saw_model,
            "expected at least one model-backed tap-note layer"
        );
        assert!(
            saw_moving,
            "expected at least one scrolling model material in cel tap note"
        );
        assert!(
            saw_nomove,
            "expected at least one nomove model material in cel tap note"
        );
    }

    #[test]
    fn default_exposes_multi_layer_tap_note() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert_eq!(ns.notes.len(), ns.note_layers.len());
        assert!(ns.note_layers.iter().any(|layers| layers.len() > 1));
        let q4_layers = ns
            .note_layers
            .first()
            .expect("default should expose 4th-note tap layers");
        assert_eq!(
            q4_layers.len(),
            5,
            "default tap note should have arrow + four circles"
        );
        let circle_layers = q4_layers
            .iter()
            .filter(|slot| slot.texture_key().to_ascii_lowercase().contains("_circle"))
            .count();
        assert_eq!(
            circle_layers, 4,
            "default tap note should keep four circle layers"
        );
    }

    #[test]
    fn default_exposes_lift_layers_for_each_quantization() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert_eq!(ns.lift_note_layers.len(), ns.note_layers.len());
        assert!(ns.lift_note_layers.iter().all(|layers| !layers.is_empty()));
    }

    #[test]
    fn lambda_tap_note_uses_source_size_hints() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "lambda")
            .expect("dance/lambda should load from assets/noteskins");
        let q4_layers = ns
            .note_layers
            .first()
            .expect("lambda should expose 4th-note tap layers");
        assert_eq!(
            q4_layers.len(),
            5,
            "lambda tap note should have arrow + four circles"
        );
        let arrow = q4_layers
            .first()
            .expect("lambda should expose primary arrow layer");
        assert_eq!(
            arrow.logical_size(),
            [64.0, 64.0],
            "lambda arrow logical size should use '(res 64x512)' source frame dimensions"
        );
        let circle = q4_layers
            .iter()
            .find(|slot| slot.texture_key().to_ascii_lowercase().contains("_circle"))
            .expect("lambda should expose circle layers");
        assert_eq!(
            circle.logical_size(),
            [16.0, 16.0],
            "lambda circle logical size should honor '(doubleres)' source dimensions"
        );
    }

    #[test]
    fn default_receptor_overlay_press_and_lift_behavior_is_parsed() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let behavior = ns.receptor_glow_behavior;

        assert!(
            (behavior.press_duration - 0.2).abs() <= 1e-6,
            "default press command duration should be 0.2s"
        );
        assert!(
            (behavior.press_alpha_start - 0.8).abs() <= 1e-6,
            "default press command should start overlay alpha at 0.8"
        );
        assert!(
            (behavior.press_alpha_end - 0.4).abs() <= 1e-6,
            "default press command should settle overlay alpha at 0.4 while held"
        );
        assert!(
            (behavior.duration - 0.2).abs() <= 1e-6,
            "default lift command duration should be 0.2s"
        );
        assert!(
            behavior.alpha_end.abs() <= 1e-6,
            "default lift command should fade overlay alpha to 0"
        );

        let (press_start_alpha, press_start_zoom) = behavior.sample_press(behavior.press_duration);
        let (press_end_alpha, press_end_zoom) = behavior.sample_press(0.0);
        assert!((press_start_alpha - behavior.press_alpha_start).abs() <= 1e-6);
        assert!((press_end_alpha - behavior.press_alpha_end).abs() <= 1e-6);
        assert!((press_start_zoom - behavior.press_zoom_start).abs() <= 1e-6);
        assert!((press_end_zoom - behavior.press_zoom_end).abs() <= 1e-6);

        let (lift_start_alpha, lift_start_zoom) = behavior.sample_lift(
            behavior.duration,
            behavior.press_alpha_end,
            behavior.press_zoom_end,
        );
        let (lift_end_alpha, lift_end_zoom) =
            behavior.sample_lift(0.0, behavior.press_alpha_end, behavior.press_zoom_end);
        assert!((lift_start_alpha - behavior.press_alpha_end).abs() <= 1e-6);
        assert!((lift_start_zoom - behavior.press_zoom_end).abs() <= 1e-6);
        assert!((lift_end_alpha - behavior.alpha_end).abs() <= 1e-6);
        assert!((lift_end_zoom - behavior.zoom_end).abs() <= 1e-6);
    }

    #[test]
    fn default_receptor_overlay_keeps_source_size_ratio() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");

        let receptor = ns
            .receptor_off
            .first()
            .expect("dance/default should resolve receptor sprite");
        let overlay = ns
            .receptor_glow
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("dance/default should resolve receptor overlay sprite");

        assert_eq!(
            receptor.logical_size(),
            [64.0, 64.0],
            "default receptor should use logical source-frame size"
        );
        assert_eq!(
            overlay.logical_size(),
            [74.0, 74.0],
            "default overlay should preserve larger source-frame size than receptor"
        );
    }

    #[test]
    fn howdy_receptor_none_command_keeps_init_zoom_static() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let (ns, root) =
            load_fixture_itg_skin(&style, "howdy", &["Down Tap Note.png", "Down Receptor.png"]);
        let receptor = ns
            .receptor_off
            .first()
            .expect("dance/howdy should resolve receptor sprite");
        assert!(
            (receptor.model_draw.zoom[0] - 0.8).abs() <= 1e-6,
            "howdy receptor InitCommand should set base zoom to 0.8"
        );

        let behavior = ns.receptor_step_behavior_for_col(0, None);
        assert_eq!(behavior.duration, 0.0);
        assert!(
            (behavior.sample_zoom(0.8) - 1.0).abs() <= 1e-6,
            "howdy constant-size NoneCommand should not start a shrink/return pulse"
        );

        clear_itg_runtime_caches();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn command_stack_receptor_none_command_drives_press_pulse() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let (ns, root) = load_fixture_itg_skin(
            &style,
            "command-stack",
            &["Down Tap Note.png", "Down Receptor.png"],
        );
        let behavior = ns.receptor_step_behavior_for_col(0, None);

        assert!((behavior.duration - 0.11).abs() <= 1e-6);
        assert!((behavior.sample_zoom(behavior.duration) - 0.75).abs() <= 1e-6);
        assert!((behavior.sample_zoom(0.0) - 1.0).abs() <= 1e-6);

        clear_itg_runtime_caches();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn devcel_receptor_hit_commands_do_not_use_none_zoom_pulse() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "devcel-2024")
            .expect("dance/devcel-2024 should load from assets/noteskins");
        let none = ns.receptor_step_behavior_for_col(0, None);
        let w1 = ns.receptor_step_behavior_for_col(0, Some("W1"));
        let w3 = ns.receptor_step_behavior_for_col(0, Some("W3"));

        assert!(none.duration > 0.0);
        assert!((none.sample_zoom(none.duration) - 0.75).abs() <= 1e-6);
        assert_eq!(w1.duration, 0.0);
        assert_eq!(w3.duration, 0.0);
        assert!(w1.interrupts);
        assert!(w3.interrupts);
        assert!((w1.sample_zoom(0.0) - 1.0).abs() <= 1e-6);
        assert!((w3.sample_zoom(0.0) - 1.0).abs() <= 1e-6);

        clear_itg_runtime_caches();
    }

    fn assert_devcel_roll_active_sequence(slot: &SpriteSlot, label: &str) {
        let SpriteSource::Animated {
            frame_count,
            frame_indices,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("{label} should be animated");
        };

        assert_eq!(*frame_count, 6, "{label} should keep every ITG state");
        assert_eq!(
            frame_indices.as_deref(),
            Some([0, 1, 2, 3, 2, 1].as_slice()),
            "{label} should preserve repeated texture frames"
        );
        let delays = frame_durations
            .as_deref()
            .unwrap_or_else(|| panic!("{label} should preserve state delays"));
        assert_eq!(delays, [0.44, 0.03, 0.03, 0.44, 0.03, 0.03]);
        assert!((delays.iter().sum::<f32>() - 1.0).abs() <= 1e-6);
        assert_eq!(slot.frame_index_from_phase(0.955), 4);
        assert_eq!(slot.uv_for_frame_at(2, 0.0), slot.uv_for_frame_at(4, 0.0));
        assert_eq!(slot.uv_for_frame_at(1, 0.0), slot.uv_for_frame_at(5, 0.0));
    }

    #[test]
    fn devcel_roll_active_preserves_repeated_frame_states() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "devcel-2024")
            .expect("dance/devcel-2024 should load from assets/noteskins");
        let body = ns
            .roll
            .body_active
            .as_ref()
            .expect("devcel roll body active should resolve");
        let bottom = ns
            .roll
            .bottomcap_active
            .as_ref()
            .expect("devcel roll bottomcap active should resolve");

        assert_devcel_roll_active_sequence(body, "devcel roll body active");
        assert_devcel_roll_active_sequence(bottom, "devcel roll bottomcap active");

        clear_itg_runtime_caches();
    }

    #[test]
    fn receptor_pulse_uses_actor_init_command_not_fallback_metric() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("receptor-init-command");
        let skin_dir = root.join("dance/steady");
        let common_dir = root.join("common/common");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[ReceptorArrow]\nNoneCommand=\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    return LoadActor(NOTESKIN:GetPath(button, Var "Element"))
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"local t = Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "go receptor");
        Frame0000=0;
        Delay0000=0;
        NoneCommand=NOTESKIN:GetMetricA("ReceptorArrow", "NoneCommand");
    };
};
return t
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n[ReceptorArrow]\nInitCommand=effectclock,'beat';diffuseramp;effectcolor1,color(\"0,0,0,1\");effectcolor2,color(\"1,1,1,1\");effecttiming,.5,0,.5,0\n",
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("_down go receptor.png"));

        let data = noteskin_itg::load_noteskin_data_cached(&root, "dance", "steady")
            .expect("steady test noteskin data should load");
        assert!(
            data.metrics
                .get("ReceptorArrow", "InitCommand")
                .is_some_and(|cmd| cmd.contains("diffuseramp")),
            "test skin should inherit a pulsing fallback metric"
        );

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "steady", &style).expect("steady test noteskin should load");
        let receptor = ns
            .receptor_off
            .first()
            .expect("steady test noteskin should resolve a receptor");
        assert_eq!(
            receptor.source.frame_count(),
            1,
            "Frame0000-only receptor actor should stay on a single frame"
        );
        for beat in [0.0, 0.25, 0.5, 0.75] {
            let color = ns.receptor_pulse.color_for_beat(beat);
            assert!(
                color.iter().all(|channel| (*channel - 1.0).abs() <= 1e-6),
                "receptor pulse should ignore fallback InitCommand at beat {beat}, got {color:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn loader_init_command_applies_to_resolved_receptor() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("loader-init-command");
        let skin_dir = root.join("dance/mirror");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=mirror\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    if element == "Receptor" and button == "Right" then
        local t = LoadActor(NOTESKIN:GetPath("Left", "Receptor"))
        t.InitCommand=function(self) self:SetTextureFiltering(false); self:y(1); self:zoomx(-1); end
        return t
    end
    if element == "Receptor" and button == "Left" then
        return LoadActor(NOTESKIN:GetPath("Left", "Receptor"))
    end
    return LoadActor(NOTESKIN:GetPath("Down", element))
end

return skin
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&skin_dir.join("Left Receptor.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "mirror", &style).expect("mirror test noteskin should load");
        let left = ns
            .receptor_off
            .first()
            .expect("left receptor should resolve");
        let right = ns
            .receptor_off
            .get(3)
            .expect("right receptor should resolve");

        assert!(!left.def.mirror_h);
        assert!(right.def.mirror_h);
        assert!((right.model_draw.pos[1] - 1.0).abs() <= f32::EPSILON);
        let uv = right.uv_for_frame_at(0, 0.0);
        assert!(
            uv[0] < uv[2],
            "mirroring stays as actor scale, not a reversed UV rect; got {uv:?}"
        );

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn explosion_children_keep_per_button_rotation() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("explosion-child-rotation");
        let skin_dir = root.join("dance/ghostrot");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=ghostrot\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }
skin.PartsToRotate = { ["Tap Explosion Dim W1"] = true, ["Hold Explosion"] = true }
skin.Rotate = { Up = 180, Down = 0, Left = 90, Right = -90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local path = element == "Explosion" and NOTESKIN:GetPath("", "Fallback Explosion") or NOTESKIN:GetPath(load_button, element)
    local t = LoadActor(path)
    if skin.PartsToRotate[element] then
        t.BaseRotationZ = skin.Rotate[button]
    end
    return t
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Fallback Explosion.lua"),
            r#"return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "Tap Explosion Dim W1") .. {
        InitCommand=cmd(diffusealpha,0);
        W1Command=cmd(diffusealpha,1);
        JudgmentCommand=cmd(finishtweening);
        DimCommand=cmd(visible,true);
    };
    NOTESKIN:LoadActor(Var "Button", "Hold Explosion") .. {
        InitCommand=cmd(diffusealpha,0);
        HoldingOnCommand=cmd(diffusealpha,1);
    };
    NOTESKIN:LoadActor(Var "Button", "Hold Explosion") .. {
        InitCommand=cmd(diffusealpha,0);
        RollOnCommand=cmd(diffusealpha,1);
    };
}"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&skin_dir.join("Down Tap Explosion Dim W1.png"));
        write_noteskin_png(&skin_dir.join("Down Hold Explosion.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "ghostrot", &style).expect("test noteskin should load");
        let rotations = (0..4)
            .map(|col| {
                ns.tap_explosion_for_col(col, "W1")
                    .expect("W1 explosion should resolve for each column")
                    .slot
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(rotations, vec![90, 0, 180, -90]);
        let hold_rotations = (0..4)
            .map(|col| {
                ns.hold_visuals_for_col(col, false)
                    .explosion
                    .as_ref()
                    .expect("hold explosion should resolve for each column")
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(hold_rotations, vec![90, 0, 180, -90]);
        let roll_rotations = (0..4)
            .map(|col| {
                ns.hold_visuals_for_col(col, true)
                    .explosion
                    .as_ref()
                    .expect("roll explosion should resolve for each column")
                    .def
                    .rotation_deg
            })
            .collect::<Vec<_>>();
        assert_eq!(roll_rotations, vec![90, 0, 180, -90]);

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn child_flash_hold_emitter_does_not_fall_back_to_static_explosion() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("hold-child-flash-emitter");
        let skin_dir = root.join("dance/flashhold");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=flashhold\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    local element = Var "Element"
    local path = element == "Explosion" and NOTESKIN:GetPath("", "Fallback Explosion") or NOTESKIN:GetPath(button, element)
    return LoadActor(path)
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Fallback Explosion.lua"),
r#"local holdflash = NOTESKIN:LoadActor(Var "Button", "Flash Dim") .. {
    InitCommand=function(self) self:blend(Blend.Add):diffuse(0,0,0,0) end;
    FlashCommand=function(self) self:diffuse(1,1,1,1):linear(0.05):diffuse(1,1,1,0) end;
}

return Def.ActorFrame {
    Def.ActorFrame {
        HoldingOnCommand=function(self) self.emitting=true; self:finishtweening():playcommand("Emit") end;
        HoldingOffCommand=function(self) self.emitting=false end;
        RollOnCommand=function(self) self.emitting=true; self:finishtweening():playcommand("Emit") end;
        RollOffCommand=function(self) self.emitting=false end;
        EmitCommand=function(self) self:queuecommand("Emit") end;
        holdflash;
    };
}"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&skin_dir.join("Down Flash Dim.png"));
        write_noteskin_png(&skin_dir.join("Down Hold Explosion.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "flashhold", &style).expect("test noteskin should load");
        for col in 0..4 {
            assert!(
                ns.hold_visuals_for_col(col, false).explosion.is_none(),
                "hold column {col} should not use a static fallback for child FlashCommand emitters"
            );
            assert!(
                ns.hold_visuals_for_col(col, true).explosion.is_none(),
                "roll column {col} should not use a static fallback for child FlashCommand emitters"
            );
        }

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn receptor_reverse_commands_are_kept_per_layer() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("receptor-reverse-command");
        let skin_dir = root.join("dance/revbar");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=revbar\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    return LoadActor(NOTESKIN:GetPath(button, Var "Element"))
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"local t = Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "go receptor");
        Frame0000=0;
        Delay0000=0;
        ReverseOnCommand=function(self)
            self:baserotationz(180)
        end;
        ReverseOffCommand=function(self)
            self:baserotationz(0)
        end;
    };
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_down", "tap flash");
        Frame0000=0;
        Delay0000=1;
        ReverseOnCommand=function(self)
            self:baserotationz(180):vertalign("bottom")
        end;
        ReverseOffCommand=function(self)
            self:baserotationz(0):vertalign("top")
        end;
    };
};
return t
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("_down go receptor.png"));
        write_noteskin_png(&skin_dir.join("_down tap flash.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg(&root, "dance", "revbar", &style).expect("revbar test noteskin should load");
        let off = ns
            .receptor_off_reverse
            .first()
            .copied()
            .expect("revbar should keep receptor reverse commands");
        assert_eq!(off.state(false).base_rotation_z, Some(0.0));
        assert_eq!(off.state(true).base_rotation_z, Some(180.0));

        let glow = ns
            .receptor_glow_reverse
            .first()
            .copied()
            .expect("revbar should keep receptor glow reverse commands");
        assert_eq!(glow.state(false).base_rotation_z, Some(0.0));
        assert_eq!(glow.state(false).vert_align, Some(0.0));
        assert_eq!(glow.state(true).base_rotation_z, Some(180.0));
        assert_eq!(glow.state(true).vert_align, Some(1.0));

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn default_and_cel_parse_notedisplay_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            default_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(
            default_ns
                .note_display_metrics
                .tap_hold_roll_on_row_means_hold
        );
        assert!(
            default_ns
                .note_display_metrics
                .flip_head_and_tail_when_reverse
        );
        assert!(default_ns.note_display_metrics.flip_hold_body_when_reverse);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_hold_head_for_taps_on_same_row
        );
        assert!(
            !cel_ns
                .note_display_metrics
                .draw_roll_head_for_taps_on_same_row
        );
        assert!(cel_ns.note_display_metrics.flip_head_and_tail_when_reverse);
        assert!(cel_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(cel_ns.note_display_metrics.top_hold_anchor_when_reverse);
    }

    #[test]
    fn ddr_note_and_cel_keep_distinct_reverse_hold_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ddr_note_ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        assert!(
            !ddr_note_ns
                .note_display_metrics
                .flip_head_and_tail_when_reverse
        );
        assert!(!ddr_note_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(
            !ddr_note_ns
                .note_display_metrics
                .top_hold_anchor_when_reverse
        );

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(cel_ns.note_display_metrics.flip_head_and_tail_when_reverse);
        assert!(cel_ns.note_display_metrics.flip_hold_body_when_reverse);
        assert!(cel_ns.note_display_metrics.top_hold_anchor_when_reverse);
    }

    #[test]
    fn default_and_cel_parse_note_color_translation_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let default_tap =
            default_ns.note_display_metrics.part_texture_translate[NoteAnimPart::Tap as usize];
        assert_eq!(default_tap.note_color_count, 8);
        assert_eq!(default_tap.note_color_type, NoteColorType::Denominator);
        assert!((default_tap.note_color_spacing[1] - 0.125).abs() <= 1e-6);
        let default_tap_8th = default_ns.part_uv_translation(NoteAnimPart::Tap, 0.5, false);
        assert!(default_tap_8th[0].abs() <= f32::EPSILON);
        assert!((default_tap_8th[1] - 0.125).abs() <= 1e-6);

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let cel_roll_head =
            cel_ns.note_display_metrics.part_texture_translate[NoteAnimPart::RollHead as usize];
        assert_eq!(cel_roll_head.note_color_count, 8);
        assert_eq!(cel_roll_head.note_color_type, NoteColorType::Denominator);
        assert!((cel_roll_head.note_color_spacing[0] - 0.03125).abs() <= 1e-6);
        let cel_roll_head_8th = cel_ns.part_uv_translation(NoteAnimPart::RollHead, 0.5, false);
        assert!((cel_roll_head_8th[0] - 0.03125).abs() <= 1e-6);
        assert!(cel_roll_head_8th[1].abs() <= f32::EPSILON);
    }

    #[test]
    fn default_and_cel_resolve_hold_topcaps() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let default_ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let default_visuals = default_ns.hold_visuals_for_col(0, false);
        assert!(
            default_visuals.topcap_inactive.is_none() && default_visuals.topcap_active.is_none(),
            "dance/default should honor ret.Blank and keep hold topcap visuals unresolved"
        );
        assert!(
            default_visuals.bottomcap_inactive.is_some()
                || default_visuals.bottomcap_active.is_some(),
            "dance/default should resolve hold bottomcap visuals"
        );

        let cel_ns =
            load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let cel_visuals = cel_ns.hold_visuals_for_col(0, false);
        assert!(
            cel_visuals.topcap_inactive.is_none() && cel_visuals.topcap_active.is_none(),
            "dance/cel should honor ret.Blank and keep hold topcap visuals unresolved"
        );
        assert!(
            cel_visuals.bottomcap_inactive.is_some() || cel_visuals.bottomcap_active.is_some(),
            "dance/cel should still resolve hold bottomcap visuals"
        );
    }

    #[test]
    fn default_does_not_bake_quantization_uv_shift_into_slots() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.first())
            .expect("default should expose first 4th-note layer");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.first())
            .expect("default should expose first 8th-note layer");
        assert_eq!(q4.def.src, q8.def.src);
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON
                && (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON
        );
    }

    #[test]
    fn ddr_vivid_parses_hold_body_offsets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        assert!(
            (ns.note_display_metrics
                .start_drawing_hold_body_offset_from_head
                - 0.0)
                .abs()
                <= f32::EPSILON
        );
        assert!(
            (ns.note_display_metrics
                .stop_drawing_hold_body_offset_from_tail
                + 32.0)
                .abs()
                <= 1e-6
        );
        assert!((ns.note_display_metrics.hold_let_go_gray_percent - 0.33).abs() <= 1e-6);
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].length - 4.0)
                .abs()
                <= 1e-6
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].length - 2.0)
                .abs()
                <= 1e-6
        );
        assert!(
            !ns.note_display_metrics.part_animation[NoteAnimPart::HoldBody as usize].vivid
                && !ns.note_display_metrics.part_animation[NoteAnimPart::RollBody as usize].vivid
        );
    }

    #[test]
    fn vivid_zero_spacing_keeps_model_uv_offsets_across_quants() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let q4 = ns
            .note_layers
            .first()
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 4th notes");
        let q8 = ns
            .note_layers
            .get(1)
            .and_then(|layers| layers.iter().find(|slot| slot.model.is_some()))
            .expect("vivid should expose model-backed tap note layer for 8th notes");
        assert!(
            (q4.uv_offset[0] - q8.uv_offset[0]).abs() <= f32::EPSILON,
            "vivid should not force note-color X offset when spacing metrics are zero"
        );
        assert!(
            (q4.uv_offset[1] - q8.uv_offset[1]).abs() <= f32::EPSILON,
            "vivid should not force note-color Y offset when spacing metrics are zero"
        );
    }

    #[test]
    fn vivid_tap_note_honors_vertex_tex_matrix_scale_flags() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns =
            load_itg_skin(&style, "vivid").expect("dance/vivid should load from assets/noteskins");
        let layers = ns
            .note_layers
            .first()
            .expect("vivid should expose at least one tap note layer set");
        let mut saw_static_uv_vertex = false;
        let mut saw_scrolling_uv_vertex = false;
        for layer in layers.iter().filter_map(|slot| slot.model.as_ref()) {
            for vertex in layer.vertices.iter() {
                let sx = vertex.tex_matrix_scale[0];
                let sy = vertex.tex_matrix_scale[1];
                if sx < 0.5 || sy < 0.5 {
                    saw_static_uv_vertex = true;
                } else {
                    saw_scrolling_uv_vertex = true;
                }
            }
        }
        assert!(
            saw_static_uv_vertex,
            "vivid tap note should include vertices that ignore texture-matrix scroll"
        );
        assert!(
            saw_scrolling_uv_vertex,
            "vivid tap note should include vertices that follow texture-matrix scroll"
        );
    }

    #[test]
    fn ddr_note_receptor_uses_beat_clock_with_mixed_delays() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let slot = ns
            .receptor_off
            .first()
            .expect("ddr-note should define receptor_off for first column");
        let SpriteSource::Animated {
            rate,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("ddr-note receptor should resolve to animated sprite");
        };
        assert!(
            matches!(rate, AnimationRate::FramesPerBeat(_)),
            "ddr-note receptor expected beat clock animation, got {rate:?}"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-note receptor should preserve per-frame delays");
        assert!(
            delays.len() >= 2,
            "expected at least 2 receptor delays, got {:?}",
            delays
        );
        assert!(
            (delays[0] - 0.2).abs() < 0.01,
            "expected first frame delay near 0.2 beat, got {}",
            delays[0]
        );
        assert!(
            (delays[1] - 0.8).abs() < 0.01,
            "expected second frame delay near 0.8 beat, got {}",
            delays[1]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.19), 0);
        assert_eq!(slot.frame_index(0.0, 0.25), 1);
        assert_eq!(slot.frame_index(0.0, 0.95), 1);
        assert_eq!(slot.frame_index(0.0, 1.05), 0);
    }

    #[test]
    fn ddr_note_receptor_phase_index_uses_weighted_delays() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let slot = ns
            .receptor_off
            .first()
            .expect("ddr-note should define receptor_off for first column");
        assert_eq!(slot.frame_index_from_phase(0.00), 0);
        assert_eq!(slot.frame_index_from_phase(0.19), 0);
        assert_eq!(slot.frame_index_from_phase(0.20), 1);
        assert_eq!(slot.frame_index_from_phase(0.95), 1);
        assert_eq!(slot.frame_index_from_phase(1.05), 0);
        assert_eq!(slot.frame_index_from_phase(-0.05), 1);
    }

    #[test]
    fn ddr_note_hold_body_and_cap_use_per_column_assets() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        let expected = [
            ("left hold body inactive", "left hold bottomcap inactive"),
            ("down hold body inactive", "down hold bottomcap inactive"),
            ("up hold body inactive", "up hold bottomcap inactive"),
            ("right hold body inactive", "right hold bottomcap inactive"),
        ];

        for (col, (want_body, want_cap)) in expected.into_iter().enumerate() {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold body inactive per column");
            let cap = visuals
                .bottomcap_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold bottomcap inactive per column");
            assert!(
                body.contains(want_body),
                "column {col} expected body containing '{want_body}', got '{body}'"
            );
            assert!(
                cap.contains(want_cap),
                "column {col} expected cap containing '{want_cap}', got '{cap}'"
            );
        }
    }

    #[test]
    fn ddr_note_hold_head_uses_down_hold_head_sheet() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");

        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let inactive = visuals
                .head_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head inactive");
            let active = visuals
                .head_active
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("ddr-note should provide hold head active");
            assert!(
                inactive.contains("down hold head inactive"),
                "column {col} expected Down hold head inactive sheet, got '{inactive}'"
            );
            assert!(
                active.contains("down hold head active"),
                "column {col} expected Down hold head active sheet, got '{active}'"
            );
        }
    }

    #[test]
    fn multi_layer_hold_heads_keep_model_layers() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("multi-layer-hold-head");
        let skin_dir = root.join("dance/multilayer");
        fs::create_dir_all(skin_dir.join("textures")).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=multilayer\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local ret = ... or {}
ret.Redir = function(sButton, sElement)
    return "Down", sElement
end
ret.Load = function()
    local button, element = ret.Redir(Var "Button", Var "Element")
    return LoadActor(NOTESKIN:GetPath(button, element))
end
return ret
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Tap Note.lua"),
            r#"return Def.Model {
    Meshes=NOTESKIN:GetPath('_down','tap note model');
    Materials=NOTESKIN:GetPath('_down','tap note model');
    Bones=NOTESKIN:GetPath('_down','tap note model');
};
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Hold Head Inactive.lua"),
            r#"return Def.Model {
    Meshes=NOTESKIN:GetPath('_down','tap note model');
    Materials=NOTESKIN:GetPath('_down','tap note model');
    Bones=NOTESKIN:GetPath('_down','tap note model');
};
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("_down tap note model.txt"),
            r#"MilkShape 3D ASCII
Meshes: 2
"fill" 0 0
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
"frame" 0 1
3
0 -1.0 -1.0 0.0 0.0 0.0 -1
0 1.0 -1.0 0.0 1.0 0.0 -1
0 0.0 1.0 0.0 0.0 1.0 -1
0
1
0 0 1 2 0 0 0 1
Materials: 2
"fill_mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures/fill.png"
""
"frame_mat"
0.0 0.0 0.0 1.0
1.0 1.0 1.0 1.0
0.0 0.0 0.0 1.0
0.0 0.0 0.0 1.0
0.0
1.0
"textures/frame.png"
""
"#,
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("textures/fill.png"));
        write_noteskin_png(&skin_dir.join("textures/frame.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "multilayer", &style)
            .expect("temp multilayer noteskin should load");

        for col in 0..style.num_cols {
            let note_idx = col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            let tap_keys = ns.note_layers[note_idx]
                .iter()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .collect::<Vec<_>>();
            let visuals = ns.hold_visuals_for_col(col, false);
            let head_layers = visuals
                .head_inactive_layers
                .as_deref()
                .expect("hold heads should keep all model layers");
            let head_keys = head_layers
                .iter()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .collect::<Vec<_>>();

            assert_eq!(
                head_keys, tap_keys,
                "column {col} hold head should use the full tap-note model layer stack"
            );
            assert!(
                head_keys.iter().any(|key| key.contains("fill.png")),
                "column {col} hold head is missing the fill layer: {head_keys:?}"
            );
            assert!(
                head_keys.iter().any(|key| key.contains("frame.png")),
                "column {col} hold head is missing the frame layer: {head_keys:?}"
            );
        }

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn default_skin_blanks_hold_and_roll_explosion() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        assert!(
            ns.hold.explosion.is_none(),
            "default hold explosion should stay blank per NoteSkin.lua"
        );
        assert!(
            ns.roll.explosion.is_none(),
            "default roll explosion should stay blank per NoteSkin.lua"
        );
        for col in 0..style.num_cols {
            let hold_visuals = ns.hold_visuals_for_col(col, false);
            let roll_visuals = ns.hold_visuals_for_col(col, true);
            assert!(
                hold_visuals.explosion.is_none(),
                "default hold visuals should not resolve explosion for col {col}"
            );
            assert!(
                roll_visuals.explosion.is_none(),
                "default roll visuals should not resolve explosion for col {col}"
            );
        }
    }

    #[test]
    fn default_mine_hit_explosion_comes_from_noteskin_actor_and_commands() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("default should resolve HitMine Explosion actor");
        let key = mine.slot.texture_key().to_ascii_lowercase();
        assert!(
            key.contains("noteskins/common/common/fallback hitmine explosion"),
            "default mine hit explosion should resolve to common fallback actor texture, got '{key}'"
        );
        assert!(
            (mine.animation.duration() - 0.6).abs() <= 1e-6,
            "default mine hit explosion should follow HitMineCommand duration (0.6s), got {}",
            mine.animation.duration()
        );
    }

    #[test]
    fn blank_tap_explosions_do_not_fall_back_to_common() {
        clear_itg_runtime_caches();
        let root = temp_noteskin_root("blank-tap-explosion");
        let skin_dir = root.join("dance/blanktap");
        let common_dir = root.join("common/common");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();
        fs::write(
            skin_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n",
        )
        .unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Up = "Down", Down = "Down", Left = "Down", Right = "Down" }
skin.ElementRedir = { ["Tap Explosion Dim"] = "Tap Explosion Bright" }
skin.Blank = { ["Tap Explosion Bright"] = true, ["Tap Explosion Dim"] = true }

function skin.Load()
    local button = skin.ButtonRedir[Var "Button"] or Var "Button"
    local element = skin.ElementRedir[Var "Element"] or Var "Element"
    local t = LoadActor(NOTESKIN:GetPath(button, element))
    if skin.Blank[Var "Element"] then
        t = Def.Actor {}
        if Var "SpriteOnly" then
            t = LoadActor(NOTESKIN:GetPath("", "_blank"))
        end
    end
    return t
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("metrics.ini"),
            "[Global]\nFallbackNoteSkin=common\n",
        )
        .unwrap();
        write_noteskin_png(&skin_dir.join("Down Tap Note.png"));
        write_noteskin_png(&skin_dir.join("Down Receptor.png"));
        write_noteskin_png(&common_dir.join("Fallback Tap Explosion Dim.png"));
        write_noteskin_png(&common_dir.join("Fallback Tap Explosion Bright.png"));

        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg(&root, "dance", "blanktap", &style)
            .expect("blanktap test noteskin should load");
        assert!(
            ns.tap_explosions.is_empty(),
            "blank tap explosions should not leak common fallback sprites: {:?}",
            ns.tap_explosions.keys().collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(&root);
        clear_itg_runtime_caches();
    }

    #[test]
    fn cel_hold_heads_remap_to_tap_layers() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            assert!(
                visuals.head_inactive.is_none() && visuals.head_active.is_none(),
                "cel hold heads should use tap-note fallback layers, got inactive={:?} active={:?}",
                visuals
                    .head_inactive
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string()),
                visuals
                    .head_active
                    .as_ref()
                    .map(|slot| slot.texture_key().to_string())
            );
        }
    }

    #[test]
    fn cel_hold_body_resolves_for_all_columns() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        for col in 0..style.num_cols {
            let visuals = ns.hold_visuals_for_col(col, false);
            let body = visuals
                .body_inactive
                .as_ref()
                .map(|slot| slot.texture_key().to_ascii_lowercase())
                .expect("cel should provide hold body inactive for each column");
            assert!(
                body.contains("down hold body inactive"),
                "column {col} expected down hold body inactive, got '{body}'"
            );
        }
    }

    #[test]
    fn enchantment_tap_note_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let idx = 2 * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
        let slot = ns
            .note_layers
            .get(idx)
            .and_then(|layers| layers.first())
            .expect("enchantment should expose first tap note layer for 4th quant");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = slot.source.as_ref()
        else {
            panic!("enchantment tap note should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 16,
            "enchantment tap note should use 16 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment tap note should preserve linear frame delays");
        assert_eq!(delays.len(), 16, "expected one delay per linear frame");
        assert!(
            (delays[0] - 0.0625).abs() < 1e-4,
            "expected linear frame delay 1/16 beat, got {}",
            delays[0]
        );
        assert_eq!(slot.frame_index(0.0, 0.00), 0);
        assert_eq!(slot.frame_index(0.0, 0.06), 0);
        assert_eq!(slot.frame_index(0.0, 0.07), 1);
        assert_eq!(slot.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn enchantment_tap_mine_uses_linear_frames_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "enchantment")
            .expect("dance/enchantment should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("enchantment should define first-column mine slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            ..
        } = mine.source.as_ref()
        else {
            panic!("enchantment mine should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 8,
            "enchantment mine should use 8 linear frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("enchantment mine should preserve linear frame delays");
        assert_eq!(delays.len(), 8, "expected one delay per mine frame");
        assert!(
            (delays[0] - 0.125).abs() < 1e-4,
            "expected linear frame delay 1/8 beat, got {}",
            delays[0]
        );
        assert_eq!(mine.frame_index(0.0, 0.00), 0);
        assert_eq!(mine.frame_index(0.0, 0.12), 0);
        assert_eq!(mine.frame_index(0.0, 0.13), 1);
        assert_eq!(mine.frame_index(0.0, 1.01), 0);
    }

    #[test]
    fn ddr_vivid_hold_explosion_uses_four_animated_frames() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-vivid")
            .expect("dance/ddr-vivid should load from assets/noteskins");
        let hold = ns
            .hold
            .explosion
            .as_ref()
            .expect("ddr-vivid should define hold explosion slot");
        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = hold.source.as_ref()
        else {
            panic!("ddr-vivid hold explosion should resolve to animated sprite");
        };
        assert_eq!(
            *frame_count, 4,
            "ddr-vivid hold explosion should use 4 frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("ddr-vivid hold explosion should preserve frame delays");
        assert_eq!(
            delays.len(),
            4,
            "expected one delay per hold explosion frame"
        );
        assert!(
            delays.iter().all(|delay| (*delay - 0.01).abs() < 1e-4),
            "expected all hold explosion frame delays to be 0.01, got {delays:?}"
        );
        assert_eq!(hold.frame_index(0.0, 0.0), 0);
        let advanced = match rate {
            AnimationRate::FramesPerSecond(_) => hold.frame_index(0.011, 0.0),
            AnimationRate::FramesPerBeat(_) => hold.frame_index(0.0, 0.011),
        };
        assert_eq!(
            advanced, 1,
            "ddr-vivid hold explosion should advance to frame 1 after one delay"
        );
    }

    #[test]
    fn setstateproperties_linear_frames_applies_to_synthetic_8x8_slot() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let mut slot = ns
            .mine_hit_explosion
            .as_ref()
            .expect("default should define mine hit explosion")
            .slot
            .clone();
        let key = "tests/fake/Fallback HitMine Explosion 8x8 (res 1536x1536).png".to_string();
        slot.source = Arc::new(SpriteSource::Atlas {
            texture_key: Arc::<str>::from(key.as_str()),
            tex_dims: (2048, 2048),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        });
        slot.model = None;
        let (cols, rows) = crate::sprite_sheet_dims(&key);
        let available = (cols.max(1) as usize).saturating_mul(rows.max(1) as usize);
        assert!(
            available > 1,
            "expected synthetic mine explosion texture to have multiple frames, got {available} for '{key}'"
        );

        itg_apply_state_properties_from_script(
            &mut slot,
            "setstateproperties,Sprite.LinearFrames(64,(64/60))",
            false,
        );

        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = slot.source.as_ref()
        else {
            panic!("setstateproperties should convert mine slot source to animated");
        };
        let expected_frames = available.min(64);
        assert_eq!(
            *frame_count, expected_frames,
            "setstateproperties should clamp frame count by available frames"
        );
        let delays = frame_durations
            .as_ref()
            .expect("setstateproperties should preserve linear frame delays");
        assert_eq!(
            delays.len(),
            expected_frames,
            "expected one delay per mine animation frame"
        );
        assert!(
            delays
                .iter()
                .all(|delay| (*delay - (1.0 / 60.0)).abs() < 1e-4),
            "expected setstateproperties delays to be 1/60s, got {delays:?}"
        );
        match rate {
            AnimationRate::FramesPerSecond(fps) => {
                assert!(
                    (fps - 60.0).abs() < 1e-3,
                    "expected setstateproperties mine animation to run at 60fps, got {fps}"
                );
            }
            AnimationRate::FramesPerBeat(v) => panic!("expected time-based animation, got {v} fpb"),
        }
        assert_eq!(slot.frame_index(0.0, 0.0), 0);
        assert_eq!(
            slot.frame_index((1.0 / 60.0) + 0.001, 0.0),
            1,
            "mine animation should advance after one frame delay"
        );
    }

    #[test]
    fn setallstatedelays_overrides_existing_sprite_animation() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "default")
            .expect("dance/default should load from assets/noteskins");
        let mut slot = ns
            .mine_hit_explosion
            .as_ref()
            .expect("default should define mine hit explosion")
            .slot
            .clone();
        let key = "tests/fake/Fallback HitMine Explosion 8x8 (res 1536x1536).png".to_string();
        slot.source = Arc::new(SpriteSource::Atlas {
            texture_key: Arc::<str>::from(key.as_str()),
            tex_dims: (2048, 2048),
            cached_handle: AtomicU64::new(deadlib_render::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
        });
        slot.model = None;

        itg_apply_state_properties_from_script(
            &mut slot,
            "setstateproperties,Sprite.LinearFrames(4,0.4);SetAllStateDelays,0.05",
            false,
        );

        let SpriteSource::Animated {
            frame_count,
            frame_durations,
            rate,
            ..
        } = slot.source.as_ref()
        else {
            panic!("setallstatedelays should keep the source animated");
        };
        assert_eq!(*frame_count, 4);
        let delays = frame_durations
            .as_ref()
            .expect("setallstatedelays should preserve explicit frame durations");
        assert_eq!(delays.as_ref(), [0.05, 0.05, 0.05, 0.05]);
        match rate {
            AnimationRate::FramesPerSecond(fps) => {
                assert!((*fps - 20.0).abs() < 1e-3, "expected 20fps, got {fps}");
            }
            AnimationRate::FramesPerBeat(v) => panic!("expected time-based animation, got {v} fpb"),
        }
        assert_eq!(slot.frame_index(0.0, 0.0), 0);
        assert_eq!(slot.frame_index(0.051, 0.0), 1);
    }

    #[test]
    fn cel_roll_glowshift_keeps_diffuse_and_uses_glow_channel() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let roll = ns
            .roll
            .explosion
            .as_ref()
            .expect("cel should define roll explosion");
        assert!(
            roll.texture_key()
                .to_ascii_lowercase()
                .contains("down hold explosion"),
            "cel roll explosion should resolve to down hold explosion texture"
        );

        let draw_0 = roll.model_draw_at(0.0, 0.0);
        let draw_1 = roll.model_draw_at(0.0125, 0.0);
        assert!(
            draw_0.visible && draw_1.visible,
            "roll explosion should be visible while active"
        );
        assert!(
            (draw_0.tint[3] - draw_1.tint[3]).abs() <= 1e-6,
            "glowshift should not modulate diffuse alpha"
        );

        let glow_alphas = [0.0f32, 0.0125, 0.025, 0.0375]
            .iter()
            .filter_map(|&t| {
                roll.model_glow_at(t, 0.0, draw_0.tint[3])
                    .map(|glow| glow[3])
            })
            .collect::<Vec<_>>();
        assert!(
            glow_alphas.len() >= 2,
            "glowshift should emit visible glow for at least part of its cycle"
        );
        let min_alpha = glow_alphas.iter().copied().fold(f32::INFINITY, f32::min);
        let max_alpha = glow_alphas
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            (max_alpha - min_alpha) > 0.05,
            "glow alpha should animate over time for glowshift; got {:?}",
            glow_alphas
        );
    }

    #[test]
    fn cel_w1_tap_explosion_resolves_dim_and_bright_paths() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let w1 = ns
            .tap_explosions
            .get("W1")
            .expect("cel should define W1 tap explosion");
        assert!(
            w1.animation.initial.visible,
            "cel W1 tap explosion should start visible"
        );
        assert!(
            w1.animation.initial.color[3] > 0.9,
            "cel W1 tap explosion should start from the dim W1 alpha path"
        );
        assert!(
            !w1.animation.blend_add,
            "cel W1 tap explosion should render with normal blend like ITG GhostArrow sprites"
        );
        assert!(
            w1.slot
                .texture_key()
                .to_ascii_lowercase()
                .contains("tap explosion dim w1"),
            "cel dim W1 tap explosion should use the dim W1 actor first"
        );

        let w1_bright = ns
            .tap_explosion_for_col_with_bright(0, "W1", true)
            .expect("cel should define bright W1 tap explosion");
        assert!(
            w1_bright
                .slot
                .texture_key()
                .to_ascii_lowercase()
                .contains("tap explosion bright w1"),
            "cel bright W1 tap explosion should use the bright W1 actor first"
        );
        assert!(
            w1_bright.animation.initial.color[3] > 0.9,
            "cel bright W1 tap explosion should start from the bright W1 alpha path"
        );

        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("cel should define hit-mine explosion");
        assert!(
            mine.animation.blend_add,
            "cel hit-mine explosion should keep additive blend from noteskin commands"
        );
    }

    #[test]
    fn command_stack_tap_explosions_keep_button_rotation() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let (ns, root) = load_fixture_itg_skin(
            &style,
            "command-stack",
            &[
                "Down Tap Note.png",
                "Down Receptor.png",
                "Down Flash.png",
                "Down Glow.png",
                "Down Spark.png",
                "Down Mine Emitter.png",
            ],
        );

        for window in ["W1", "W2", "W3", "W4", "W5"] {
            for (col, expected_rotation) in [90, 0, 180, -90].into_iter().enumerate() {
                let explosion = ns.tap_explosion_for_col(col, window).unwrap_or_else(|| {
                    panic!("{window} tap explosion should resolve for column {col}")
                });
                let mut rotated_child_count = 0usize;
                for layer in explosion.layers.iter() {
                    let key = layer.slot.texture_key().to_ascii_lowercase();
                    if key.contains("flash") || key.contains("glow") {
                        rotated_child_count += 1;
                        assert_eq!(
                            layer.slot.def.rotation_deg, expected_rotation,
                            "{window} column {col} should keep per-button rotation for {key}"
                        );
                    } else if key.contains("spark") {
                        assert_eq!(
                            layer.slot.def.rotation_deg, 0,
                            "{window} column {col} Spark should remain unrotated per PartsToRotate"
                        );
                    }
                    assert!(
                        !key.contains("tap explosion dim"),
                        "{window} column {col} should not replace the actor stack with direct Tap Explosion art"
                    );
                }
                assert!(
                    rotated_child_count > 0,
                    "{window} column {col} should keep at least one rotated Flash/Glow child"
                );
            }
        }

        clear_itg_runtime_caches();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn command_stack_mine_explosion_uses_emitter_commands_without_spin() {
        clear_itg_runtime_caches();
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let (ns, root) = load_fixture_itg_skin(
            &style,
            "command-stack",
            &[
                "Down Tap Note.png",
                "Down Receptor.png",
                "Down Flash.png",
                "Down Glow.png",
                "Down Spark.png",
                "Down Mine Emitter.png",
            ],
        );
        let mine = ns
            .mine_hit_explosion
            .as_ref()
            .expect("command-stack should define a hit-mine explosion");

        assert!(
            mine.layers.iter().any(|layer| !layer.animation.blend_add),
            "fixture mine explosion should keep the normal ECommand layer"
        );
        assert!(
            mine.layers.iter().any(|layer| layer.animation.blend_add),
            "fixture mine explosion should keep the additive E2Command layer"
        );
        assert!(
            (mine.duration() - 64.0 / 60.0).abs() <= 1e-6,
            "fixture mine explosion should use the emitter E/E2 duration, got {}",
            mine.duration()
        );
        for (idx, layer) in mine.layers.iter().enumerate() {
            assert!(
                layer.animation.initial.color[3] > 0.99,
                "mine layer {idx} should become visible when E/E2Command fires"
            );
            assert_eq!(
                layer.animation.initial.rotation_z, 0.0,
                "mine layer {idx} should not inherit the common rotating HitMineCommand"
            );
            assert!(
                layer
                    .animation
                    .segments
                    .iter()
                    .all(|segment| segment.end_rotation_z.is_none()),
                "mine layer {idx} should not animate rotation"
            );
        }

        clear_itg_runtime_caches();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cel_tap_mine_prefers_model_actor_over_texture_fallback() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            mine.model.is_some(),
            "cel mine should come from Tap Mine model actor, not _mine texture fallback"
        );
        assert!(
            ns.mine_frames.first().is_some_and(Option::is_none),
            "cel mine uses a single model actor and should not duplicate it as a frame layer"
        );
    }

    #[test]
    fn cel_tap_mine_uv_phase_uses_beat_clock_from_metrics() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        assert!(
            ns.animation_is_beat_based,
            "cel metrics use beat-based noteskin animation"
        );
        assert!(
            (ns.note_display_metrics.part_animation[NoteAnimPart::Mine as usize].length - 1.0)
                .abs()
                <= f32::EPSILON,
            "cel tap mine animation length should be 1 beat"
        );
        let phase = ns.tap_mine_uv_phase(0.5, 1.0, 0.0);
        assert!(
            phase <= 1e-6,
            "one beat should wrap tap mine phase to 0 for cel; got {phase}"
        );
    }

    #[test]
    fn cel_tap_mine_does_not_set_model_spin_effect() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::None),
            "cel mine should not set model spin effect via parser commands"
        );
    }

    #[test]
    fn cel_tap_mine_uses_milkshape_bone_rotation_timing() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "cel").expect("dance/cel should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("cel should define first-column mine slot");
        assert!(
            (mine.model_auto_rot_total_frames - 120.0).abs() <= f32::EPSILON,
            "cel mine should use milkshape total frame count for auto-rotation"
        );
        assert!(
            mine.model_auto_rot_z_keys.len() >= 2,
            "cel mine should expose at least two auto-rotation keys"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(1.0, 0.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta - 87.3).abs() <= 0.5,
            "cel mine should rotate by ~87.3 degrees after one second; got delta={delta}"
        );
    }

    #[test]
    fn lambda_tap_mine_spin_uses_beat_clock_and_magnitude() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "lambda")
            .expect("dance/lambda should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("lambda should define first-column mine slot");
        assert!(
            matches!(mine.model_effect.mode, ModelEffectMode::Spin),
            "lambda mine init command should enable spin effect"
        );
        assert!(
            matches!(mine.model_effect.clock, ModelEffectClock::Beat),
            "lambda mine spin should run on beat clock"
        );
        let rot_0 = mine.model_draw_at(0.0, 0.0).rot[2];
        let rot_1 = mine.model_draw_at(0.0, 1.0).rot[2];
        let delta = (rot_1 - rot_0 + 540.0).rem_euclid(360.0) - 180.0;
        assert!(
            (delta + 33.0).abs() <= 1e-3,
            "one beat should rotate lambda mine by -33 degrees; got delta={delta}"
        );
    }

    #[test]
    fn ddr_note_tap_mine_keeps_second_model_layer_as_frame() {
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        let ns = load_itg_skin(&style, "ddr-note")
            .expect("dance/ddr-note should load from assets/noteskins");
        let mine = ns
            .mines
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should define first-column mine slot");
        let frame = ns
            .mine_frames
            .first()
            .and_then(|slot| slot.as_ref())
            .expect("ddr-note should preserve second mine layer");
        assert!(
            mine.model.is_some(),
            "ddr-note mine fill should be model-backed"
        );
        assert!(
            frame.model.is_some(),
            "ddr-note mine frame should be model-backed second layer"
        );
    }
}
