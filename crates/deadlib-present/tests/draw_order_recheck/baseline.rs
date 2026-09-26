// Frozen from e02f2d180; helpers and draw types are unchanged.
use super::*;

pub(super) fn sort_composed_draw_items(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    if objects.len() < 2 {
        return;
    }
    if collect_ordered_sparse_z_buckets(objects, scratch) {
        sort_draw_items_from_sparse_counts(objects, scratch);
    } else {
        sort_draw_items(objects, scratch);
    }
}

fn sort_draw_items(objects: &mut [DrawItem], scratch: &mut ComposeScratch) {
    if objects.len() < 2 {
        return;
    }

    let mut min_z = objects[0].z;
    let mut max_z = min_z;
    let mut sorted_by_z = true;
    let mut sorted_by_key = true;
    let mut prev_key = (min_z, objects[0].order);
    for object in &objects[1..] {
        let key = (object.z, object.order);
        sorted_by_z &= prev_key.0 <= object.z;
        sorted_by_key &= prev_key <= key;
        min_z = min_z.min(object.z);
        max_z = max_z.max(object.z);
        prev_key = key;
    }
    if sorted_by_key {
        return;
    }
    if sorted_by_z {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
        return;
    }

    let range = (i32::from(max_z) - i32::from(min_z) + 1) as usize;
    let dense_range_limit = objects.len().saturating_mul(8).max(256);
    if range > dense_range_limit {
        sort_draw_items_sparse_buckets(objects, scratch);
        return;
    }

    scratch.z_counts.clear();
    scratch.z_counts.resize(range, 0);
    scratch.z_perm.resize(range, 0);

    let min_z_i = i32::from(min_z);
    let mut buckets_ordered = true;
    for object in objects.iter() {
        let bucket = (i32::from(object.z) - min_z_i) as usize;
        let bucket_seen = scratch.z_counts[bucket] != 0;
        let previous_order = &mut scratch.z_perm[bucket];
        let order = object.order as usize;
        buckets_ordered &= !bucket_seen || *previous_order <= order;
        *previous_order = order;
        scratch.z_counts[bucket] += 1;
    }
    if !buckets_ordered {
        objects.sort_unstable_by_key(|object| (object.z, object.order));
        return;
    }

    let mut next = 0usize;
    for count in &mut scratch.z_counts {
        let bucket_len = *count;
        *count = next;
        next += bucket_len;
    }

    scratch.z_perm.clear();
    scratch.z_perm.extend(objects.iter().map(|object| {
        let bucket = (i32::from(object.z) - min_z_i) as usize;
        let new_index = scratch.z_counts[bucket];
        scratch.z_counts[bucket] = new_index + 1;
        new_index
    }));

    for start in 0..objects.len() {
        while scratch.z_perm[start] != start {
            let next = scratch.z_perm[start];
            objects.swap(start, next);
            scratch.z_perm.swap(start, next);
        }
    }

    debug_assert!(
        objects
            .windows(2)
            .all(|pair| (pair[0].z, pair[0].order) <= (pair[1].z, pair[1].order))
    );
}
