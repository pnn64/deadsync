use super::*;
use crate::perf::{assert_churn_budget, measure_sampled};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "tap_layers/baseline.rs"]
mod baseline;

// Mirrors SpriteSlot's relevant cloning costs: a fresh atomic identity,
// four shared resources, and inline model state. No heap-owning String clone.
struct Slot {
    identity: u64,
    value: u32,
    kind: u8,
    resources: [Arc<[u8]>; 4],
    state: [f32; 48],
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
impl Clone for Slot {
    fn clone(&self) -> Self {
        Self {
            identity: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            value: self.value,
            kind: self.kind,
            resources: self.resources.clone(),
            state: self.state,
        }
    }
}
fn fixture(count: usize) -> Vec<Slot> {
    let resource: Arc<[u8]> = Arc::from([0; 64]);
    (0..count)
        .map(|i| Slot {
            identity: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            value: i as u32,
            kind: (i % 3) as u8,
            resources: std::array::from_fn(|_| resource.clone()),
            state: [i as f32; 48],
        })
        .collect()
}
fn info(slot: &Slot) -> (bool, [f32; 2]) {
    (
        slot.kind != 2,
        [if slot.kind == 0 { 1.0 } else { 0.0 }, 0.0],
    )
}
type ColumnOutput = (
    Vec<Slot>,
    Vec<Arc<[Slot]>>,
    Option<PreparedTapNoteColumn<Slot>>,
);
fn run(old: bool, input: Vec<Slot>, quantizations: usize) -> ColumnOutput {
    let mut notes = Vec::with_capacity(quantizations);
    let mut layers = Vec::with_capacity(quantizations);
    let result = if old {
        baseline::emit_tap_note_column(input, quantizations, info, &mut notes, &mut layers)
    } else {
        emit_tap_note_column(input, quantizations, info, &mut notes, &mut layers)
    };
    (notes, layers, result)
}

#[test]
fn moved_layers_preserve_order_values_sharing_and_distinct_note_identities() {
    for count in [0, 1, 2, 3, 8, 32, 128] {
        for quantizations in [0, 1, 9, 32] {
            let input = fixture(count);
            let original_ids: Vec<_> = input.iter().map(|s| s.identity).collect();
            let (old_notes, old_layers, old_column) = run(
                true,
                input.iter().map(Clone::clone).collect(),
                quantizations,
            );
            let (notes, layers, column) = run(false, input, quantizations);
            let values = |slots: &[Slot]| {
                slots
                    .iter()
                    .map(|s| (s.value, s.kind, s.state))
                    .collect::<Vec<_>>()
            };
            assert_eq!(values(&notes), values(&old_notes));
            assert_eq!(layers.len(), old_layers.len());
            assert_eq!(column.is_some(), old_column.is_some());
            if let (Some(column), Some(old_column)) = (column, old_column) {
                assert_eq!(
                    values(&column.shared_layers),
                    values(&old_column.shared_layers)
                );
                for (slot, old) in column
                    .shared_layers
                    .iter()
                    .zip(old_column.shared_layers.iter())
                {
                    assert!(original_ids.contains(&slot.identity));
                    for (a, b) in slot.resources.iter().zip(&old.resources) {
                        assert!(Arc::ptr_eq(a, b));
                    }
                }
                for shared in layers {
                    assert!(Arc::ptr_eq(&shared, &column.shared_layers));
                }
                let lift = itg_lift_layers_for_col_shared(Vec::new(), &column.shared_layers);
                assert!(Arc::ptr_eq(&lift, &column.shared_layers));
                for (i, note) in notes.iter().enumerate() {
                    assert!(!original_ids.contains(&note.identity));
                    assert!(
                        notes[..i]
                            .iter()
                            .all(|other| other.identity != note.identity)
                    );
                }
            }
        }
    }
}

#[test]
fn tap_columns_move_heap_payloads_without_cloning_them() {
    let mut input = Vec::with_capacity(8);
    for i in 0..8 {
        input.push(vec![i; 1024]);
    }
    let pointers: Vec<_> = input.iter().map(|v| v.as_ptr()).collect();
    let mut notes = Vec::new();
    let mut layers = Vec::new();
    let mut result = None;
    assert_churn_budget(1, 16 + 8 * size_of::<Vec<u8>>(), || {
        result = emit_tap_note_column(input, 0, |_| (false, [0.0; 2]), &mut notes, &mut layers);
    });
    let result = result.unwrap();
    for (slot, pointer) in result.shared_layers.iter().zip(pointers) {
        assert_eq!(slot.as_ptr(), pointer);
    }
}

#[test]
fn empty_tap_columns_leave_existing_outputs_untouched() {
    let mut notes = vec![1u8];
    let mut layers = vec![Arc::from([2u8])];
    assert!(
        emit_tap_note_column(Vec::new(), 9, |_| panic!("empty"), &mut notes, &mut layers).is_none()
    );
    assert_eq!(notes, [1]);
    assert_eq!(layers[0].as_ref(), [2]);
}

#[test]
#[ignore = "manual release old/new benchmark"]
fn preparation_bench_layers() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, count, quantizations) in [
        ("layers_empty", 0, 9),
        ("layers_single", 1, 9),
        ("layers_three", 3, 9),
        ("layers_eight", 8, 9),
        ("layers_32", 32, 9),
        ("layers_zero_quantizations", 8, 0),
    ] {
        let input = fixture(count);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("{name}_{}", if old { "old" } else { "new" });
            // Both sides pay the same clone to provide an owned input each time.
            measure_sampled(&label, 2048, 1, || {
                run(old, black_box(&input).clone(), black_box(quantizations))
            });
        }
    }
}
