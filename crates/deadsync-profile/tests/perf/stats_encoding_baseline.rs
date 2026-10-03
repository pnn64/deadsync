// Frozen from be7c69c5d (0.5.1702).
use super::*;

pub(super) fn encode_profile_stats(stats: &ProfileStats) -> Option<Vec<u8>> {
    let mut known_pack_names: Vec<String> = stats.known_pack_names.iter().cloned().collect();
    known_pack_names.sort_unstable();
    bincode::encode_to_vec(
        ProfileStatsV1 {
            version: PROFILE_STATS_VERSION_V1,
            current_combo: stats.current_combo,
            known_pack_names,
        },
        bincode::config::standard(),
    )
    .ok()
}
