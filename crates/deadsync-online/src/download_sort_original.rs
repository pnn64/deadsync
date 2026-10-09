// Frozen starting implementations for differential tests and paired benchmarks.
use super::*;

pub(super) fn sort_pack_paths(paths: &mut [PathBuf]) {
    struct PackPathKey {
        key_start: u32,
        key_end: u32,
        original_index: usize,
    }

    let key_bytes = paths
        .iter()
        .map(|path| path_file_name_lossy(path).len())
        .sum::<usize>();
    if key_bytes > u32::MAX as usize {
        paths.sort_by(|left, right| {
            cmp_ascii_case_insensitive(
                path_file_name_lossy(left).as_ref(),
                path_file_name_lossy(right).as_ref(),
            )
        });
        return;
    }

    let permutation_bytes = paths.len() * std::mem::size_of::<usize>();
    let mut keys = Vec::with_capacity(key_bytes.max(permutation_bytes));
    let mut path_keys = Vec::with_capacity(paths.len());
    for (original_index, path) in paths.iter().enumerate() {
        let key_start = keys.len();
        keys.extend(
            path_file_name_lossy(path)
                .bytes()
                .map(|byte| byte.to_ascii_lowercase()),
        );
        path_keys.push(PackPathKey {
            key_start: key_start as u32,
            key_end: keys.len() as u32,
            original_index,
        });
    }
    path_keys.sort_unstable_by(|left, right| {
        keys[left.key_start as usize..left.key_end as usize]
            .cmp(&keys[right.key_start as usize..right.key_end as usize])
            .then_with(|| left.original_index.cmp(&right.original_index))
    });

    keys.clear();
    keys.resize(permutation_bytes, 0);
    for (destination_index, path_key) in path_keys.iter().enumerate() {
        set_permutation_value(&mut keys, path_key.original_index, destination_index);
    }
    for original_index in 0..paths.len() {
        loop {
            let destination_index = permutation_value(&keys, original_index);
            if destination_index == original_index {
                break;
            }
            paths.swap(original_index, destination_index);
            swap_permutation_values(&mut keys, original_index, destination_index);
        }
    }
}

pub(super) fn permutation_value(bytes: &[u8], index: usize) -> usize {
    const WIDTH: usize = std::mem::size_of::<usize>();
    let start = index * WIDTH;
    usize::from_ne_bytes(bytes[start..start + WIDTH].try_into().expect("usize width"))
}

pub(super) fn set_permutation_value(bytes: &mut [u8], index: usize, value: usize) {
    const WIDTH: usize = std::mem::size_of::<usize>();
    let start = index * WIDTH;
    bytes[start..start + WIDTH].copy_from_slice(&value.to_ne_bytes());
}

pub(super) fn swap_permutation_values(bytes: &mut [u8], left: usize, right: usize) {
    const WIDTH: usize = std::mem::size_of::<usize>();
    for offset in 0..WIDTH {
        bytes.swap(left * WIDTH + offset, right * WIDTH + offset);
    }
}
