// Frozen from def9a12f131b4dcbc0cc3ed4c3f9fd2f61f54ec1; only the function name changed.
fn original_choices_for(
    files: &[PathBuf],
    root: &Path,
    family: &str,
) -> Result<Vec<Choice>, Error> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let mut groups = BTreeMap::<(String, String), Choice>::new();
    let other = if family == "Cel" { "Metal" } else { "Cel" };
    let mut label = String::new();
    let mut folder_text = String::new();
    for path in files {
        let relative = path
            .strip_prefix(root)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .to_str()
            .ok_or_else(|| Error::Invalid("non-UTF8 workshop path".into()))?
            .replace('\\', "/");
        let mut parts = relative.splitn(3, '/');
        parts.next();
        let category = parts.next();
        let Some((folders, filename)) = parts.next().and_then(|rest| rest.rsplit_once('/')) else {
            return Err(Error::Invalid(format!("invalid customization: {relative}")));
        };
        if folders.split('/').any(|folder| folder.contains(other)) {
            continue;
        }
        label.clear();
        for folder in folders.split('/') {
            folder_text.clear();
            for part in folder.split(family) {
                folder_text.push_str(part);
            }
            let trimmed = folder_text.trim_matches(|c: char| c.is_whitespace() || c == '-');
            if !trimmed.is_empty() {
                if !label.is_empty() {
                    label.push_str(" / ");
                }
                label.push_str(trimmed);
            }
        }
        let inactive = filename.to_lowercase().contains("inactive");
        let slot = match category.expect("three path components were present") {
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
    // BTreeMap already groups slots in lexical order. Sort only within each
    // slot, retaining the same stable label order without cloning slot keys.
    for slot in choices.chunk_by_mut(|a, b| a.slot == b.slot) {
        slot.sort_by_cached_key(|choice| choice.label.to_lowercase());
    }
    Ok(choices)
}
