//! Compile Cel and Metal Workshop assets into a selectable noteskin pack.

use crate::pack::{Choice, Error, FileSwap, InstalledPack, Manifest, MetricSwap, Skin};
use image::{ImageReader, RgbaImage, imageops};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const PACK_ID: &str = "hurg-cel-metal";
pub const COMMIT: &str = "5ba831ae039e7319b7ae5f223b3532c05e8b4072";
pub const SOURCE: &str = "https://github.com/HURG-IIDX/Noteskin-Workshop-Cel-and-Metal";
// One 16 MiB atlas per skin, with room for 1,024 choices.
const CELL: u32 = 64;
const ATLAS: u32 = 2048;

/// Build previews and a manifest beside the extracted upstream asset directories.
///
/// Runs on a load worker. `progress` returns false to cancel between choices.
/// The manifest is written last; callers must publish the directory only on success.
///
/// # Errors
/// Returns an error for missing or malformed assets, I/O failure, or cancellation.
pub fn compile(root: &Path, mut progress: impl FnMut(usize, usize) -> bool) -> Result<(), Error> {
    let files = files_in(&root.join("Customizations"))?;
    let mut skins = Vec::with_capacity(2);
    for family in ["Cel", "Metal"] {
        let base = format!("{family} - Workshop");
        let names: BTreeMap<_, _> = files_in(&root.join(&base))?
            .iter()
            .filter_map(|p| {
                p.file_name()?
                    .to_str()
                    .map(|s| (s.to_lowercase(), s.to_owned()))
            })
            .collect();
        let mut choices = choices_for(&files, root, family)?;
        let slots: std::collections::BTreeSet<_> = choices.iter().map(|c| c.slot.clone()).collect();
        for slot in slots {
            let sample = choices
                .iter()
                .find(|c| c.slot == slot)
                .expect("slot came from choices");
            let mut files = Vec::with_capacity(sample.files.len());
            for swap in &sample.files {
                let target = base_name(&names, &swap.target)?;
                files.push(FileSwap {
                    target: target.to_owned(),
                    source: format!("{base}/{target}"),
                });
            }
            choices.push(Choice {
                slot,
                id: "base".into(),
                label: "Original".into(),
                cell: 0,
                files,
                metrics: vec![],
            });
        }
        if choices.len() > (ATLAS / CELL).pow(2) as usize {
            return Err(Error::Invalid("workshop preview atlas is full".into()));
        }
        let count = choices.len();
        let mut atlas = RgbaImage::new(ATLAS, ATLAS);
        for (index, choice) in choices.iter_mut().enumerate() {
            if !progress(index, count) {
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
            }
            choice.cell = index as u16;
            for swap in &mut choice.files {
                swap.target = base_name(&names, &swap.target)?.to_owned();
            }
            let fallback = format!("{base}/_mine tex.png");
            let path = choice
                .files
                .iter()
                .find(|s| s.source.to_lowercase().ends_with(".png"))
                .map_or(fallback.as_str(), |s| s.source.as_str());
            let tile = thumbnail(&root.join(path))?;
            let x = (index as u32 % (ATLAS / CELL)) * CELL + (CELL - tile.width()) / 2;
            let y = (index as u32 / (ATLAS / CELL)) * CELL + (CELL - tile.height()) / 2;
            imageops::overlay(&mut atlas, &tile, i64::from(x), i64::from(y));
        }
        let preview = format!("{}-preview.png", family.to_lowercase());
        atlas
            .save(root.join(&preview))
            .map_err(|e| Error::Invalid(e.to_string()))?;
        skins.push(Skin {
            id: format!("{}-workshop", family.to_lowercase()),
            base,
            preview,
            options: choices,
        });
    }
    let manifest = Manifest {
        schema: 1,
        id: PACK_ID.into(),
        version: format!("{COMMIT}-rust1"),
        source: SOURCE.into(),
        skins,
    };
    fs::write(
        root.join("pack.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    InstalledPack::load(root)?;
    Ok(())
}

fn files_in(root: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    let mut dirs = vec![root.to_owned()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                dirs.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            } else {
                return Err(Error::Invalid(
                    "workshop contains a non-regular file".into(),
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

fn base_name<'a>(names: &'a BTreeMap<String, String>, target: &str) -> Result<&'a str, Error> {
    names
        .get(&target.to_lowercase())
        .map(String::as_str)
        .ok_or_else(|| Error::Invalid(format!("missing workshop base file: {target}")))
}

fn slug(label: &str) -> String {
    let mut result = String::with_capacity(label.len());
    for c in label.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else if !result.is_empty() && !result.ends_with('-') {
            result.push('-');
        }
    }
    result.trim_end_matches('-').to_owned()
}

fn choices_for(files: &[PathBuf], root: &Path, family: &str) -> Result<Vec<Choice>, Error> {
    let mut groups = BTreeMap::<(String, String), Choice>::new();
    let other = if family == "Cel" { "Metal" } else { "Cel" };
    for path in files {
        let relative = path
            .strip_prefix(root)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .to_str()
            .ok_or_else(|| Error::Invalid("non-UTF8 workshop path".into()))?
            .replace('\\', "/");
        let parts: Vec<_> = relative.split('/').collect();
        if parts.len() < 4 {
            return Err(Error::Invalid(format!("invalid customization: {relative}")));
        }
        let folders = &parts[2..parts.len() - 1];
        if folders.iter().any(|s| s.contains(other)) {
            continue;
        }
        let label = folders
            .iter()
            .map(|s| s.replace(family, ""))
            .map(|s| {
                s.trim_matches(|c: char| c.is_whitespace() || c == '-')
                    .to_owned()
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
        let filename = parts[parts.len() - 1];
        let inactive = filename.to_lowercase().contains("inactive");
        let slot = match parts[1] {
            "Arrows" => "arrows",
            "Receptors" => "receptors",
            "Tap Explosions" => "tap_explosions",
            "Hold Explosions" => "hold_explosions",
            "Mines" => "mines",
            "Mine Size" => "mine_size",
            "Lifts" => "lifts",
            "Holds" => {
                if inactive {
                    "hold_inactive"
                } else {
                    "hold_active"
                }
            }
            "Rolls" => {
                if inactive {
                    "roll_inactive"
                } else {
                    "roll_active"
                }
            }
            category => return Err(Error::Invalid(format!("unknown customization: {category}"))),
        };
        let id = slug(&label);
        let choice = groups
            .entry((slot.into(), id.clone()))
            .or_insert_with(|| Choice {
                slot: slot.into(),
                id,
                label: label.clone(),
                cell: 0,
                files: vec![],
                metrics: vec![],
            });
        if choice.label != label {
            return Err(Error::Invalid(format!("duplicate customization: {label}")));
        }
        choice.files.push(FileSwap {
            target: filename.into(),
            source: relative,
        });
    }
    let mut choices: Vec<_> = groups.into_values().collect();
    for choice in &mut choices {
        let label = choice.label.to_lowercase();
        if choice.slot == "arrows" && (label.contains("rgb") || label.contains("ddr vivid")) {
            choice.metrics.push(MetricSwap {
                section: "NoteDisplay".into(),
                key: "TapNoteAnimationLength".into(),
                value: "4".into(),
            });
        }
    }
    choices.sort_by_cached_key(|c| (c.slot.clone(), c.label.to_lowercase()));
    Ok(choices)
}

fn thumbnail(path: &Path) -> Result<RgbaImage, Error> {
    let mut reader = ImageReader::open(path)?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let mut image = reader
        .decode()
        .map_err(|e| Error::Invalid(e.to_string()))?
        .into_rgba8();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_lowercase();
    // Logical resolution hints are not sprite-sheet dimensions.
    let name = if let Some(start) = name.find("(res ") {
        let end = name[start..]
            .find(')')
            .map_or(name.len(), |n| start + n + 1);
        format!("{}{}", &name[..start], &name[end..])
    } else {
        name
    };
    for (pos, _) in name.match_indices('x') {
        let left: String = name[..pos]
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect();
        let left: String = left.chars().rev().collect();
        let right: String = name[pos + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let (Ok(cols), Ok(rows)) = (left.parse::<u32>(), right.parse::<u32>()) {
            if cols == 0 || rows == 0 || cols > image.width() || rows > image.height() {
                return Err(Error::Invalid(format!("invalid sprite sheet: {name}")));
            }
            image = imageops::crop_imm(&image, 0, 0, image.width() / cols, image.height() / rows)
                .to_image();
            break;
        }
    }
    Ok(imageops::thumbnail(&image, CELL - 4, CELL - 4))
}
