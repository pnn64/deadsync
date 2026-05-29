pub use deadsync_chart::song::{
    SongBackgroundChange, SongBackgroundChangeTarget, SongBackgroundLuaChange, SongData,
    SongForegroundChange, SongForegroundLuaChange, SongPack,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub const ITG_SYNC_OFFSET_SECONDS: f32 = -0.009;

#[inline(always)]
pub const fn default_sync_pref_offset(pref: crate::config::DefaultSyncOffset) -> f32 {
    match pref {
        crate::config::DefaultSyncOffset::Null => 0.0,
        crate::config::DefaultSyncOffset::Itg => ITG_SYNC_OFFSET_SECONDS,
    }
}

#[inline(always)]
pub const fn pack_sync_pref_offset(
    pref: rssp::pack::SyncPref,
    default: crate::config::DefaultSyncOffset,
) -> f32 {
    match pref {
        rssp::pack::SyncPref::Default => default_sync_pref_offset(default),
        rssp::pack::SyncPref::Null => 0.0,
        rssp::pack::SyncPref::Itg => ITG_SYNC_OFFSET_SECONDS,
    }
}

#[inline(always)]
pub const fn pack_sync_pref_default(
    pref: rssp::pack::SyncPref,
    default: crate::config::DefaultSyncOffset,
) -> crate::config::DefaultSyncOffset {
    match pref {
        rssp::pack::SyncPref::Default => default,
        rssp::pack::SyncPref::Null => crate::config::DefaultSyncOffset::Null,
        rssp::pack::SyncPref::Itg => crate::config::DefaultSyncOffset::Itg,
    }
}

static SONG_CACHE: std::sync::LazyLock<Mutex<Vec<SongPack>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
static SONG_CACHE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Provides safe, read-only access to the global song cache.
pub fn get_song_cache() -> std::sync::MutexGuard<'static, Vec<SongPack>> {
    SONG_CACHE.lock().unwrap()
}

pub fn song_cache_generation() -> u64 {
    SONG_CACHE_GENERATION.load(Ordering::Relaxed)
}

/// A public function to allow the parser to populate the cache.
pub(super) fn set_song_cache(packs: Vec<SongPack>) {
    let mut cache = SONG_CACHE.lock().unwrap();
    *cache = packs;
    SONG_CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DefaultSyncOffset;
    use rssp::pack::SyncPref;

    #[test]
    fn pack_sync_pref_offset_matches_itg_group_offset() {
        assert_eq!(
            pack_sync_pref_offset(SyncPref::Null, DefaultSyncOffset::Itg),
            0.0
        );
        assert_eq!(
            pack_sync_pref_offset(SyncPref::Itg, DefaultSyncOffset::Null),
            ITG_SYNC_OFFSET_SECONDS
        );
    }

    #[test]
    fn default_pack_sync_pref_uses_machine_default() {
        assert_eq!(
            pack_sync_pref_offset(SyncPref::Default, DefaultSyncOffset::Null),
            0.0
        );
        assert_eq!(
            pack_sync_pref_offset(SyncPref::Default, DefaultSyncOffset::Itg),
            ITG_SYNC_OFFSET_SECONDS
        );
    }
}
