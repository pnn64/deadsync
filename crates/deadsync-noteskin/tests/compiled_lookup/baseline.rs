// Frozen from 68ff515a3 (0.5.1649).
use super::*;

pub fn decl_for_path(
    actors: &CompiledActors,
    search_dirs: &[PathBuf],
    path: &Path,
    button: Option<&str>,
    color: Option<crate::Quantization>,
) -> Option<noteskin_actor::ItgLuaActorDecl> {
    let key = actor_manifest_key(search_dirs, path)?;
    if let Some(color) = color {
        let color = color.color_name();
        let file = button
            .and_then(|button| actors.find(&format!("{key}|{button}|color={color}")))
            .or_else(|| actors.find(&format!("{key}|color={color}")));
        if let Some(file) = file {
            return Some(file.decl.clone());
        }
    }
    if let Some(button) = button {
        if let Some(file) = actors.find(&format!("{key}|{button}")) {
            return Some(file.decl.clone());
        }
    }
    actors.find(&key).map(|file| file.decl.clone())
}
