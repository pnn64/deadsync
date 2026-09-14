// Frozen from a463bbc40 (0.5.1229); keep independent of optimized helpers.
use super::*;

pub(super) fn slug(label: &str) -> String {
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

pub(super) fn choices_for(
    files: &[PathBuf],
    root: &Path,
    family: &str,
) -> Result<Vec<Choice>, Error> {
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
