// Frozen from main a17584eb34cfe5975679d1f2a2a29e635e7565ce; visibility only.

pub(super) fn child_dirs(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut dirs = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let mut path =
                PathBuf::with_capacity(dir.as_os_str().len() + file_name.as_os_str().len() + 1);
            path.push(dir);
            path.push(&file_name);
            let is_dir = entry.file_type().map_or_else(
                |_| path.is_dir(),
                |file_type| file_type.is_dir() || (file_type.is_symlink() && path.is_dir()),
            );
            (is_dir && !file_name.to_string_lossy().starts_with("._")).then_some(path)
        })
        .collect::<Vec<_>>();
    dirs.sort_by(|left, right| {
        let left = left
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let right = right
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        ascii_case_insensitive_cmp(left.as_ref(), right.as_ref())
    });
    Ok(dirs)
}

pub(super) fn pack_group_representatives(packs: &[PackScan]) -> Vec<usize> {
    let hashes = packs
        .iter()
        .map(|pack| ci_hash(&pack.group_name))
        .collect::<Vec<_>>();
    let mut order = (0..packs.len()).collect::<Vec<_>>();
    order.sort_unstable_by(|&left, &right| {
        hashes[left]
            .cmp(&hashes[right])
            .then_with(|| ci_cmp(&packs[left].group_name, &packs[right].group_name))
    });
    let mut representatives = vec![0; packs.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len()
            && hashes[order[start]] == hashes[order[end]]
            && ci_eq(
                &packs[order[start]].group_name,
                &packs[order[end]].group_name,
            )
        {
            end += 1;
        }
        let representative = *order[start..end]
            .iter()
            .min()
            .expect("group run is non-empty");
        for &index in &order[start..end] {
            representatives[index] = representative;
        }
        start = end;
    }
    representatives
}

pub(super) fn sort_song_packs(packs: &mut [SongPack]) {
    if packs.len() < 2 {
        return;
    }
    let mut order = (0..packs.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| {
        ascii_case_insensitive_cmp(&packs[left].sort_title, &packs[right].sort_title).then_with(
            || ascii_case_insensitive_cmp(&packs[left].group_name, &packs[right].group_name),
        )
    });
    let mut destinations = vec![0; packs.len()];
    for (new_index, old_index) in order.into_iter().enumerate() {
        destinations[old_index] = new_index;
    }
    for index in 0..packs.len() {
        while destinations[index] != index {
            let destination = destinations[index];
            packs.swap(index, destination);
            destinations.swap(index, destination);
        }
    }
}
