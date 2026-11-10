use super::*;

#[test]
fn numeric_writes_do_not_intern_temporary_text() {
    let mut vm = Vm::new();
    vm.set_output_capture(true);
    vm.execute(&crate::compile(b"collectgarbage('stop')").unwrap())
        .unwrap();
    let number = b"12345678901234";
    assert!(vm.strings.find(number).is_none());
    let program = crate::compile(b"return io.write(12345678901234, '|', -0.0, '|', 1e14)").unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Boolean(true)]);
    assert_eq!(vm.output().unwrap(), b"12345678901234|-0|1e+14");
    assert!(vm.strings.find(number).is_none());
}

#[test]
fn integer_formatting_preserves_precision_boundaries_and_signed_zero() {
    for (number, expected) in [
        (0.0, "0"),
        (-0.0, "-0"),
        (1.5, "1.5"),
        (99_999_999_999_999.0, "99999999999999"),
        (-99_999_999_999_999.0, "-99999999999999"),
        (100_000_000_000_000.0, "1e+14"),
        (-100_000_000_000_000.0, "-1e+14"),
        (100_000_000_000_001.0, "1e+14"),
        (99_999_999_999_999.5, "1e+14"),
        (9_999_999_999_999.5, "9999999999999.5"),
        (0.00009999999999999995, "0.0001"),
    ] {
        assert_eq!(
            native::try_format_number(number).unwrap(),
            expected.as_bytes()
        );
    }
}
