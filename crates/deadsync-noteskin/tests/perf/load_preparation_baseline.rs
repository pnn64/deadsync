// Frozen from a85e01991 (0.5.1203); receivers adapted for the test module.
use deadsync_noteskin::{actor as noteskin_actor, compiled::*};
use log::warn;
use std::path::{Path, PathBuf};
pub(super) fn old_load_request(
    loader: &CompiledLoader,
    button: &str,
    element: &str,
) -> ItgLoadRequest {
    if let Some(entry) = loader.find(button, element) {
        return ItgLoadRequest {
            blank: entry.blank,
            load_button: entry.load_button.clone(),
            load_element: entry.load_element.clone(),
            rotation_x: entry.rotation_x,
            rotation_y: entry.rotation_y,
            rotation_z: entry.rotation_z,
            init_command: entry.init_command.clone(),
        };
    }
    warn!("compiled noteskin loader is missing '{button} {element}'");
    ItgLoadRequest {
        blank: false,
        load_button: button.to_string(),
        load_element: element.to_string(),
        rotation_x: None,
        rotation_y: None,
        rotation_z: None,
        init_command: None,
    }
}

pub(super) fn old_decl_for_path(
    actors: &CompiledActors,
    search_dirs: &[PathBuf],
    path: &Path,
) -> Option<noteskin_actor::ItgLuaActorDecl> {
    let key = actor_manifest_key(search_dirs, path)?;
    actors.find(&key).cloned().map(|file| file.decl)
}

use deadsync_noteskin::{actor, compiled, itg, runtime::itg_load_sprite_decl_slot};

pub(super) fn old_first_actor_sprite_slot<T>(
    data: &itg::NoteskinData,
    compiled_actors: &compiled::CompiledActors,
    path: &Path,
    mut load_texture: impl FnMut(&Path) -> Option<T>,
    mut load_frame: impl FnMut(&Path, usize) -> Option<T>,
    mut load_animated: impl FnMut(
        &Path,
        usize,
        usize,
        Option<&[usize]>,
        Option<&[f32]>,
        bool,
    ) -> Option<T>,
) -> Option<T> {
    if !actor::is_lua_path(path) {
        return load_texture(path);
    }

    let decl = old_decl_for_path(compiled_actors, &data.search_dirs, path)?;
    let default_anim_is_beat = itg::animation_is_beat_based(data);
    for sprite in decl.sprites {
        let slot = itg_load_sprite_decl_slot(
            data,
            &sprite,
            None,
            default_anim_is_beat,
            &mut load_texture,
            &mut load_frame,
            &mut load_animated,
        );
        if let Some(slot) = slot {
            return Some(slot);
        }
    }
    None
}
