use super::*;
use crate::allocation_probe::probe;

// The AST and diagnostic must both be dropped while the accounting probe is
// armed, so byte balance covers every successful allocation made by the parser.
fn outcome(source: &[u8], streamed: bool, roots: bool) -> Result<(), ErrorKind> {
    let run = || -> Result<(), Error> {
        let mut parser = Parser::try_new()?;
        if roots {
            parser.enable_native_roots()?;
        }
        if streamed {
            for end in 0..source.len() {
                assert!(parser.resume(&source[..end], false)?.is_none());
                drop(parser.take_native_events());
            }
        }
        drop(parser.resume(source, true)?.expect("complete source"));
        Ok(())
    };
    run().map_err(|error| error.kind())
}

#[test]
fn parser_recovers_and_releases_partial_state_at_every_allocation() {
    let cases: &[(&[u8], bool)] = &[
        (b"", true),
        (b"return 6*7", true),
        (b"local alpha,beta=1,2; return alpha+beta,global", true),
        (b"local a=1; return function(b,...) return function(c) return a+b+c,arg end end", true),
        (b"local t={a='hello', [2]=[=[long\r\nstring]=], 3, 4}; function t:m(x) self.a=x end; t:m('x\\n\\255'); return t.a", true),
        (b"for i=1,10,2 do if i<3 then break elseif i>8 then f(i) else do local x=i end end end; for k,v in pairs(t) do t[k]=v end", true),
        (b"local a=0; while a<10 do a=a+1 end; repeat local x=a; a=x-1 until a==0; return not a or a and 3, -2^3 .. 'x'", true),
        (b"local function f(a,...) return a,f(...) end; a.b.c=function() return nil,true,false end; return f{}", true),
        (b"return 'unterminated", false),
        (b"return '\\999'", false),
        (b"return [=[unterminated", false),
        (b"return [==broken", false),
        (b"return 0xnot_a_number", false),
        (b"local = 2", false),
        (b"if true then\nreturn 1", false),
        (b"local x 'bad\\255'", false),
        (b"local x [=[bad\xff]=]", false),
        (b"local x \x01", false),
        (b"break", false),
        (b"return function() return ... end", false),
    ];
    let mut rejected = 0;
    for (case, &(source, valid)) in cases.iter().enumerate() {
        for (streamed, roots) in [(false, false), (true, false), (true, true)] {
            let (healthy, count) = probe(0, || outcome(source, streamed, roots));
            assert_eq!(healthy.is_ok(), valid, "case {case}: {healthy:?}");
            assert_eq!(
                count.allocated_bytes, count.freed_bytes,
                "healthy case {case}"
            );
            for nth in 1..=count.attempts {
                // Printed before arming, so an accidental process-aborting
                // allocation still leaves the failing case and index in logs.
                eprintln!(
                    "parser allocation case={case} streamed={streamed} roots={roots} nth={nth}"
                );
                let (result, stats) = probe(nth, || outcome(source, streamed, roots));
                assert_eq!(result, Err(ErrorKind::Memory));
                assert!(stats.rejected > 0);
                assert_eq!(
                    stats.allocated_bytes, stats.freed_bytes,
                    "case {case}, allocation {nth}"
                );
                rejected += 1;
            }
            assert_eq!(outcome(source, streamed, roots), healthy);
        }
    }
    eprintln!("parser recovered {rejected} allocation failures");
}
