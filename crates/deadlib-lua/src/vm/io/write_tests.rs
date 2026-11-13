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

#[test]
fn numeric_text_preserves_rounding_without_temporary_allocations() {
    for (number, expected) in [
        (-0.0, "-0"),
        (0.29f64.next_down(), "0.29"),
        (-0.000125, "-0.000125"),
        (123.45000000000001, "123.45"),
        (1.234567890123456, "1.2345678901235"),
        (9.99999999999996, "10"),
        (0.00009999999999999996, "0.0001"),
        (0.000009999999999999996, "1e-05"),
        (9_999_999_999_999.9, "9999999999999.9"),
        (99_999_999_999_999.5, "1e+14"),
    ] {
        let (text, allocations) = crate::allocation_probe::probe(1, || native::number_text(number));
        assert_eq!(allocations.attempts, 0);
        assert_eq!(text.as_bytes(), expected.as_bytes());
    }
}
