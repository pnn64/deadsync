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
pub fn compile(root: &Path, progress: impl FnMut(usize, usize) -> bool) -> Result<(), Error> {
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
        for (index, choice) in choices.iter_mut().enumerate() {
            choice.cell = index as u16;
            for swap in &mut choice.files {
                swap.target = base_name(&names, &swap.target)?.to_owned();
            }
        }
        let preview = format!("{}-preview.png", family.to_lowercase());
        skins.push(Skin {
            id: format!("{}-workshop", family.to_lowercase()),
            base,
            preview,
            options: choices,
        });
    }
    let atlases = build_atlases(root, &skins, progress)?;
    for (skin, atlas) in skins.iter().zip(atlases) {
        atlas
            .save(root.join(&skin.preview))
            .map_err(|e| Error::Invalid(e.to_string()))?;
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

fn thumbnail_sources(skins: &[Skin]) -> BTreeMap<&str, Vec<(usize, u16)>> {
    let mut sources = BTreeMap::<&str, Vec<(usize, u16)>>::new();
    for (index, skin) in skins.iter().enumerate() {
        for choice in &skin.options {
            let path = choice
                .files
                .iter()
                .find(|swap| swap.source.to_ascii_lowercase().ends_with(".png"))
                .map(|swap| swap.source.as_str());
            // Mine-size entries contain no PNG, so use the base mine texture.
            let path = path.unwrap_or(if skin.id == "cel-workshop" {
                "Cel - Workshop/_mine tex.png"
            } else {
                "Metal - Workshop/_mine tex.png"
            });
            sources.entry(path).or_default().push((index, choice.cell));
        }
    }
    sources
}

fn build_atlases(
    root: &Path,
    skins: &[Skin],
    progress: impl FnMut(usize, usize) -> bool,
) -> Result<Vec<RgbaImage>, Error> {
    build_atlases_with(skins, progress, &|path| thumbnail(&root.join(path)))
}

fn build_atlases_with(
    skins: &[Skin],
    mut progress: impl FnMut(usize, usize) -> bool,
    decode: &(impl Fn(&str) -> Result<RgbaImage, Error> + Sync),
) -> Result<Vec<RgbaImage>, Error> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    let sources: Vec<_> = thumbnail_sources(skins).into_iter().collect();
    let total = sources.len();
    if !progress(0, total) {
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
    }
    let mut atlases: Vec<_> = skins.iter().map(|_| RgbaImage::new(ATLAS, ATLAS)).collect();
    if total == 0 {
        return Ok(atlases);
    }
    // Two simultaneous source decodes; only 60x60 thumbnails cross the channel.
    // Shared Cel/Metal sources decode once and populate all their atlas cells.
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(2);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel(1);
        for chunk in sources.chunks(total.div_ceil(workers)) {
            let tx = tx.clone();
            let cancelled = &cancelled;
            scope.spawn(move || {
                for (path, cells) in chunk {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    if tx.send((cells, decode(path))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let result: Result<(), Error> = (|| {
            for (index, (cells, tile)) in rx.iter().enumerate() {
                let tile = tile?;
                for &(skin, cell) in cells {
                    let x = (u32::from(cell) % (ATLAS / CELL)) * CELL + (CELL - tile.width()) / 2;
                    let y = (u32::from(cell) / (ATLAS / CELL)) * CELL + (CELL - tile.height()) / 2;
                    imageops::overlay(&mut atlases[skin], &tile, i64::from(x), i64::from(y));
                }
                if !progress(index + 1, total) {
                    return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).into());
                }
            }
            Ok(())
        })();
        cancelled.store(true, Ordering::Relaxed);
        drop(rx); // Wake blocked senders on cancellation/error before joining.
        result
    })?;
    Ok(atlases)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn skin(id: &str, sources: &[&str]) -> Skin {
        Skin {
            id: id.into(),
            base: String::new(),
            preview: String::new(),
            options: sources
                .iter()
                .enumerate()
                .map(|(cell, source)| Choice {
                    slot: "arrows".into(),
                    id: cell.to_string(),
                    label: cell.to_string(),
                    cell: cell as u16,
                    files: vec![FileSwap {
                        source: (*source).into(),
                        target: "arrow.png".into(),
                    }],
                    metrics: vec![],
                })
                .collect(),
        }
    }

    #[test]
    fn shared_sources_decode_once_and_fill_every_cell_with_monotonic_progress() {
        let skins = [
            skin("cel-workshop", &["a.png", "b.png"]),
            skin("metal-workshop", &["b.png", "a.png"]),
        ];
        let calls = AtomicUsize::new(0);
        let mut progress = Vec::new();
        let atlases = build_atlases_with(
            &skins,
            |done, total| {
                progress.push((done, total));
                true
            },
            &|path| {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(RgbaImage::from_pixel(
                    60,
                    60,
                    image::Rgba(if path == "a.png" {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 255]
                    }),
                ))
            },
        )
        .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert_eq!(progress, [(0, 2), (1, 2), (2, 2)]);
        assert_eq!(atlases[0].get_pixel(32, 32), atlases[1].get_pixel(96, 32));
        assert_eq!(atlases[0].get_pixel(96, 32), atlases[1].get_pixel(32, 32));
        assert_ne!(atlases[0].get_pixel(32, 32), atlases[0].get_pixel(96, 32));
        assert_eq!(atlases[0].get_pixel(0, 0).0, [0; 4]);
    }

    #[test]
    fn cancellation_and_decode_errors_stop_bounded_workers() {
        let skins = [skin("cel-workshop", &["a.png", "b.png", "c.png", "d.png"])];
        let calls = AtomicUsize::new(0);
        let result = build_atlases_with(&skins, |_, _| false, &|_| {
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(RgbaImage::new(60, 60))
        });
        assert!(matches!(result, Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::Interrupted));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let result =
            build_atlases_with(&skins, |done, _| done == 0, &|_| Ok(RgbaImage::new(60, 60)));
        assert!(matches!(result, Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::Interrupted));
        let result = build_atlases_with(&skins, |_, _| true, &|_| {
            Err(Error::Invalid("bad PNG".into()))
        });
        assert!(matches!(result, Err(Error::Invalid(message)) if message == "bad PNG"));
    }

    #[test]
    #[ignore = "requires DEADSYNC_WORKSHOP_FIXTURE; decodes the installed Workshop twice"]
    fn workshop_atlas_benchmark() {
        let root =
            PathBuf::from(std::env::var_os("DEADSYNC_WORKSHOP_FIXTURE").expect("fixture path"));
        let pack = InstalledPack::load(&root).unwrap();
        let started = std::time::Instant::now();
        let mut old = Vec::new();
        let mut count = 0;
        for skin in &pack.manifest.skins {
            let mut atlas = RgbaImage::new(ATLAS, ATLAS);
            for choice in &skin.options {
                let fallback = format!("{}/_mine tex.png", skin.base);
                let path = choice
                    .files
                    .iter()
                    .find(|swap| swap.source.to_lowercase().ends_with(".png"))
                    .map_or(fallback.as_str(), |swap| swap.source.as_str());
                let tile = thumbnail(&root.join(path)).unwrap();
                let x =
                    (u32::from(choice.cell) % (ATLAS / CELL)) * CELL + (CELL - tile.width()) / 2;
                let y =
                    (u32::from(choice.cell) / (ATLAS / CELL)) * CELL + (CELL - tile.height()) / 2;
                imageops::overlay(&mut atlas, &tile, i64::from(x), i64::from(y));
                count += 1;
            }
            old.push(atlas);
        }
        let old_time = started.elapsed();
        let started = std::time::Instant::now();
        let new = build_atlases(&root, &pack.manifest.skins, |_, _| true).unwrap();
        let new_time = started.elapsed();
        assert_eq!(
            old, new,
            "parallel/deduplicated atlases preserve every pixel"
        );
        eprintln!(
            "Workshop atlas: old {count} decodes {:.3}s; new {} decodes {:.3}s",
            old_time.as_secs_f64(),
            thumbnail_sources(&pack.manifest.skins).len(),
            new_time.as_secs_f64()
        );
    }
}
