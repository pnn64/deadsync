// Frozen from 3c4a40d1d; unchanged fallback helpers and draw types are shared.
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

fn collect_ordered_sparse_z_buckets(objects: &[DrawItem], scratch: &mut ComposeScratch) -> bool {
    begin_sparse_z_collection(scratch);
    scratch.z_counts.clear();
    scratch.z_perm.clear();
    for object in objects {
        let encoded_z = (i32::from(object.z) - i32::from(i16::MIN)) as usize;
        let bucket = scratch.sparse_z_bucket_by_key[encoded_z];
        if bucket == MISSING_Z_BUCKET {
            if scratch.sparse_z_keys.len() == MAX_SPARSE_Z_BUCKETS {
                return false;
            }
            let bucket = scratch.sparse_z_keys.len();
            scratch.sparse_z_bucket_by_key[encoded_z] = bucket as u8;
            scratch.sparse_z_keys.push(encoded_z);
            scratch.z_counts.push(1);
            scratch.z_perm.push(object.order as usize);
        } else {
            let bucket = bucket as usize;
            if scratch.z_perm[bucket] > object.order as usize {
                return false;
            }
            scratch.z_perm[bucket] = object.order as usize;
            scratch.z_counts[bucket] += 1;
        }
    }

    scratch.sparse_z_keys.sort_unstable();
    for (sorted_bucket, &encoded_z) in scratch.sparse_z_keys.iter().enumerate() {
        let insertion_bucket = scratch.sparse_z_bucket_by_key[encoded_z] as usize;
        scratch.z_perm[sorted_bucket] = scratch.z_counts[insertion_bucket];
        scratch.sparse_z_bucket_by_key[encoded_z] = sorted_bucket as u8;
    }
    std::mem::swap(&mut scratch.z_counts, &mut scratch.z_perm);
    true
}
