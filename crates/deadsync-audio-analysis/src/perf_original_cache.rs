// Frozen from 0530013f08d5b481d29533127de2dc41033f63be; only visibility differs.
use super::*;
pub(super) fn encode_replaygain_cache(payload: &ReplayGainCacheFile) -> Result<Vec<u8>, String> {
    let config = bincode::config::standard();
    let body_len = bincode::encoded_size(payload, config).map_err(|e| format!("{e}"))?;
    let mut out = vec![0; 12 + body_len];
    out[..8].copy_from_slice(&CACHE_MAGIC.to_le_bytes());
    out[8..12].copy_from_slice(&CACHE_VERSION.to_le_bytes());
    let written =
        bincode::encode_into_slice(payload, &mut out[12..], config).map_err(|e| format!("{e}"))?;
    out.truncate(12 + written);
    Ok(out)
}
