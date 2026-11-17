use deadlib_lua::{Vm, compile};

#[test]
fn lua51_garbage_collection_retires_deleted_keys_without_invalidating_next() {
    let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
    vm.set_output_capture(true);
    vm.execute(&compile(include_bytes!(
        "../../../tests/fixtures/regression-expectations/deadlib-lua/lua51_gc_dead_keys.lua"
    )).unwrap()).unwrap();
    assert_eq!(
        std::str::from_utf8(vm.output().unwrap()).unwrap().replace("\r\n", "\n"),
        include_str!("../../../tests/fixtures/regression-expectations/deadlib-lua/lua51_gc_dead_keys.txt")
            .replace("\r\n", "\n"),
    );
}

#[test]
fn lua51_table_placement_matches_reference_after_mutation_and_chunk_reload() {
    let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
    vm.set_output_capture(true);
    vm.execute(
        &compile(include_bytes!(
            "../../../tests/fixtures/regression-expectations/deadlib-lua/lua51_table_order.lua"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        std::str::from_utf8(vm.output().unwrap())
            .unwrap()
            .replace("\r\n", "\n"),
        include_str!(
            "../../../tests/fixtures/regression-expectations/deadlib-lua/lua51_table_order.txt"
        )
        .replace("\r\n", "\n"),
    );
}
