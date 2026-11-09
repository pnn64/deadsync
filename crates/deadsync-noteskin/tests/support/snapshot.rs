use deadsync_noteskin::compiled::*;
use serde_json::{Value, json};
use std::path::Path;

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.into_iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        value => value,
    }
}

// Resource manifests use parent/basename keys rather than full paths.
// Register those exact derived path aliases alongside the temporary directory.
#[allow(dead_code)]
pub fn register_fixture_paths(path: &Path, register: impl Fn(&Path)) {
    register(path);
    let name = path.file_name().unwrap().to_string_lossy();
    let parent = path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    let manifest = format!("{parent}/{name}").to_ascii_lowercase();
    register(Path::new(&manifest));
    register(Path::new(name.as_ref()));
}

pub fn bundle(bundle: &CompiledNoteskinBundle) -> Value {
    let CompiledNoteskinBundle {
        version,
        game,
        skin,
        source_hash,
        loader,
        actors,
    } = bundle;
    let CompiledLoader {
        version: loader_version,
        game: loader_game,
        skin: loader_skin,
        entries,
    } = loader;
    let CompiledActors {
        version: actors_version,
        files,
    } = actors;
    let entries: Vec<_> = entries
        .iter()
        .map(|entry| {
            let CompiledLoaderEntry {
                button,
                element,
                color,
                load_button,
                load_element,
                blank,
                rotation_x,
                rotation_y,
                rotation_z,
                init_command,
            } = entry;
            let color = color.map(|color| format!("{color:?}"));
            json!({"button":button,"element":element,"color":color,"load_button":load_button,
            "load_element":load_element,"blank":blank,"rotation_x":rotation_x,
            "rotation_y":rotation_y,"rotation_z":rotation_z,"init_command":init_command})
        })
        .collect();
    let files: Vec<_> = files
        .iter()
        .map(|file| {
            let CompiledActorFile { key, decl } = file;
            // Preserve signed zero and nonfinite f32 bits that JSON would
            // otherwise normalize. All other declaration fields are text/ints.
            let delay_bits: Vec<_> = decl
                .sprites
                .iter()
                .map(|sprite| {
                    sprite.frame_delays.as_ref().map(|delays| {
                        delays
                            .iter()
                            .map(|delay| delay.to_bits())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            json!({"key":key,"decl":decl,"frame_delay_bits":delay_bits})
        })
        .collect();
    canonical(
        json!({"version":version,"game":game,"skin":skin,"source_hash":source_hash,
        "loader":{"version":loader_version,"game":loader_game,"skin":loader_skin,"entries":entries},
        "actors":{"version":actors_version,"files":files}}),
    )
}
