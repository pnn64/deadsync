use deadlib_lua::{Value, Vm, compile};

fn numbers(source: &[u8]) -> Vec<f64> {
    let mut vm = Vm::new();
    vm.execute(&compile(source).unwrap())
        .unwrap()
        .iter()
        .map(|value| match value {
            Value::Number(number) => *number,
            _ => panic!("expected numeric result: {value:?}"),
        })
        .collect()
}

#[test]
fn sums_preserve_iteration_direction_and_live_outputs() {
    assert_eq!(
        numbers(
            br#"
            local a, last = 0, -1
            for i = 1, 1000000 do a, last = a + i, i end
            local b = 0
            for i = 100, 1, -3 do b = i + b end
            local c = 0
            for i = -1, -100, -3 do c = c + i end
            return a, last, b, c
        "#
        ),
        [500000500000.0, 1000000.0, 1717.0, -1717.0]
    );
}

#[test]
fn arithmetic_boundaries_keep_sequential_float_results() {
    let results = numbers(
        br#"
        local a, b, c = 0, -0.0, -9007199254740992
        for i = 9007199254740988, 9007199254740991 do a = a + i end
        for i = 0, 0 do b = b + i end
        for i = 1, 7 do c = c + i end
        local d = 0
        for i = 0.25, 3, 0.5 do d = d + i end
        return a, 1 / b, c, d
    "#,
    );
    let sequential =
        (9007199254740988_i64..=9007199254740991).fold(0.0_f64, |sum, value| sum + value as f64);
    let cancellation = (1..=7).fold(-9007199254740992.0_f64, |sum, value| sum + value as f64);
    assert_eq!(results[0].to_bits(), sequential.to_bits());
    assert_eq!(results[1], f64::INFINITY);
    assert_eq!(results[2].to_bits(), cancellation.to_bits());
    assert_eq!(results[3], 9.0);
}

#[test]
fn loop_carried_values_and_index_edits_remain_observable() {
    assert_eq!(
        numbers(
            br#"
            local a, previous, last = 0, 3, 0
            for i = 1, 10 do
                last = previous
                previous = i
                a = a + i
            end
            local b = 0
            for i = 1, 5 do
                i = 2
                b = b + i
            end
            return a, previous, last, b
        "#
        ),
        [55.0, 10.0, 9.0, 10.0]
    );
}

#[test]
fn budgets_match_count_hook_dispatch_without_firing_the_hook() {
    let hook =
        compile(b"debug.sethook(function() error('unexpected hook') end, '', 2147483647)").unwrap();
    for source in [
        b"local s=0; for i=1,7 do s=s+i end; return s".as_slice(),
        b"local s=1; for i=10,1,-2 do s=i+s end; return s".as_slice(),
    ] {
        let program = compile(source).unwrap();
        for limit in 0..160 {
            let mut ordinary = Vm::new();
            let mut hooked = Vm::new();
            hooked.execute(&hook).unwrap();
            ordinary.set_instruction_limit(Some(limit));
            hooked.set_instruction_limit(Some(limit));
            let ordinary = ordinary.execute(&program).map(|r| r.to_vec());
            let hooked = hooked.execute(&program).map(|r| r.to_vec());
            match (ordinary, hooked) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "limit {limit}"),
                (Err(a), Err(b)) => {
                    assert_eq!(a.kind(), b.kind(), "limit {limit}");
                    assert_eq!(a.line(), b.line(), "limit {limit}");
                    assert_eq!(a.message_bytes(), b.message_bytes(), "limit {limit}");
                }
                other => panic!("budget behavior differs at {limit}: {other:?}"),
            }
        }
    }
}
