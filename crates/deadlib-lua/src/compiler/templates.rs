//! Reuse identical short leaf functions during a fresh compilation.
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

#[derive(Default)]
pub(super) struct Templates(Vec<Option<(Source, u32)>>);

impl Templates {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    pub(super) fn remember(
        &mut self,
        source: Source,
        id: u32,
        prototype: &Prototype,
    ) -> Result<(), Error> {
        if prototype.upvalues.is_empty()
            && prototype.bindings.is_empty()
            && prototype.code.iter().all(|i| i.op != Op::Closure)
            && prototype.template_lines_valid()
        {
            if self.0.is_empty() {
                self.0.try_resize(32, None)?;
            }
            self.0[source.slot()] = Some((source, id));
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
        let Some(&Some((cached, id))) = self.templates.0.get(source.slot()) else {
            return Ok(None);
        };
        if cached.fingerprint != source.fingerprint
            || (cached.offsets != source.offsets
                && cached.bytes(self.source) != source.bytes(self.source))
        {
            return Ok(None);
        }
        let prototype = self.prototypes[id as usize].reuse_leaf(id, line)?;
        let id = self.prototypes.len() as u32;
        self.prototypes.try_push(prototype)?;
        Ok(Some(id))
    }
}
