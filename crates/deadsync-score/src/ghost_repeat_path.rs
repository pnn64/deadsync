// Frozen save body with current helpers: isolate repeated path construction.
use super::*;
pub(super) fn save_best(
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
