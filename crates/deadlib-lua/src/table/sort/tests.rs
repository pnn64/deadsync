use super::*;
use crate::{Vm, compile};
use std::fmt::Write;

#[test]
fn partitions_preserve_the_interpreted_comparison_trace_and_error_permutation() {
    let mut random = 19_u32;
    for len in 0..48 {
        let input: Vec<_> = (0..len)
            .map(|_| {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                Value::Number(f64::from((random >> 24) % 17) - 8.)
            })
            .collect();
        let text = |value| match value {
            Value::Number(number) => number.to_string(),
            Value::Nil => "nil".to_string(),
            _ => unreachable!("numeric input and nil boundary"),
        };
        let array = input
            .iter()
            .copied()
            .map(text)
            .collect::<Vec<_>>()
            .join(",");
        for predicate in ["a<b", "b<a", "false", "true"] {
            let mut values = input.clone();
            let mut trace = String::new();
            let result = scalar(&mut values, Value::Nil, Value::Nil, |a, b| {
                write!(trace, "{}:{};", text(a), text(b)).unwrap();
                match predicate {
                    "true" => true,
                    "false" => false,
                    _ => {
                        let (Value::Number(a), Value::Number(b)) = (a, b) else {
                            unreachable!("strict comparison remains inside numeric input")
                        };
                        if predicate == "a<b" { a < b } else { b < a }
                    }
                }
            });
            // The logging callback requires ordinary Lua frames, providing an
            // independent reference for every comparison, swap and failure.
            let source = format!(
                "local t={{{array}}};local trace='';\
                 local ok=pcall(table.sort,t,function(a,b)\
                 trace=trace..tostring(a)..':'..tostring(b)..';';return {predicate} end);\
                 return ok,trace,table.concat(t,',')"
            );
            let mut vm = Vm::new();
            let returned = vm
                .execute(&compile(source.as_bytes()).unwrap())
                .unwrap()
                .to_vec();
            assert_eq!(
                returned[0],
                Value::Boolean(result.is_ok()),
                "{len}: {predicate}"
            );
            let Value::String(trace_id) = returned[1] else {
                panic!("comparison trace")
            };
            assert_eq!(
                vm.string_bytes(trace_id).unwrap(),
                trace.as_bytes(),
                "{len}: {predicate}"
            );
            let Value::String(array_id) = returned[2] else {
                panic!("partial array")
            };
            let expected = values
                .iter()
                .copied()
                .map(text)
                .collect::<Vec<_>>()
                .join(",");
            assert_eq!(
                vm.string_bytes(array_id).unwrap(),
                expected.as_bytes(),
                "{len}: {predicate}"
            );
        }
    }
}
