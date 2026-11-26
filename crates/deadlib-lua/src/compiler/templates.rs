//! Reuse identical short leaf functions in the compiler workspace.
use super::*;

#[derive(Clone, Copy)]
pub(super) struct Source {
    fingerprint: u64,
    offsets: [u32; 2],
}

impl Source {
    pub(super) fn new(input: &[u8], offsets: [u32; 2]) -> Option<Self> {
        (offsets[0] != offsets[1]).then(|| Self {
            fingerprint: crate::hash::hash_bytes(&input[offsets[0] as usize..offsets[1] as usize]),
            offsets,
        })
    }

    fn bytes(self, input: &[u8]) -> &[u8] {
        &input[self.offsets[0] as usize..self.offsets[1] as usize]
    }

    fn slot(self) -> usize {
        self.fingerprint as usize % 32
    }
}

struct CachedLeaf {
    source: Box<[u8]>,
    prototype: Prototype,
}

#[derive(Default)]
struct Slot {
    current: Option<(Source, u32)>,
    seen: Option<(u64, u64)>,
    cached: Option<CachedLeaf>,
}

#[derive(Default)]
pub(super) struct Templates(Vec<Slot>);

impl Templates {
    pub(super) fn begin(&mut self, locale: Option<u64>) {
        if let Some(locale) = locale {
            for slot in &mut self.0 {
                slot.current = None;
                if slot.seen.is_some_and(|(revision, _)| revision != locale) {
                    slot.seen = None;
                    slot.cached = None;
                }
            }
        } else {
            self.0.clear();
        }
    }

    pub(super) fn remember(
        &mut self,
        source: Source,
        id: u32,
        prototype: &mut Prototype,
        input: &[u8],
        locale: Option<u64>,
    ) -> Result<(), Error> {
        if prototype.upvalues.is_empty()
            && prototype.bindings.is_empty()
            && prototype.code.iter().all(|i| i.op != Op::Closure)
            && prototype.template_lines_valid()
        {
            if self.0.is_empty() {
                fallible::reserve(&mut self.0, 32)?;
                self.0.resize_with(32, Slot::default);
            }
            let slot = &mut self.0[source.slot()];
            if let Some(locale) = locale {
                let key = (locale, source.fingerprint);
                // Allocate retained code after a second sighting. Cache hits
                // still compare every byte of the source fragment.
                if slot.seen == Some(key) {
                    let bytes = source.bytes(input).try_boxed()?;
                    prototype.share_leaf(id)?;
                    slot.cached = Some(CachedLeaf {
                        source: bytes,
                        prototype: prototype.clone(),
                    });
                } else {
                    slot.seen = Some(key);
                    slot.cached = None;
                }
            }
            slot.current = Some((source, id));
        }
        Ok(())
    }
}

impl Compiler<'_> {
    pub(super) fn reuse_template(
        &mut self,
        source: Source,
        line: u32,
    ) -> Result<Option<u32>, Error> {
        let Some(slot) = self.templates.0.get_mut(source.slot()) else {
            return Ok(None);
        };
        let id = self.prototypes.len() as u32;
        if let Some((cached, origin)) = slot.current
            && cached.fingerprint == source.fingerprint
            && (cached.offsets == source.offsets
                || cached.bytes(self.source) == source.bytes(self.source))
        {
            let prototype = self.prototypes[origin as usize].reuse_leaf(origin, line)?;
            self.prototypes.try_push(prototype)?;
            return Ok(Some(id));
        }
        if let Some(locale) = self.template_locale
            && let Some(cached) = &slot.cached
            && slot.seen == Some((locale, source.fingerprint))
            && *cached.source == *source.bytes(self.source)
        {
            let prototype = cached.prototype.shared_leaf_for_owner(id, line);
            self.prototypes.try_push(prototype)?;
            slot.current = Some((source, id));
            return Ok(Some(id));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Value, Vm, parser};

    #[test]
    fn reused_bodies_take_the_new_owner_and_lines_and_respect_captures() {
        fn compile(spare: &mut Spare, source: &str, fragmented: bool, locale: u64) -> Program {
            let mut parser = parser::Parser::try_new_in(&mut parser::Spare::default(), 0).unwrap();
            if fragmented {
                for end in (17..source.len()).step_by(17) {
                    assert!(
                        parser
                            .resume(&source.as_bytes()[..end], false)
                            .unwrap()
                            .is_none()
                    );
                }
            }
            let mut ast = parser.resume(source.as_bytes(), true).unwrap().unwrap();
            compile_in(&mut ast, source.as_bytes(), b"=leaf", spare, locale).unwrap()
        }

        let padding = "a".repeat(2048);
        let leaf = "function(v)\n    return v + value\nend";
        let locale = crate::locale::revision();
        let mut spare = Spare::default();
        let first = compile(
            &mut spare,
            &format!("--{padding}\nreturn {leaf}"),
            false,
            locale,
        );
        assert_eq!(first.data.prototypes[0].shared_owner(), None);
        let warm = compile(
            &mut spare,
            &format!("--{padding}\n\nreturn {leaf}"),
            false,
            locale,
        );
        assert_eq!(warm.data.prototypes[0].shared_owner(), Some(0));

        let moved = format!(
            "--{padding}\nlocal other = 11\nlocal function unused() return other end\nreturn {leaf},\n{leaf}"
        );
        let rebased = compile(&mut spare, &moved, true, locale);
        for (index, line) in [(1, 4), (2, 7)] {
            let prototype = &rebased.data.prototypes[index];
            assert_eq!(prototype.shared_owner(), Some(1));
            assert_eq!(prototype.defined, [line, line + 2]);
            assert_eq!(prototype.debug_line(0), i64::from(line + 1));
        }

        let captured = compile(
            &mut spare,
            &format!("--{padding}\nlocal value = 9\nreturn {leaf}"),
            false,
            locale,
        );
        assert_eq!(captured.data.prototypes[0].upvalues.len(), 1);
        let changed_locale = compile(&mut spare, &moved, true, locale.wrapping_sub(1));
        assert_eq!(changed_locale.data.prototypes[1].shared_owner(), None);
        assert_eq!(changed_locale.data.prototypes[2].shared_owner(), None);
        drop(spare);

        let mut vm = Vm::new();
        vm.set_global(b"value", Value::Number(5.0)).unwrap();
        let [a, b] = *vm.execute(&rebased).unwrap() else {
            panic!("both relocated leaves");
        };
        let second = vm.pin(b).unwrap();
        assert_eq!(
            vm.call(a, &[Value::Number(37.0)]).unwrap(),
            [Value::Number(42.0)]
        );
        assert_eq!(
            vm.call(vm.rooted(second).unwrap(), &[Value::Number(37.0)])
                .unwrap(),
            [Value::Number(42.0)]
        );
        let callback = vm.execute(&captured).unwrap()[0];
        assert_eq!(
            vm.call(callback, &[Value::Number(33.0)]).unwrap(),
            [Value::Number(42.0)]
        );
    }
}
