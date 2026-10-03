// Frozen from 8a2ecf968 / 0.5.1703.
use super::*;
pub(super) fn itl_mark_unlock_folders<'a, I>(data: &mut ItlFileData, folders: I) -> bool
where
    I: IntoIterator<Item = &'a str>,
{
    let mut changed = false;
    for folder in folders {
        let folder = folder.trim();
        if !folder.is_empty() {
            changed |= data.unlock_folders.insert(folder.to_string(), true) != Some(true);
        }
    }
    changed
}
