// Frozen from main 16a06a2619a6cdcc34c647603d5dfa186e2f3e73; visibility only.

pub(super) fn set_sync_pref_in_packs(
    packs: &mut [SongPack],
    group_name: &str,
    sync_pref: SyncPref,
) -> bool {
    let wanted = group_name.to_lowercase();
    let mut changed = false;
    for pack in packs
        .iter_mut()
        .filter(|pack| pack.group_name.to_lowercase() == wanted)
    {
        changed |= pack.sync_pref != sync_pref;
        pack.sync_pref = sync_pref;
    }
    changed
}
