#[path = "support/allocation_probe.rs"]
mod allocation_probe;

use allocation_probe::probe;
use deadlib_lua::{ErrorKind, compile_named};

fn outcome(source: &[u8]) -> Result<usize, ErrorKind> {
    compile_named(source, b"=allocation-recovery")
        .map(|program| program.instruction_count())
        .map_err(|error| error.kind())
}

#[test]
fn compilation_returns_memory_errors_and_releases_partial_programs() {
    let mut cases: Vec<Vec<u8>> = [
        b"".as_slice(),
        b"return 6*7",
        b"local a,b=1,2; local function f(x,...) return function(y) return a+b+x+y, arg end end; return f",
        b"local t={x='a', [2]='b', 3, 4}; function t:m(x) self.x=x end; t:m('y'); return t.x",
        b"local x=0; for i=1,10,2 do if i>7 then break elseif i==3 then x=x+i else x=x-i end end; return x",
        b"local t={1,2}; for k,v in pairs(t) do t[k]=v+1 end; while t[1]<5 do t[1]=t[1]+1 end; repeat t[1]=t[1]-1 until t[1]==0; return t",
        b"return a and b or c, not (d or e), -x^2, a.b:c(1,2), f(g(...))",
        b"local a,b,c; a,b,a[c]=f(),2,3; return a,b,c",
        b"local = 'malformed'",
        b"return 'unterminated\xff",
    ].into_iter().map(<[u8]>::to_vec).collect();
    // Cross indexed-constant and name-pool growth thresholds without reaching
    // Lua's register limit. Include binary strings and both numeric zero signs.
    let mut pool = String::from("local t={};\n");
    for n in 0..150 {
        pool.push_str(&format!("t['key{n}']='value{n}\\255'; t[{n}]=-{n};\n"));
    }
    pool.push_str("return t,0,-0");
    cases.push(pool.into_bytes());
    let mut rejected = 0;
    for (case, source) in cases.iter().enumerate() {
        let (healthy, count) = probe(0, || outcome(source));
        assert_eq!(
            healthy.is_ok(),
            !matches!(case, 8 | 9),
            "case {case}: {healthy:?}"
        );
        assert_eq!(
            count.allocated_bytes, count.freed_bytes,
            "healthy case {case}"
        );
        for nth in 1..=count.attempts {
            eprintln!("compiler allocation case={case} nth={nth}");
            let (result, stats) = probe(nth, || outcome(source));
            assert_eq!(
                result,
                Err(ErrorKind::Memory),
                "case {case}, allocation {nth}"
            );
            assert!(stats.rejected > 0);
            assert_eq!(
                stats.allocated_bytes, stats.freed_bytes,
                "case {case}, allocation {nth}"
            );
            rejected += 1;
        }
        assert_eq!(outcome(source), healthy);
    }
    eprintln!("compiler recovered {rejected} allocation failures");
}

#[test]
fn native_chunk_import_recovers_at_every_allocation() {
    let mut cases = Vec::new();
    for source in [
        "return 42",
        "local x=...; local t={a='binary\\255', 1, 2}; function t:m(y) return self.a,y end; return function(z) return x+z,t:m(3) end",
        "local t={}; for i=1,20 do t[i]=i end; for k,v in pairs(t) do if v>4 then break end end; repeat t[1]=t[1]+1 until t[1]>3; return t",
    ] {
        for _stripped in [false, true] {
            let bytes = reference_test::native_chunk(
                "tests/compiler_allocation_failures.rs::native_chunk_import_recovers_at_every_allocation::chunk0",
                &(source),
            );
            cases.push((bytes, true));
        }
    }
    let sample = cases[1].0.clone();
    for end in [1, 11, sample.len() / 2, sample.len() - 1] {
        cases.push((sample[..end].to_vec(), false));
    }
    let mut bad = sample.clone();
    bad[4] = 0xff;
    cases.push((bad, false));
    let mut rejected = 0;
    for (case, (source, valid)) in cases.iter().enumerate() {
        let (healthy, count) = probe(0, || outcome(source));
        assert_eq!(healthy.is_ok(), *valid, "case {case}: {healthy:?}");
        assert_eq!(
            count.allocated_bytes, count.freed_bytes,
            "healthy case {case}"
        );
        for nth in 1..=count.attempts {
            eprintln!("native import allocation case={case} nth={nth}");
            let (result, stats) = probe(nth, || outcome(source));
            assert_eq!(
                result,
                Err(ErrorKind::Memory),
                "case {case}, allocation {nth}"
            );
            assert!(stats.rejected > 0);
            assert_eq!(
                stats.allocated_bytes, stats.freed_bytes,
                "case {case}, allocation {nth}"
            );
            rejected += 1;
        }
        assert_eq!(outcome(source), healthy);
    }
    eprintln!("native import recovered {rejected} allocation failures");
}

#[allow(dead_code)]
#[path = "../src/reference_test.rs"]
mod reference_test;
