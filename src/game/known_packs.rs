use bincode::{Decode, Encode};
use log::warn;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

const KNOWN_PACKS_PATH: &str = "save/known_packs.bin";

#[derive(Encode, Decode, Default)]
struct KnownPacksLedger {
    pack_names: HashSet<String>,
}

static KNOWN_PACKS: std::sync::LazyLock<Mutex<KnownPacksLedger>> =
    std::sync::LazyLock::new(|| Mutex::new(load_known_packs().unwrap_or_default()));

fn load_known_packs() -> Option<KnownPacksLedger> {
    let bytes = fs::read(KNOWN_PACKS_PATH).ok()?;
    let (ledger, _) =
        bincode::decode_from_slice::<KnownPacksLedger, _>(&bytes, bincode::config::standard())
            .ok()?;
    Some(ledger)
}

fn save_known_packs(ledger: &KnownPacksLedger) {
    let path = Path::new(KNOWN_PACKS_PATH);
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            warn!("Failed to create known_packs dir: {e}");
            return;
        }
    }
    let Ok(buf) = bincode::encode_to_vec(ledger, bincode::config::standard()) else {
        warn!("Failed to encode known_packs ledger");
        return;
    };
    let tmp = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp, &buf) {
        warn!("Failed to write known_packs temp file: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        warn!("Failed to commit known_packs file: {e}");
        let _ = fs::remove_file(&tmp);
    }
}

/// Called once after song scanning completes. Seeds the ledger on first launch
/// (so existing packs aren't all marked "NEW"), and returns the set of new pack names.
pub fn sync_known_packs(scanned_pack_names: &[String]) -> HashSet<String> {
    let mut ledger = KNOWN_PACKS.lock().unwrap();
    if ledger.pack_names.is_empty() && !scanned_pack_names.is_empty() {
        // First launch: seed with all current packs, nothing is "new".
        ledger.pack_names = scanned_pack_names.iter().cloned().collect();
        save_known_packs(&ledger);
        return HashSet::new();
    }
    let new_packs: HashSet<String> = scanned_pack_names
        .iter()
        .filter(|name| !ledger.pack_names.contains(name.as_str()))
        .cloned()
        .collect();
    // Add all scanned packs to the ledger (covers packs deleted and re-added).
    for name in scanned_pack_names {
        ledger.pack_names.insert(name.clone());
    }
    save_known_packs(&ledger);
    new_packs
}

/// Mark a single pack as known (e.g., when the player expands it on the wheel).
pub fn mark_pack_known(name: &str) {
    let mut ledger = KNOWN_PACKS.lock().unwrap();
    if ledger.pack_names.insert(name.to_owned()) {
        save_known_packs(&ledger);
    }
}
