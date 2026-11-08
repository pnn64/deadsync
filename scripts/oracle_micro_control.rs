//! Input-only adapter for the unmodified current harness evaluator.
use crate::song_lua_headless::{self, BpmSegment, Context, Entry};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize)]
struct Request {
    song_dir: PathBuf,
    #[serde(default)]
    virtual_simfile: Option<PathBuf>,
    #[serde(default)]
    simfile: Option<PathBuf>,
    title: String,
    difficulty: String,
    steps_type: String,
    #[serde(default)]
    description: String,
    difficulty_code: i32,
    steps_type_code: i32,
    max_beat: f32,
    bpm: f32,
    bpm_segments: Vec<[f32; 2]>,
    beat_step: f32,
    max_events: u32,
    random_seed: u32,
    entries: Vec<Root>,
}

#[derive(Deserialize)]
struct Root {
    path: PathBuf,
    layer: String,
    index: u32,
    start_beat: f32,
}

pub fn run(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), String> {
    let input = args.next().ok_or("micro control requires a JSON request")?;
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--out")) {
        return Err("micro control requires --out FILE".into());
    }
    let output = args.next().ok_or("micro control requires an output file")?;
    if args.next().is_some() { return Err("unexpected micro control argument".into()); }
    let request: Request = serde_json::from_slice(&std::fs::read(input).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    let repository = std::fs::canonicalize(".").map_err(|error| error.to_string())?;
    let resolve = |path: &Path| -> Result<PathBuf, String> {
        let resolved = std::fs::canonicalize(repository.join(path)).map_err(|error| error.to_string())?;
        resolved.strip_prefix(&repository).map_err(|_| "control input is outside the repository")?;
        Ok(resolved)
    };
    let song_dir = resolve(&request.song_dir)?;
    let (simfile, source_simfile, symbolic) = match (&request.simfile, &request.virtual_simfile) {
        (Some(path), None) => (resolve(path)?, path.clone(), false),
        (None, Some(path)) => {
            if !path.components().all(|part| matches!(part, Component::Normal(_))) {
                return Err("virtual simfile must be a relative symbolic identifier".into());
            }
            let simfile = song_dir.join(path);
            if simfile.exists() {
                return Err("virtual simfile unexpectedly exists; supply it as a real simfile".into());
            }
            (simfile, path.clone(), true)
        }
        _ => return Err("supply exactly one real or symbolic simfile".into()),
    };
    let bpms = request.bpm_segments.iter().map(|&[beat, bpm]| BpmSegment { beat, bpm }).collect::<Vec<_>>();
    let entries = request.entries.iter().map(|root| {
        let layer = match root.layer.as_str() {
            "foreground" => "foreground", "background1" => "background1", "background2" => "background2",
            _ => return Err(format!("unsupported layer {:?}", root.layer)),
        };
        Ok(Entry { path: resolve(&root.path)?, layer, index: root.index, start_beat: root.start_beat })
    }).collect::<Result<Vec<_>, String>>()?;
    let difficulty = format!("Difficulty_{}", request.difficulty.trim_start_matches("Difficulty_"));
    let context = Context {
        simfile: &simfile, song_dir: &song_dir, title: &request.title,
        difficulty: &difficulty, steps_type: &request.steps_type,
        description: &request.description, max_beat: request.max_beat,
        bpm: request.bpm, bpm_segments: &bpms, beat_step: request.beat_step,
        max_events: request.max_events, random_seed: request.random_seed,
    };
    let mut document = song_lua_headless::evaluate(&entries, &context).map_err(|error| error.to_string())?;
    document["source_simfile"] = source_simfile.to_string_lossy().replace('\\', "/").into();
    if !symbolic {
        document["simfile"] = format!("repo:/{}", source_simfile.to_string_lossy().replace('\\', "/")).into();
    }
    document["fixture_context"] = serde_json::json!({
        "chart_description": request.description, "difficulty_code": request.difficulty_code,
        "steps_type_code": request.steps_type_code, "max_beat": request.max_beat,
        "bpm": request.bpm, "bpm_segments": request.bpm_segments,
        "update_fps": 60, "beat_step": request.beat_step,
    });
    document["input_adapter"] = serde_json::json!({
        "kind": "explicit_micro_context", "simfile_is_symbolic": symbolic,
        "evaluation": "unchanged song_lua_headless::evaluate",
    });
    crate::song_lua_semantics::compact_fixture(&mut document).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&document).map_err(|error| error.to_string())?;
    std::fs::write(output, bytes).map_err(|error| error.to_string())
}
