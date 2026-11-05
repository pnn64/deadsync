//! Arrow Cloud compatible, per-chart score histories stored with a local profile.
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GhostData {
    #[serde(default)]
    pub itg: Vec<f64>,
    #[serde(default)]
    pub ex: Vec<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_ex: Vec<f64>,
}

impl GhostData {
    pub fn into_histories(self) -> [Vec<f64>; 3] {
        [self.itg, self.ex, self.hard_ex]
    }

    /// Each scoring system keeps its own best passing run; ties keep the old run.
    pub fn update_best(&mut self, histories: &[Vec<f64>; 3]) -> bool {
        let mut changed = false;
        for (old, new) in [&mut self.itg, &mut self.ex, &mut self.hard_ex]
            .into_iter()
            .zip(histories)
        {
            if !new.is_empty()
                && (old.len() != new.len() || old.last() < new.last())
                && new.iter().all(|point| point.is_finite())
            {
                old.clone_from(new);
                changed = true;
            }
        }
        changed
    }
}

pub(super) fn chart_path(profile_dir: &Path, chart_hash: &str) -> io::Result<PathBuf> {
    if chart_hash.is_empty() || !chart_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid ghost chart hash",
        ));
    }
    Ok(profile_dir
        .join("GhostData")
        .join(format!("{chart_hash}.json")))
}

pub fn load(profile_dir: &Path, chart_hash: &str) -> io::Result<GhostData> {
    let file = fs::File::open(chart_path(profile_dir, chart_hash)?)?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "ghost file too large",
        ));
    }
    let data: GhostData = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if [&data.itg, &data.ex, &data.hard_ex]
        .into_iter()
        .flatten()
        .any(|p| !p.is_finite())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid ghost points",
        ));
    }
    Ok(data)
}

pub fn save_best(
    profile_dir: &Path,
    chart_hash: &str,
    histories: &[Vec<f64>; 3],
) -> io::Result<bool> {
    let path = chart_path(profile_dir, chart_hash)?;
    // A corrupt history must not prevent the next valid play from repairing it.
    let mut data = match load(profile_dir, chart_hash) {
        Ok(data) => data,
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::InvalidData
            ) =>
        {
            GhostData::default()
        }
        Err(error) => return Err(error),
    };
    if !data.update_best(histories) {
        return Ok(false);
    }
    fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec(&data)?)?;
    if let Err(error) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(true)
}

/// Import only validated histories; malformed files do not abort profile import.
pub fn import_directory(source_profile: &Path, destination_profile: &Path) -> io::Result<usize> {
    let entries = match fs::read_dir(source_profile.join("GhostData")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    let mut imported = 0;
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(hash) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let data = match load(source_profile, hash) {
            Ok(data) => data,
            Err(error) => {
                log::warn!("Skipping ghost history {}: {error}", path.display());
                continue;
            }
        };
        imported += usize::from(save_best(
            destination_profile,
            hash,
            &data.into_histories(),
        )?);
    }
    Ok(imported)
}
