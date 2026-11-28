//! Short leaf functions with a local receiver and folded numeric arguments.
//! The signature includes the equality pattern of the arguments, which keeps
//! constant numbering and every instruction operand identical on a hit.
use super::*;

const SLOTS: usize = 16;
const PROBE_SLOTS: usize = 4;
const MAX_CALLS: usize = 4;
const MAX_NUMBERS: usize = 8;

#[derive(Default)]
pub(super) struct NumericTemplates {
    slots: Vec<Option<Cached>>,
    locale: Option<u64>,
    // An eligible miss has no nested functions, so its next completed
    // prototype supplies the body for this signature.
    pending: Option<(usize, Key, Arguments)>,
}

struct Cached {
    key: Key,
    prototype: Prototype,
    owner: Option<u32>,
    number_slots: [u8; MAX_NUMBERS],
}

#[derive(PartialEq, Eq)]
struct Key {
    bytes: [u8; 96],
    len: usize,
}
impl Key {
    fn push(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.len + bytes.len();
        self.bytes.get_mut(self.len..end)?.copy_from_slice(bytes);
        self.len = end;
        Some(())
    }

    fn name(&mut self, name: &[u8]) -> Option<()> {
        self.push(&[u8::try_from(name.len()).ok()?])?;
        self.push(name)
    }

    fn slot(&self) -> usize {
        crate::hash::hash_bytes(&self.bytes[..self.len]) as usize % SLOTS
    }
}

struct Arguments {
    numbers: [f64; MAX_NUMBERS],
    len: usize,
}
impl Arguments {
    fn number(&mut self, n: f64) -> Option<u8> {
        // Finite nonzero values have one encoding per equality class.
        if !n.is_finite() || n == 0.0 {
            return None;
        }
        if let Some(index) = self.numbers[..self.len].iter().position(|&v| v == n) {
            return Some(index as u8);
        }
        *self.numbers.get_mut(self.len)? = n;
        self.len += 1;
        Some((self.len - 1) as u8)
    }
}

impl NumericTemplates {
    fn find_slot(&self, key: &Key) -> (usize, bool) {
        let home = key.slot();
        // Keep nearby signatures that hash to the same slot. If the probe
        // range is full, the home slot remains the replacement candidate.
        for offset in 0..PROBE_SLOTS {
            let index = (home + offset) % SLOTS;
            match self.slots.get(index).and_then(Option::as_ref) {
                None => return (index, false),
                Some(cached) if &cached.key == key => return (index, true),
                _ => {}
            }
        }
        (home, false)
    }

    pub(super) fn begin(&mut self, locale: Option<u64>) {
        debug_assert!(self.pending.is_none());
        if locale != self.locale {
            self.slots.clear();
            self.locale = locale;
        } else {
            for cached in self.slots.iter_mut().flatten() {
                cached.owner = None;
            }
        }
    }
}

