// Frozen from a463bbc40 (0.5.1229); keep independent of optimized helpers.
use super::*;

pub(super) fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, Error> {
    let path = Path::new(relative);
    if relative.is_empty()
        || relative.contains(['\\', ':'])
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(Error::Invalid(format!("invalid pack path: {relative}")));
    }
    let resolved = root.join(path).canonicalize()?;
    if !resolved.starts_with(root.canonicalize()?) {
        return Err(Error::Invalid(format!(
            "pack path escapes its directory: {relative}"
        )));
    }
    Ok(resolved)
}

pub(super) fn validate_choices(root: &Path, base: &Path, skin: &Skin) -> Result<(), Error> {
    let mut ids = HashSet::new();
    for choice in &skin.options {
        if !SLOTS.contains(&choice.slot.as_str())
            || !valid_id(&choice.id)
            || !ids.insert((&choice.slot, &choice.id))
            || choice.cell >= 1024
            || choice.label.is_empty()
            || choice.label.len() > 160
            || choice.files.len() > 32
            || choice.metrics.len() > 32
        {
            return Err(Error::Invalid(format!("invalid option in {}", skin.id)));
        }
        let mut targets = HashSet::new();
        for swap in &choice.files {
            let target = checked_path(base, &swap.target)?;
            let source = checked_path(root, &swap.source)?;
            if !source.is_file() || !target.is_file() || !targets.insert(target) {
                return Err(Error::Invalid(
                    "invalid or duplicate replacement file".into(),
                ));
            }
        }
        for metric in &choice.metrics {
            if !metric.section.eq_ignore_ascii_case("NoteDisplay")
                || metric.key.is_empty()
                || metric.key.contains(['\n', '\r', '='])
                || metric.value.contains(['\n', '\r'])
            {
                return Err(Error::Invalid("invalid NoteDisplay override".into()));
            }
        }
    }
    Ok(())
}

pub(super) struct LegacyPack(pub InstalledPack);
impl std::ops::Deref for LegacyPack {
    type Target = InstalledPack;
    fn deref(&self) -> &InstalledPack {
        &self.0
    }
}
impl LegacyPack {
    pub(super) fn load(root: &Path) -> Result<InstalledPack, Error> {
        let path = root.join("pack.json");
        if fs::metadata(&path)?.len() > MAX_MANIFEST_BYTES {
            return Err(Error::Invalid("noteskin manifest exceeds 4 MiB".into()));
        }
        let bytes = fs::read(path)?;
        let manifest: Manifest = serde_json::from_slice(&bytes)?;
        if manifest.schema != 1
            || !valid_id(&manifest.id)
            || manifest.version.is_empty()
            || manifest.skins.is_empty()
            || manifest.skins.len() > MAX_SKINS
        {
            return Err(Error::Invalid(
                "unsupported or invalid noteskin pack".into(),
            ));
        }
        let mut ids = HashSet::new();
        for skin in &manifest.skins {
            if !valid_id(&skin.id) || !ids.insert(&skin.id) || skin.options.len() > MAX_CHOICES {
                return Err(Error::Invalid("invalid or duplicate pack skin ID".into()));
            }
            let base = checked_path(root, &skin.base)?;
            if !base.join("metrics.ini").is_file() || !base.join("NoteSkin.lua").is_file() {
                return Err(Error::Invalid(format!(
                    "incomplete base noteskin: {}",
                    skin.id
                )));
            }
            let preview = checked_path(root, &skin.preview)?;
            if image::image_dimensions(preview).map_err(|e| Error::Invalid(e.to_string()))?
                != (2048, 2048)
            {
                return Err(Error::Invalid(
                    "pack preview must be a 2048x2048 atlas".into(),
                ));
            }
            validate_choices(root, &base, skin)?;
        }
        let root = root.canonicalize()?;
        let mut hash = XxHash64::default();
        hash.write(&bytes);
        hash.write(root.to_string_lossy().as_bytes());
        Ok(InstalledPack {
            root,
            manifest,
            fingerprint: format!("{:016x}", hash.finish()),
        })
    }

    pub(super) fn runtime_key(&self, selection: &Selection) -> String {
        let mut hash = XxHash64::default();
        hash.write(self.fingerprint.as_bytes());
        hash.write(selection.to_string().as_bytes());
        format!("{}-{:016x}", selection.skin, hash.finish())
    }

    pub(super) fn compiler_key(&self, selection: &Selection) -> String {
        let mut program = selection.clone();
        if let Some(skin) = self.skin(&selection.skin) {
            program.options.retain(|slot, id| {
                skin.options
                    .iter()
                    .find(|choice| choice.slot == *slot && choice.id == *id)
                    .is_none_or(|choice| {
                        !choice.metrics.is_empty()
                            || choice.files.iter().any(|swap| {
                                [&swap.source, &swap.target].into_iter().any(|path| {
                                    !Path::new(path)
                                        .extension()
                                        .and_then(|ext| ext.to_str())
                                        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                                })
                            })
                    })
            });
        }
        self.runtime_key(&program)
    }
}
