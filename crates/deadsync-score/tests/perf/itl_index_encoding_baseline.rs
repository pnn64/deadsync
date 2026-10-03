// Frozen from be7c69c5d (0.5.1702).
use super::*;

pub(super) fn save_online_itl_self_index_file(
    path: &Path,
    by_key: &OnlineItlSelfCacheMap,
) -> Result<(), OnlineItlSelfIndexWriteError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| OnlineItlSelfIndexWriteError::CreateDir {
        dir: parent.to_path_buf(),
        error,
    })?;

    let std_by_key: HashMap<_, _> = by_key.iter().collect();
    let buf = bincode::encode_to_vec(&std_by_key, bincode::config::standard()).map_err(|_| {
        OnlineItlSelfIndexWriteError::Encode {
            path: path.to_path_buf(),
        }
    })?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, buf).map_err(|error| OnlineItlSelfIndexWriteError::WriteTemp {
        path: tmp_path.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(OnlineItlSelfIndexWriteError::Commit {
            path: path.to_path_buf(),
            error,
        });
    }
    Ok(())
}
