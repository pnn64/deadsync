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
fn dense_writes_match_dispatch_across_growth_and_gc_checkpoints() {
    let program = compile(
        br#"
        local function build(t, first, last)
            for i = first, last do t[i] = i end
        end
        local function fill(t, first, last, value)
            for i = first, last do t[i] = value end
        end
        local sum = 0
        for round = 1, 80 do
            local t = {1}
            local last = 311 + round
            build(t, 2, last)
            assert(#t == last)
            for i = 1, last do assert(t[i] == i) end
            fill(t, 3, last + 2, false)
            assert(#t == last + 2 and t[1] == 1 and t[2] == 2)
            for i = 3, last + 2 do assert(t[i] == false) end
            sum = sum + #t
        end
        return sum
        "#,
    )
    .unwrap();

    let mut fast = Vm::new();
    let mut ordinary = Vm::new();
    ordinary.set_instruction_limit(Some(2_000_000));
    assert_eq!(
        fast.execute(&program).unwrap(),
        ordinary.execute(&program).unwrap()
    );
    let fast_heap = fast.heap_stats();
    let ordinary_heap = ordinary.heap_stats();
    assert!(fast_heap.collections > 0);
    assert_eq!(fast_heap.collections, ordinary_heap.collections);
    assert_eq!(fast_heap.reclaimed_objects, ordinary_heap.reclaimed_objects);
}

#[test]
fn dense_sequences_keep_independent_tables_when_mutated() {
    assert_eq!(
        numbers(
            br#"
            local function sequence(last)
                local t = {}
                for i = 1, last do t[i] = i end
                return t
            end
            local a, b = sequence(157), sequence(157)
            assert(a ~= b)
            local c = sequence(157)
            assert(table.remove(c, 3) == 3 and c[3] == 4)
            table.insert(c, 3, 3)
            table.sort(c, function(x, y) return x > y end)
            assert(c[1] == 157 and c[157] == 1)

            a[80] = {value = 42}
            a[159] = 159
            a[158] = 158
            setmetatable(b, {__mode = 'v'})
            b.label = {}
            collectgarbage('collect')
            assert(a[80].value == 42 and a[159] == 159)
            assert(b.label == nil and b[80] == 80 and b[158] == nil)
            local visited = 0
            for key, value in pairs(b) do
                assert(key == value)
                visited = visited + 1
            end
            return #a, #b, visited
            "#
        ),
        [159.0, 157.0, 157.0]
    );
}

#[test]
fn dense_writes_preserve_metatable_and_hash_stores() {
    assert_eq!(
        numbers(
            br#"
            local function write(t, first, last)
                for i = first, last do t[i] = i end
            end
            local seen = 0
            local proxy = setmetatable({}, {__newindex = function(_, key, value)
                assert(key == value)
                seen = seen + value
            end})
            write(proxy, 1, 25)
            assert(next(proxy) == nil)

            local t = {[10] = 99, label = true}
            write(t, 1, 9)
            assert(t[10] == 99 and t.label)
            write(t, 10, 12)
            assert(t[10] == 10 and t[11] == 11 and t[12] == 12)
            return #t, seen
            "#
        ),
        [12.0, 325.0]
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
