use super::*;
use crate::compile;

#[test]
fn ordinary_field_results_are_rooted_until_the_scratch_register_is_reused() {
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        let target = call.arguments()[0];
        let key = call.arguments()[1];
        let Value::Table(table) = target else {
            unreachable!()
        };
        let value = call.table_get(target, key)?;
        call.raw_table_set(table, key, Value::Nil)?;
        // Collect directly inside the VM: opening a native stack or reentering
        // Lua would consume the scratch result before collection.
        call.vm.gc_control(GcOperation::Collect)?;
        call.push(value)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(read, &[]).unwrap();
    vm.set_global(b"read", function).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
            local weak = setmetatable({}, {__mode = 'v'})
            local t = {x = {answer = 42}}
            weak[1] = t.x
            local value = read(t, 'x')
            assert(value.answer == 42 and value == weak[1] and t.x == nil)
            value = nil
            collectgarbage(); collectgarbage()
            return weak[1] == nil
        "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
}
