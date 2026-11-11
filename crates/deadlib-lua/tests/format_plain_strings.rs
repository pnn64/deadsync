use deadlib_lua::{Value, Vm, compile};

fn string_results(source: &[u8]) -> Vec<Vec<u8>> {
    let program = compile(source).expect("compile string formatting probe");
    let mut vm = Vm::new();
    let results = vm.execute(&program).expect("run formatting probe").to_vec();
    results
        .into_iter()
        .map(|value| match value {
            Value::String(id) => vm.string_bytes(id).expect("live result string").to_vec(),
            other => panic!("expected string result, got {other:?}"),
        })
        .collect()
}

#[test]
fn plain_s_preserves_nul_rules_at_99_and_100_bytes() {
    let actual = string_results(
        br#"
        local nul = string.char(0)
        return string.format('%s', ''),
               string.format('%s', string.rep('a', 98) .. nul),
               string.format('%s', string.rep('a', 98) .. nul .. 'b'),
               string.format('%s', string.rep('a', 99) .. nul),
               string.format('%s', nul .. string.rep('b', 98)),
               string.format('%s', nul .. string.rep('b', 99)),
               string.format('%s', string.rep('q', 99)),
               string.format('%s', string.rep('q', 100)),
               string.format('%s', string.char(255) .. 'z')
    "#,
    );

    let mut hundred_with_middle_nul = vec![b'a'; 98];
    hundred_with_middle_nul.extend_from_slice(b"\0b");
    let mut hundred_with_trailing_nul = vec![b'a'; 99];
    hundred_with_trailing_nul.push(0);
    let mut hundred_with_leading_nul = vec![0];
    hundred_with_leading_nul.extend_from_slice(&[b'b'; 99]);
    assert_eq!(
        actual,
        vec![
            Vec::new(),
            vec![b'a'; 98],
            hundred_with_middle_nul,
            hundred_with_trailing_nul,
            Vec::new(),
            hundred_with_leading_nul,
            vec![b'q'; 99],
            vec![b'q'; 100],
            vec![255, b'z'],
        ]
    );
}

#[test]
fn plain_s_in_multi_argument_formats_preserves_coercion_and_other_specifiers() {
    let actual = string_results(
        br#"
        return string.format('(%s:%s:%s)', 'start', 42, 'end'),
               string.format('|%5s|', 'xy'),
               string.format('|%-5s|', 'xy'),
               string.format('%.2s', 'abcd'),
               string.format('%%-%s', 'ok'),
               string.format('%q', 'a\0b'),
               string.format('%s', string.rep('Z', 100))
    "#,
    );
    assert_eq!(
        actual,
        vec![
            b"(start:42:end)".to_vec(),
            b"|   xy|".to_vec(),
            b"|xy   |".to_vec(),
            b"ab".to_vec(),
            b"%-ok".to_vec(),
            b"\"a\\000b\"".to_vec(),
            vec![b'Z'; 100],
        ]
    );
}

#[test]
fn plain_s_preserves_bad_argument_errors_and_subsequent_calls() {
    assert_eq!(
        string_results(
            br#"
            local ok, error_text = pcall(string.format, '%s', {})
            assert(not ok and type(error_text) == 'string')
            return string.format('first %s', 'ok'), string.format('%s', 23)
        "#,
        ),
        vec![b"first ok".to_vec(), b"23".to_vec()]
    );
}
