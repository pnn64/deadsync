#![cfg(any(
    all(windows, target_env = "msvc"),
    all(target_os = "linux", target_env = "gnu")
))]
use deadlib_lua::{ErrorKind, Value, Vm, compile_named};
#[path = "support/allocation_probe.rs"]
mod allocation_probe;
use allocation_probe::{probe, reject_from_now, statistics};

const PREPARE: &[u8] = br#"
    assert(os.setlocale('C'))
    c_numeric=os.setlocale(nil,'numeric');c_number=tostring(1.5)
    c_ctype=os.setlocale(nil,'ctype');c_upper=string.upper(string.char(228))
    c_collate=os.setlocale(nil,'collate');c_order=string.char(228)<'z'
    assert(os.setlocale(chosen),'required locale fixture unavailable')
    other_numeric=os.setlocale(nil,'numeric');other_number=tostring(1.5)
    other_ctype=os.setlocale(nil,'ctype');other_upper=string.upper(string.char(228))
    other_collate=os.setlocale(nil,'collate');other_order=string.char(228)<'z'
    assert(os.setlocale('C'))
"#;
const VERIFY: &[u8] = br#"
    local n=os.setlocale(nil,'numeric')
    assert(n==c_numeric or n==other_numeric)
    local number=n==c_numeric and c_number or other_number
    assert(tostring(1.5)==number and tonumber(number)==1.5)
    local c=os.setlocale(nil,'ctype')
    assert(c==c_ctype or c==other_ctype)
    assert(string.upper(string.char(228))==(c==c_ctype and c_upper or other_upper))
    local s=os.setlocale(nil,'collate')
    assert(s==c_collate or s==other_collate)
    local order=c_order;if s~=c_collate then order=other_order end
    assert((string.char(228)<'z')==order)
    return 42
"#;

#[derive(Debug)]
enum Outcome {
    Error(ErrorKind, Option<Value>),
    Returned(Value),
}

fn run(setup: &[u8], source: &[u8], runtime_error: bool, nth: usize, locale: &[u8]) -> usize {
    let (attempts, allocations) = probe(0, || {
        let mut vm = Vm::try_new().unwrap();
        let chosen = vm.try_intern(locale).unwrap();
        vm.set_global(b"chosen", chosen).unwrap();
        let prepare = compile_named(PREPARE, b"=locale-prepare").unwrap();
        let setup = compile_named(setup, b"=locale-setup").unwrap();
        let source = compile_named(source, b"@locale-rejection.lua").unwrap();
        let verify = compile_named(VERIFY, b"=locale-verify").unwrap();
        let reset =
            compile_named(b"assert(os.setlocale('C'));return 42", b"=locale-reset").unwrap();
        vm.execute(&prepare).unwrap();
        vm.execute(&setup).unwrap();
        vm.try_load_program(&source).unwrap();
        vm.try_load_program(&verify).unwrap();
        vm.try_load_program(&reset).unwrap();
        let start = statistics().attempts;
        reject_from_now(nth);
        let result = match vm.execute(&source) {
            Ok(values) => Outcome::Returned(values.first().copied().unwrap_or(Value::Nil)),
            Err(error) => Outcome::Error(error.kind(), error.value()),
        };
        let attempts = statistics().attempts - start;
        reject_from_now(0);
        if nth == 0 {
            match (runtime_error, result) {
                (true, Outcome::Error(ErrorKind::Runtime, _))
                | (false, Outcome::Returned(Value::Number(42.))) => {}
                (_, result) => panic!("unexpected healthy result: {result:?}"),
            }
        } else {
            assert!(statistics().rejected > 0);
            let Outcome::Error(ErrorKind::Memory, Some(Value::String(id))) = result else {
                panic!("unexpected rejection {nth}: {result:?}");
            };
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
            vm.collect_garbage().unwrap();
            assert_eq!(vm.string_bytes(id), Some(b"not enough memory".as_slice()));
        }
        // A native selection may already have taken effect. Its queried name
        // and numeric/ctype/collation caches must still describe the same state.
        assert_eq!(vm.execute(&verify).unwrap(), &[Value::Number(42.)]);
        if runtime_error {
            assert_eq!(vm.execute(&source).unwrap_err().kind(), ErrorKind::Runtime);
        } else {
            assert_eq!(vm.execute(&source).unwrap(), &[Value::Number(42.)]);
        }
        assert_eq!(vm.execute(&reset).unwrap(), &[Value::Number(42.)]);
        attempts
    });
    assert_eq!(
        allocations.allocated_bytes, allocations.freed_bytes,
        "locale storage leaked at {nth}"
    );
    attempts
}

#[test]
fn locale_selection_queries_and_collation_recover_with_consistent_caches() {
    let default = if cfg!(windows) {
        "French_France.1252"
    } else {
        "C.UTF-8"
    };
    let locale = std::env::var("DEADLUA_TEST_LOCALE").unwrap_or_else(|_| default.into());
    let cases: &[(&[u8], &[u8], bool)] = &[
        (b"", b"return os.setlocale()=='C' and 42", false),
        (b"", b"return os.setlocale('C')=='C' and 42", false),
        (b"", b"return os.setlocale('C\\0ignored','numeric\\0ignored')=='C' and 42", false),
        (b"", b"return os.setlocale('deadlua-invalid-locale')==nil and 42", false),
        (b"", b"os.setlocale(false)", true),
        (b"", b"os.setlocale('C','invalid-category')", true),
        (b"", b"os.setlocale('C',false)", true),
        (b"", b"return type(os.setlocale(chosen))=='string' and 42", false),
        (b"assert(os.setlocale(chosen))", b"return os.setlocale('C')=='C' and 42", false),
        (b"assert(os.setlocale(chosen,'ctype'))", b"local saved=os.setlocale();assert(os.setlocale('C'));return os.setlocale(saved)==saved and 42", false),
        (b"assert(os.setlocale(chosen))", b"return ('a'<'z') and ('a'<= 'a') and ('z'>'a') and 42", false),
        (b"assert(os.setlocale(chosen))", b"return ('a\\0b'<'a\\0c') and ('a\\0b'<='a\\0b') and 42", false),
        (b"assert(os.setlocale(chosen))", b"local t={'z','a\\0c','a\\0b'};table.sort(t);return t[1]=='a\\0b' and t[2]=='a\\0c' and t[3]=='z' and 42", false),
    ];
    let mut total = 0;
    for (index, &(setup, source, runtime_error)) in cases.iter().enumerate() {
        let count = run(setup, source, runtime_error, 0, locale.as_bytes());
        for nth in 1..=count {
            eprintln!("locale case={index} rejection={nth}");
            run(setup, source, runtime_error, nth, locale.as_bytes());
        }
        total += count;
    }
    for category in ["all", "collate", "ctype", "monetary", "numeric", "time"] {
        for source in [
            format!("return os.setlocale(nil,'{category}')=='C' and 42"),
            format!("return type(os.setlocale(chosen,'{category}'))=='string' and 42"),
        ] {
            let count = run(b"", source.as_bytes(), false, 0, locale.as_bytes());
            for nth in 1..=count {
                eprintln!("locale category={category} rejection={nth}");
                run(b"", source.as_bytes(), false, nth, locale.as_bytes());
            }
            total += count;
        }
    }
    eprintln!(
        "locale recovered {total} allocation failures across {} cases ({locale})",
        cases.len() + 12
    );
}
