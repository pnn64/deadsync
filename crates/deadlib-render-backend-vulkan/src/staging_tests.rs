use super::*;

fn staging(capacity: vk::DeviceSize) -> TextureStagingBuffer {
    TextureStagingBuffer {
        resource: BufferResource {
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
        },
        capacity,
    }
}

#[test]
fn staging_selection_preserves_fits_ties_and_extreme_sizes() {
    for (capacities, needed, expected) in [
        (vec![], 0, None),
        (vec![0], 0, Some(0)),
        (vec![0, 1, 2], 3, None),
        (vec![4096, 1024, 4096, 2048, 8192], 4096, Some(0)),
        (vec![4096, 1024, 4096, 2048, 8192], 1025, Some(3)),
        (vec![4096, 1024, 4096, 2048, 8192], 4097, Some(4)),
        (vec![u64::MAX, 0, 1], u64::MAX, Some(0)),
        (vec![u64::MAX, 0, 1], 0, Some(1)),
    ] {
        let pool: Vec<_> = capacities.into_iter().map(staging).collect();
        assert_eq!(best_fit_staging_index(&pool, needed), expected);
    }
}

#[test]
fn staging_reuse_preserves_pool_order_and_byte_accounting() {
    let mut pool: Vec<_> = [4096, 1024, 4096, 2048, 8192].map(staging).into();
    let mut bytes = 19456;
    let mut selected = None;
    perf::assert_no_churn(|| {
        selected = take_pooled_texture_staging(&mut pool, &mut bytes, 4096);
    });
    assert_eq!(selected.unwrap().capacity, 4096);
    assert_eq!(bytes, 15360);
    assert_eq!(
        pool.iter().map(|s| s.capacity).collect::<Vec<_>>(),
        [8192, 1024, 4096, 2048]
    );
    assert!(take_pooled_texture_staging(&mut pool, &mut bytes, 8193).is_none());
    assert_eq!(bytes, 15360);
    assert_eq!(pool.len(), 4);
}