impl Compiler<'_> {
    fn numeric_signature(&self, id: ExprId, stack: u16) -> Option<(Key, Arguments)> {
        self.template_locale?;
        let (expr, span) = &self.ast.exprs[id];
        let Expr::Function {
            params,
            vararg: false,
            body,
            end_line,
            template_line: 0,
            ..
        } = *expr
        else {
            return None;
        };
        if span.start != end_line {
            return None;
        }
        let [param] = self.ast.names(params) else {
            return None;
        };
        let [statement] = self.ast.ids(body) else {
            return None;
        };
        let Stmt::Call(mut call) = self.ast.stmts[*statement].0 else {
            return None;
        };
        let mut key = Key {
            bytes: [0; 96],
            len: 0,
        };
        key.push(&stack.to_ne_bytes())?;
        key.name(param.bytes(self.source))?;
        let mut args = Arguments {
            numbers: [0.0; MAX_NUMBERS],
            len: 0,
        };
        for _ in 0..MAX_CALLS {
            let Expr::Method(receiver, method, arguments, _) = &self.ast.exprs[call].0 else {
                return None;
            };
            let arguments = self.ast.ids(*arguments);
            key.name(method.bytes(self.source))?;
            key.push(&[u8::try_from(arguments.len()).ok()?])?;
            for &argument in arguments {
                key.push(&[args.number(self.folded[argument]?)?])?;
            }
            let (receiver_expr, receiver_span) = &self.ast.exprs[*receiver];
            if matches!(receiver_expr, Expr::Name(_)) {
                return (args.len != 0 && receiver_span.binding.local_index() == Some(0))
                    .then_some((key, args));
            }
            call = *receiver;
        }
        None
    }

    pub(super) fn reuse_numeric_template(
        &mut self,
        function: ExprId,
        stack: u16,
        line: u32,
    ) -> Result<Option<u32>, Error> {
        let Some((key, args)) = self.numeric_signature(function, stack) else {
            return Ok(None);
        };
        let (slot, hit) = self.numeric_templates.find_slot(&key);
        if !hit {
            debug_assert!(self.numeric_templates.pending.is_none());
            self.numeric_templates.pending = Some((slot, key, args));
            return Ok(None);
        }
        let cached = self.numeric_templates.slots[slot].as_mut().unwrap();
        let constants = cached.prototype.constants();
        let mut values = fallible::with_capacity(constants.len())?;
        values.extend(constants.iter().map(|constant| match constant {
            Constant::PooledString { start, end } => Constant::PooledString {
                start: *start,
                end: *end,
            },
            _ => Constant::Nil,
        }));
        for (index, &number) in args.numbers[..args.len].iter().enumerate() {
            values[usize::from(cached.number_slots[index])] = Constant::Number(number);
        }
        let id = self.prototypes.len() as u32;
        let owner = *cached.owner.get_or_insert(id);
        let prototype =
            cached
                .prototype
                .substitute_leaf_constants(owner, line, values.try_into_boxed_slice()?);
        self.prototypes.try_push(prototype)?;
        Ok(Some(id))
    }

    pub(super) fn remember_numeric_template(&mut self, child: u32) -> Result<(), Error> {
        let Some((slot, key, args)) = self.numeric_templates.pending.take() else {
            return Ok(());
        };
        let prototype = &self.prototypes[child as usize];
        if !prototype.upvalues.is_empty()
            || !prototype.bindings.is_empty()
            || !prototype.template_lines_valid()
        {
            return Ok(());
        }
        let mut number_slots = [u8::MAX; MAX_NUMBERS];
        for (id, constant) in prototype.constants().iter().enumerate() {
            match constant {
                Constant::Number(n) => {
                    let Some(index) = args.numbers[..args.len].iter().position(|v| v == n) else {
                        return Ok(());
                    };
                    let Ok(id) = u8::try_from(id) else {
                        return Ok(());
                    };
                    if std::mem::replace(&mut number_slots[index], id) != u8::MAX {
                        return Ok(());
                    }
                }
                Constant::PooledString { .. } => {}
                _ => return Ok(()),
            }
        }
        if number_slots[..args.len].contains(&u8::MAX) {
            return Ok(());
        }
        let slots = &mut self.numeric_templates.slots;
        if slots.is_empty() {
            fallible::reserve(slots, SLOTS)?;
            slots.resize_with(SLOTS, || None);
        }
        let prototype = &mut self.prototypes[child as usize];
        prototype.share_leaf(child)?;
        slots[slot] = Some(Cached {
            key,
            prototype: prototype.clone(),
            owner: Some(child),
            number_slots,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn colliding_numeric_signatures_share_their_own_code() {
        let source = format!(
            "return function(item) item:size(10, 20):tilt(0.25) end,\n\
             function(item) item:wait(0.5):turn(-10) end,\n\
             function(item) item:size(13, 23):tilt(0.25) end,\n\
             function(item) item:wait(0.5):turn(-30) end\n--{}",
            "x".repeat(2200)
        );
        let name = b"@colliding-callbacks.lua";
        let reference = crate::compile_named(source.as_bytes(), name).unwrap();
        let mut vm = crate::Vm::new();
        let program = vm.compile_source(source.as_bytes(), name).unwrap();
        let bytes = |program: &crate::Program| {
            crate::chunk51::dump(&program.data, program.data.entry as usize).unwrap()
        };
        assert_eq!(bytes(&program), bytes(&reference));
        let children = &program.data.prototypes[..4];
        assert_eq!(children[0].code().as_ptr(), children[2].code().as_ptr());
        assert_eq!(children[1].code().as_ptr(), children[3].code().as_ptr());
        assert_ne!(children[0].code().as_ptr(), children[1].code().as_ptr());
    }
}
