// Frozen from 735a994c75240ae043dc153204572d6952ed14fc.
use super::*;

pub(super) fn write_verdicts(path: &Path, verdicts: &HashMap<u64, bool>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let keyed: HashMap<String, bool> = verdicts
        .iter()
        .map(|(id, verdict)| (id.to_string(), *verdict))
        .collect();
    let text = serde_json::to_string(&keyed).map_err(|error| error.to_string())?;
    // Written beside and renamed, so an interrupted write cannot leave a
    // half-file that reads as "no verdicts" next launch.
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        error.to_string()
    })
}

// The same serialization block, measured separately from filesystem latency.
pub(super) fn encode(verdicts: &HashMap<u64, bool>) -> Result<String, String> {
    let keyed: HashMap<String, bool> = verdicts
        .iter()
        .map(|(id, verdict)| (id.to_string(), *verdict))
        .collect();
    let text = serde_json::to_string(&keyed).map_err(|error| error.to_string())?;
    Ok(text)
}
