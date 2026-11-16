use super::*;


#[path = "../../../tests/support/allocations.rs"]
#[allow(dead_code)]
pub(crate) mod allocations;


#[test]
fn owned_bind_reuses_storage_without_allocator_churn() {
    let mut current = Keymap::default();
    let values = vec![
        InputBinding::Key(KeyCode::KeyA),
        InputBinding::PadDir(PadDir::Left),
    ];
    current.bind(VirtualAction::p1_left, &values);
    current.bind(VirtualAction::p1_left, &[]); // retain reverse-list capacities
    let pointer = values.as_ptr();
    allocations::assert_no_churn(|| current.bind_owned(VirtualAction::p1_left, values));
    assert_eq!(
        current.bindings_for_action(VirtualAction::p1_left).as_ptr(),
        pointer
    );
}
