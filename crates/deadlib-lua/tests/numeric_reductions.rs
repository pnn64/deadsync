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
fn array_extrema_preserve_float_order_and_fall_back_for_observable_reads() {
    let program = compile(
        br#"
        local min, max = math.min, math.max
        local function scan(values, first, last, step, sum, lower, upper)
            for i = first, last, step do
                local value = values[i]
                sum = sum + min(value, upper) + max(lower, value)
            end
            return sum
        end
        return function()
            local big = 9007199254740992
            local dense = {big, -big, 1, -2.25, 3.5, 5, 0}
            local forward = scan(dense, 1, 7, 1, 1.25, -big, big)
            local reverse = scan(dense, 7, 1, -1, 1.25, -big, big)
            local strided = scan(dense, 1, 7, 2, 0, -5, 4)
            local negative_zero = -1 / math.huge
            local zeros = {negative_zero, negative_zero, negative_zero}
            local negative = 1 / scan(zeros, 1, 3, 1, negative_zero, negative_zero, 0)
            local nan = 0 / 0
            local unordered = scan({2, nan, 1}, 1, 3, 1, 0, -4, 4)
            assert(unordered ~= unordered)

            array_reduction_reads = 0
            local sparse = setmetatable({1, nil, 3}, {__index = function(_, k)
                array_reduction_reads = array_reduction_reads + 1
                return k
            end})
            local observed = scan(sparse, 1, 3, 1, 0, -4, 4)
            assert(array_reduction_reads == 1 and observed == 12)
            local converted = scan({1, '2.5', 3}, 1, 3, 1, 0, -4, 4)
            local saved = min
            min = function(value, _) return 10 * value end
            local replaced = scan({1, 2, 3}, 1, 3, 1, 0, -4, 4)
            min = saved
            return forward, reverse, strided, negative, observed, converted, replaced
        end
        "#,
    )
    .unwrap();
    let mut vm = Vm::new();
    let work = vm.execute(&program).unwrap()[0];
    let work = vm.pin(work).unwrap();
    let fast = vm.call(vm.rooted(work).unwrap(), &[]).unwrap().to_vec();
    assert_eq!(fast[3], Value::Number(f64::NEG_INFINITY));
    assert_eq!(fast[5], Value::Number(13.0));
    assert_eq!(fast[6], Value::Number(66.0));
    vm.set_instruction_limit(Some(100_000));
    let ordinary = vm.call(vm.rooted(work).unwrap(), &[]).unwrap();
    assert_eq!(fast, ordinary);
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
