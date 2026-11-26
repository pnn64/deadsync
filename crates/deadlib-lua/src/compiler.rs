//! Register allocation and Lua 5.1 bytecode emission.

use crate::memory::fallible::Map as HashMap;
use crate::memory::fallible::{self, SliceExt, VecExt};
use crate::{
    Error, ErrorKind,
    bytecode::{
        BOOLEAN_SKIP, COMPARE_BRANCH, Constant, DebugCall, DebugInfo, DebugLocal, DebugName,
        Instruction, InstructionMetadata, NUMERIC_WHILE, NameKind, OPEN, OPEN_ARGS, Op, Program,
        Prototype, PrototypeBody, RK, RegisterMaps, Upvalue,
    },
    lexer::{Kind, Lexeme},
    parser::{Ast, Expr, ExprId, NameBinding, Stmt, StmtId},
};

mod boolean;
mod constants;
mod expressions;
mod numeric_templates;
mod statements;
mod tasks;
mod templates;
use tasks::Task;

/// Bytes of a local or upvalue name in the compiler's name pool.
#[derive(Clone, Copy)]
struct Name {
    start: u32,
    len: u32,
    /// The first eight bytes, zero padded: most lookups reject without the pool.
    head: u64,
}
fn name_head(name: &[u8]) -> u64 {
    let mut bytes = [0; 8];
    let len = name.len().min(8);
    bytes[..len].copy_from_slice(&name[..len]);
    u64::from_le_bytes(bytes)
}
impl Name {
    fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.start as usize + self.len as usize
    }
    /// Compare with `name`, whose `name_head` is `head`.
    fn equals(self, pool: &[u8], name: &[u8], head: u64) -> bool {
        self.head == head
            && self.len as usize == name.len()
            && (name.len() <= 8 || pool[self.range()] == *name)
    }
}
struct Local {
    name: Name,
    reg: u8,
    debug: usize,
    captured: bool,
}
const TAIL: u32 = OPEN - 1;
const RK_LOAD: u16 = 2;
#[derive(Clone, Copy)]
enum Target {
    Local(u8),
    Upvalue(u16),
    Global(u32),
    Index(u8, TargetKey),
}
#[derive(Clone, Copy)]
enum TargetKey {
    Register(u8),
    Constant(u32),
}

#[derive(Clone, Copy)]
enum LineUse {
    Operand,
    Condition,
    Copy,
}

// Keep the instruction and its temporary metadata together during emission.
// Projecting the instruction lets lowering and jump patching share their fields.
#[derive(Clone, Copy)]
struct Emission {
    instruction: Instruction,
    line: u32,
    live: u32,
    hook: u32,
}
impl std::ops::Deref for Emission {
    type Target = Instruction;
    fn deref(&self) -> &Instruction {
        &self.instruction
    }
}
impl std::ops::DerefMut for Emission {
    fn deref_mut(&mut self) -> &mut Instruction {
        &mut self.instruction
    }
}
#[derive(Default)]
struct Function {
    code: Vec<Emission>,
    // An empty dense table keeps the function in consecutive-prefix mode.
    gc_maps: Vec<[u64; 4]>,
    gc_prefix_start: u8,
    gc_prefix_len: u8,
    gc_index: HashMap<[u64; 4], u16>,
    gc_last: Option<u16>,
    live: [u64; 4],
    debug_hidden: [u64; 4],
    debug_named: [u64; 4],
    constants: constants::Constants,
    locals: Vec<Local>,
    upvalues: Vec<(Name, Upvalue)>,
    loops: Vec<(u16, Vec<usize>)>,
    next: u16,
    max: u16,
    params: u16,
    vararg: bool,
    compat_arg: bool,
    defined: [u32; 2],
    debug_locals: Vec<DebugLocal>,
    debug_calls: Vec<DebugCall>,
    debug_names: Vec<u8>,
    debug_temporaries: Vec<u8>,
    active_lines: Vec<u32>,
    hook_visible: usize,
    logical_tests: Vec<usize>,
    debug_stack: u16,
    zero_sign: Option<bool>,
}

impl Function {
    /// Restore the default state while keeping every buffer's capacity.
    fn reset(&mut self) {
        self.code.clear();
        self.gc_maps.clear();
        self.gc_prefix_start = 0;
        self.gc_prefix_len = 0;
        self.gc_index.clear();
        self.gc_last = None;
        self.live = [0; 4];
        self.debug_hidden = [0; 4];
        self.debug_named = [0; 4];
        self.constants.clear();
        self.locals.clear();
        self.upvalues.clear();
        self.loops.clear();
        self.next = 0;
        self.max = 0;
        self.params = 0;
        self.vararg = false;
        self.compat_arg = false;
        self.defined = [0; 2];
        self.debug_locals.clear();
        self.debug_calls.clear();
        self.debug_names.clear();
        self.debug_temporaries.clear();
        self.active_lines.clear();
        self.hook_visible = 0;
        self.logical_tests.clear();
        self.debug_stack = 0;
        self.zero_sign = None;
    }
}

struct Compiler<'a> {
    ast: &'a Ast,
    source: &'a [u8],
    // Move only a pointer when entering or leaving a nested function.
    function: Box<Function>,
    parents: Vec<Box<Function>>,
    /// Finished functions whose cleared buffers serve later nested functions.
    spare: Vec<Box<Function>>,
    /// Local and upvalue names of every function in this compilation.
    names: Vec<u8>,
    prototypes: Vec<Prototype>,
    templates: templates::Templates,
    numeric_templates: numeric_templates::NumericTemplates,
    template_locale: Option<u64>,
    folded: Vec<Option<f64>>,
    tasks: Vec<Task<'a>>,
    /// Assignment targets nest if a target or value contains a function.
    assignment_targets: Vec<Target>,
    result: u8,
    boolean: boolean::Boolean,
    line: u32,
}

/// Code generation storage kept between compilations by one owner.
#[derive(Default)]
pub(crate) struct Spare {
    functions: Vec<Box<Function>>,
    parents: Vec<Box<Function>>,
    names: Vec<u8>,
    folded: Vec<Option<f64>>,
    tasks: Vec<Task<'static>>,
    assignment_targets: Vec<Target>,
    templates: templates::Templates,
    numeric_templates: numeric_templates::NumericTemplates,
}

impl Spare {
    pub(crate) fn folded_literals(&self) -> &[Option<f64>] {
        &self.folded
    }
}

/// Reuse an empty continuation stack for a borrow of a different syntax tree.
fn recycle<'b>(mut tasks: Vec<Task<'_>>) -> Vec<Task<'b>> {
    tasks.clear();
    // An empty source yields nothing; in-place collection keeps the allocation.
    tasks.into_iter().map(|_| unreachable!()).collect()
}

/// Move a buffer's values into an exact-size box, keeping the buffer's capacity.
fn drain_boxed<T>(values: &mut Vec<T>) -> Result<Box<[T]>, Error> {
    let mut exact = fallible::with_capacity(values.len())?;
    exact.append(values);
    exact.try_into_boxed_slice()
}

pub(crate) fn compile(ast: Ast, source: &[u8], name: &[u8]) -> Result<Program, Error> {
    compile_with_templates(&ast, source, name, &mut Spare::default(), None)
}

/// Compile with storage from `spare`, which keeps it again after success.
pub(crate) fn compile_in(
    ast: &mut Ast,
    source: &[u8],
    name: &[u8],
    spare: &mut Spare,
    parse_locale: u64,
) -> Result<Program, Error> {
    let same_locale = parse_locale == crate::locale::revision();
    if !same_locale {
        // Two identical source fragments may have been tokenized under
        // different rules if the locale changed during this parse.
        for (expression, _) in &mut ast.exprs {
            if let Expr::Function {
                template_source, ..
            } = expression
            {
                *template_source = [0; 2];
            }
        }
    }
    let template_locale =
        (same_locale && !crate::memory::compilation::active()).then_some(parse_locale);
    compile_with_templates(ast, source, name, spare, template_locale)
}

fn compile_with_templates(
    ast: &Ast,
    source: &[u8],
    name: &[u8],
    spare: &mut Spare,
    template_locale: Option<u64>,
) -> Result<Program, Error> {
    let mut templates = std::mem::take(&mut spare.templates);
    templates.begin(template_locale);
    let mut numeric_templates = std::mem::take(&mut spare.numeric_templates);
    numeric_templates.begin(template_locale);
    let mut functions = std::mem::take(&mut spare.functions);
    let mut function = match functions.pop() {
        Some(function) => function,
        None => fallible::boxed(Function::default())?,
    };
    function.vararg = true;
    function.zero_sign = ast.root_zero_sign;
    function.debug_stack = ast.root_stack;
    let mut folded = std::mem::take(&mut spare.folded);
    crate::optimizer::literals_into(ast, &mut folded)?;
    let mut names = std::mem::take(&mut spare.names);
    names.clear();
    let mut tasks = recycle(std::mem::take(&mut spare.tasks));
    let mut prototypes = Vec::new();
    if !crate::memory::compilation::active() {
        // Every AST node emits about one instruction. Sizing the entry
        // function's buffers up front avoids their growth copies; nested
        // functions reuse buffers released by earlier functions.
        let estimate = ast.exprs.len() + ast.stmts.len() + 8;
        fallible::reserve(&mut function.code, estimate)?;
        fallible::reserve(&mut tasks, 64)?;
        // The parser records one stack size for each nested function.
        prototypes = fallible::with_capacity(ast.stacks.len() + 1)?;
    }
    tasks.try_push(Task::Block(ast.ids(ast.body)))?;
    let mut c = Compiler {
        ast,
        source,
        function,
        parents: std::mem::take(&mut spare.parents),
        spare: functions,
        names,
        prototypes,
        templates,
        numeric_templates,
        template_locale,
        folded,
        tasks,
        assignment_targets: std::mem::take(&mut spare.assignment_targets),
        result: 0,
        boolean: boolean::Boolean::default(),
        line: 1,
    };
    c.run()?;
    c.line = ast.end_line;
    c.emit(Op::Return, 0, 0, 0)?;
    let entry = c.finish()?;
    c.spare.try_push(c.function)?;
    spare.functions = c.spare;
    debug_assert!(c.parents.is_empty());
    spare.parents = c.parents;
    spare.names = c.names;
    spare.folded = c.folded;
    spare.tasks = recycle(c.tasks);
    debug_assert!(c.assignment_targets.is_empty());
    spare.assignment_targets = c.assignment_targets;
    let p = Program::try_new(c.prototypes, entry, name)?;
    p.verify()?;
    spare.templates = c.templates;
    spare.numeric_templates = c.numeric_templates;
    Ok(p)
}

impl<'a> Compiler<'a> {
    /// A cleared function, reusing the buffers of a finished one when possible.
    fn fresh_function(&mut self) -> Result<Box<Function>, Error> {
        match self.spare.pop() {
            Some(function) => Ok(function),
            None => fallible::boxed(Function::default()),
        }
    }
    fn finish(&mut self) -> Result<u32, Error> {
        let f = &mut *self.function;
        boolean::thread_jumps(&mut f.code)?;
        for &pc in &f.logical_tests {
            let i = f.code[pc];
            if i.c as usize > pc
                && f.code.get(i.c as usize).is_some_and(|next| {
                    matches!(next.op, Op::JumpFalse | Op::JumpTrue)
                        && next.op != i.op
                        && next.a == i.a
                })
            {
                // Value-producing logical chains share the outer test.
                // Statement predicates retain their own callback boundary.
                f.code[pc].c += 1;
            }
        }
        for local in &mut f.debug_locals {
            if local.end == u32::MAX {
                local.end = f.code.len() as u32;
            }
        }
        let mut names = fallible::with_capacity(f.upvalues.len())?;
        let mut upvalues = fallible::with_capacity(f.upvalues.len())?;
        for &(name, value) in &f.upvalues {
            names.push(DebugName::store(
                &mut f.debug_names,
                &self.names[name.range()],
            )?);
            upvalues.push(value);
        }
        let id = self.prototypes.len() as u32;
        let mut code = fallible::with_capacity(f.code.len())?;
        let mut metadata = fallible::with_capacity(f.code.len())?;
        let mut previous: Option<Instruction> = None;
        let first_line = f.defined[0].clamp(1, u32::MAX - 63);
        let mut nearby_lines = 0u64;
        for (pc, emitted) in f.code.iter().enumerate() {
            let mut instruction = emitted.instruction;
            // Active lines include comparisons and branches whose individual
            // hook events are coalesced below.
            if emitted.hook != 0 {
                let offset = emitted.hook.wrapping_sub(first_line);
                if offset < 64 {
                    nearby_lines |= 1 << offset;
                } else if f.active_lines.last().copied() != Some(emitted.hook) {
                    f.active_lines.try_push(emitted.hook)?;
                }
            }
            let hook = match instruction.op {
                Op::JumpFalse | Op::JumpTrue
                    if previous.is_some_and(|p| {
                        matches!(
                            p.op,
                            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::Bool
                        ) && p.a == instruction.a
                    }) =>
                {
                    0
                }
                Op::Bool
                    if f.code[pc + 1..]
                        .iter()
                        .find(|next| next.op != Op::Close)
                        .is_some_and(|next| {
                            matches!(next.op, Op::JumpFalse | Op::JumpTrue)
                                && next.a == instruction.a
                        }) =>
                {
                    0
                }
                _ => emitted.hook,
            };
            if instruction.op == Op::Load {
                instruction.b &= 1;
            }
            if instruction.op == Op::Jump
                && instruction.b == 0
                && let Some([compare, branch, update, body @ ..]) =
                    f.code.get(instruction.c as usize..pc)
                && matches!(compare.op, Op::Lt | Op::Le | Op::Gt | Op::Ge)
                && branch.op == Op::JumpFalse
                && branch.b == COMPARE_BRANCH
                && branch.c as usize == pc + 1
                && matches!(update.op, Op::Add | Op::Sub)
                && body.len() <= 32
                && body
                    .iter()
                    .all(|i| matches!(i.op, Op::Add | Op::Call | Op::Close | Op::Load | Op::Move))
                && body
                    .iter()
                    .any(|i| i.op == Op::Call && i.b == 2 && i.c == 1)
            {
                instruction.b = NUMERIC_WHILE;
            }
            if instruction.op != Op::Close {
                previous = Some(instruction);
            }
            code.push(instruction);
            metadata.push(InstructionMetadata {
                line: emitted.line,
                hook_line: hook,
                gc_live: emitted.live,
            });
        }
        let sort_lines = !f.active_lines.is_empty();
        fallible::reserve(&mut f.active_lines, nearby_lines.count_ones() as usize)?;
        while nearby_lines != 0 {
            f.active_lines.push(first_line + nearby_lines.trailing_zeros());
            nearby_lines &= nearby_lines - 1;
        }
        if sort_lines {
            f.active_lines.sort_unstable();
            f.active_lines.dedup();
        }
        let debug_locals = drain_boxed(&mut f.debug_locals)?;
        let debug_calls = drain_boxed(&mut f.debug_calls)?;
        let constants = f.constants.finish()?;
        self.prototypes.try_push(Prototype::new(
            PrototypeBody {
                code: code.try_into_boxed_slice()?,
                gc_maps: if f.gc_maps.is_empty() {
                    RegisterMaps::Prefix {
                        start: f.gc_prefix_start,
                        len: f.gc_prefix_len,
                    }
                } else {
                    RegisterMaps::Owned(f.gc_maps.try_boxed()?)
                },
                original_constants: constants.values,
                constant_strings: constants.strings,
                registers: f.max.max(f.debug_stack).max(2),
                params: f.params,
                vararg: f.vararg,
                compat_arg: f.compat_arg,
                upvalues: upvalues.try_into_boxed_slice()?,
                bindings: Box::default(),
                debug: DebugInfo {
                    stack: f.debug_stack,
                    native: None,
                    names: f.debug_names.try_boxed()?,
                    upvalues: names.try_into_boxed_slice()?,
                    locals: debug_locals,
                    calls: debug_calls,
                    temporaries: f.debug_temporaries.try_boxed()?,
                },
                ..PrototypeBody::default()
            },
            metadata.try_into_boxed_slice()?,
            f.defined,
            f.active_lines.try_boxed()?,
        ))?;
        f.reset();
        Ok(id)
    }
    fn intern_name(&mut self, name: &[u8]) -> Result<Name, Error> {
        let start = u32::try_from(self.names.len()).map_err(|_| Error::memory())?;
        let len = u32::try_from(name.len()).map_err(|_| Error::memory())?;
        self.names.try_extend_from_slice(name)?;
        Ok(Name {
            start,
            len,
            head: name_head(name),
        })
    }
    fn upvalue(&mut self, name: &[u8]) -> Result<Option<u16>, Error> {
        let names = &self.names;
        let head = name_head(name);
        if let Some(index) = self
            .function
            .upvalues
            .iter()
            .position(|(n, _)| n.equals(names, name, head))
        {
            return Ok(Some(index as u16));
        }
        let mut owner = None;
        for (index, parent) in self.parents.iter_mut().enumerate().rev() {
            if let Some(local) = parent
                .locals
                .iter_mut()
                .rev()
                .find(|l| l.name.equals(names, name, head))
            {
                local.captured = true;
                owner = Some((index, Upvalue::Local(local.reg)));
                break;
            }
            if let Some(up) = parent
                .upvalues
                .iter()
                .position(|(n, _)| n.equals(names, name, head))
            {
                owner = Some((index, Upvalue::Outer(up as u16)));
                break;
            }
        }
        let Some((owner, mut up)) = owner else {
            return Ok(None);
        };
        let name = self.intern_name(name)?;
        for index in owner + 1..self.parents.len() {
            if self.parents[index].upvalues.len() >= 60 {
                return Err(self.error(ErrorKind::Limit, "too many upvalues"));
            }
            let parent = &mut self.parents[index];
            let slot = parent.upvalues.len() as u16;
            parent.upvalues.try_push((name, up))?;
            up = Upvalue::Outer(slot);
        }
        if self.function.upvalues.len() >= 60 {
            return Err(self.error(ErrorKind::Limit, "too many upvalues"));
        }
        let index = self.function.upvalues.len() as u16;
        self.function.upvalues.try_push((name, up))?;
        Ok(Some(index))
    }
    fn close(&mut self, base: u16) -> Result<(), Error> {
        if base < self.function.max {
            self.emit(Op::Close, base as u8, 0, 0)?;
        }
        Ok(())
    }
    fn error(&self, kind: ErrorKind, message: &'static str) -> Error {
        Error::new_static(kind, self.line, message)
    }
    fn alloc(&mut self, count: u16) -> Result<u8, Error> {
        let start = self.function.next;
        self.function.next = self
            .function
            .next
            .checked_add(count)
            .filter(|n| *n <= 256)
            .ok_or_else(|| self.error(ErrorKind::Limit, "function needs too many registers"))?;
        self.function.max = self.function.max.max(self.function.next);
        u8::try_from(start)
            .map_err(|_| self.error(ErrorKind::Limit, "function needs too many registers"))
    }
    fn reset(&mut self, next: u16) {
        let f = &mut self.function;
        f.next = next;
        // The largest allocation tells us whether the upper register words
        // have ever been used by this function.
        if f.max <= 64 {
            debug_assert_eq!(f.live[1..], [0; 3]);
            debug_assert_eq!(f.debug_hidden[1..], [0; 3]);
            if next < 64 {
                let mask = (1u64 << next) - 1;
                let live = f.live[0] & mask;
                if live != f.live[0] {
                    f.live[0] = live;
                    f.gc_last = None;
                }
                f.debug_hidden[0] &= mask;
            }
            return;
        }
        let word = usize::from(next) / 64;
        let bit = next % 64;
        if word < 4 {
            let before = f.live;
            f.live[word] &= (1u64 << bit) - 1;
            f.live[word + 1..].fill(0);
            if f.live != before {
                f.gc_last = None;
            }
            f.debug_hidden[word] &= (1u64 << bit) - 1;
            f.debug_hidden[word + 1..].fill(0);
        }
    }
    fn live_register(&mut self, register: u16) {
        if register < 256 {
            let word = usize::from(register) / 64;
            let bit = 1u64 << (register % 64);
            if self.function.live[word] & bit == 0 {
                self.function.live[word] |= bit;
                self.function.gc_last = None;
            }
            self.function.debug_hidden[word] &= !bit;
        }
    }
    fn live_map(&mut self) -> Result<u16, Error> {
        // Register changes invalidate the current map at the point of mutation.
        if let Some(index) = self.function.gc_last {
            return Ok(index);
        }
        let mask = self.function.live;
        if self.function.gc_maps.is_empty() {
            let low = mask[0];
            if low & low.wrapping_add(1) == 0 && mask[1..] == [0; 3] {
                let count = low.trailing_ones() as u8;
                let f = &mut self.function;
                if f.gc_prefix_len == 0 {
                    f.gc_prefix_start = count;
                }
                if let Some(index) = count.checked_sub(f.gc_prefix_start)
                    && index <= f.gc_prefix_len
                {
                    if index == f.gc_prefix_len {
                        f.gc_prefix_len += 1;
                    }
                    f.gc_last = Some(u16::from(index));
                    return Ok(u16::from(index));
                }
            }
            self.promote_gc_maps()?;
        }
        // Small functions need only a few maps. Their dense vector avoids both
        // hashing and an additional allocation until a larger index is useful.
        let present = if self.function.gc_maps.len() <= 16 {
            self.function
                .gc_maps
                .iter()
                .position(|m| *m == mask)
                .map(|i| i as u16)
        } else {
            self.function.gc_index.get(&mask).copied()
        };
        if let Some(index) = present {
            self.function.gc_last = Some(index);
            return Ok(index);
        }
        let index = u16::try_from(self.function.gc_maps.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many GC register maps"))?;
        self.function.gc_maps.try_push(mask)?;
        if index == 16 {
            self.function.gc_index.try_reserve(17)?;
            for (i, &mask) in self.function.gc_maps.iter().enumerate() {
                self.function.gc_index.insert(mask, i as u16);
            }
        } else if index > 16 {
            self.function.gc_index.try_reserve(1)?;
            self.function.gc_index.insert(mask, index);
        }
        self.function.gc_last = Some(index);
        Ok(index)
    }
    #[cold]
    fn promote_gc_maps(&mut self) -> Result<(), Error> {
        let f = &mut self.function;
        let maps = RegisterMaps::prefix(f.gc_prefix_start, f.gc_prefix_len);
        f.gc_maps.try_extend_from_slice(maps)?;
        f.gc_prefix_len = 0;
        if maps.len() > 16 {
            f.gc_index.try_reserve(maps.len())?;
            for (i, &mask) in maps.iter().enumerate() {
                f.gc_index.insert(mask, i as u16);
            }
        }
        Ok(())
    }
    fn emit(&mut self, op: Op, a: u8, mut b: u16, mut c: u32) -> Result<usize, Error> {
        if op.rk_c() {
            while let Some(load) = self.function.code.last().copied().filter(|load| {
                load.op == Op::Load
                    && load.b & RK_LOAD != 0
                    && ((op.rk_b() && b == u16::from(load.a)) || c == u32::from(load.a))
            }) {
                let value = RK + load.c;
                if op.rk_b() && b == u16::from(load.a) {
                    b = value as u16;
                }
                if c == u32::from(load.a) {
                    c = value;
                }
                let removed = self.function.code.pop().unwrap();
                let before = removed.live as u16 as usize;
                let f = &mut self.function;
                f.live = if f.gc_maps.is_empty() {
                    RegisterMaps::prefix(f.gc_prefix_start, f.gc_prefix_len)[before]
                } else {
                    f.gc_maps[before]
                };
                self.function.gc_last = Some(before as u16);
                assert_eq!(removed.hook, 0);
            }
        }
        let pc = self.function.code.len();
        if pc >= u32::MAX as usize {
            return Err(self.error(ErrorKind::Limit, "function has too many instructions"));
        }
        let before = self.live_map()?;
        let writes_register = match op {
            Op::Load
            | Op::Move
            | Op::Bool
            | Op::GetGlobal
            | Op::NewTable
            | Op::Closure
            | Op::GetUpvalue
            | Op::GetTable
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Mod
            | Op::Pow
            | Op::Neg
            | Op::Not
            | Op::Len
            | Op::Concat
            | Op::ConcatRange
            | Op::Eq
            | Op::Ne
            | Op::Lt
            | Op::Le
            | Op::Gt
            | Op::Ge => {
                self.live_register(u16::from(a));
                true
            }
            Op::Call | Op::CallIter | Op::TailCall | Op::Vararg => {
                let saved = self.function.next;
                self.reset(u16::from(a));
                self.function.next = saved;
                let count = if c == OPEN || op == Op::TailCall {
                    1
                } else {
                    c as u16
                };
                for r in u16::from(a)..u16::from(a) + count {
                    self.live_register(r);
                }
                true
            }
            Op::ForLoop => {
                self.live_register(u16::from(a) + 3);
                true
            }
            _ => false,
        };
        let after = if writes_register { self.live_map()? } else { before };
        let hook = if op == Op::Close { 0 } else { self.line };
        self.function.code.try_push(Emission {
            instruction: Instruction { op, a, b, c },
            line: self.line,
            live: u32::from(before) | (u32::from(after) << 16),
            hook,
        })?;
        self.function.hook_visible += usize::from(op != Op::Close && self.line != 0);
        if op == Op::Move
            && (c == OPEN
                || u16::from(a) == b
                || (self.function.debug_named[usize::from(a) / 64] & (1u64 << (a % 64)) == 0
                    && self.function.debug_named[usize::from(b) / 64] & (1u64 << (b % 64)) == 0))
        {
            self.debug_line(0);
        }
        Ok(pc)
    }
    fn patch(&mut self, pc: usize) {
        self.function.code[pc].c = self.function.code.len() as u32;
    }
    fn debug_line(&mut self, line: u32) {
        let previous = self.function.code.last().expect("emitted hook line").hook;
        self.function.hook_visible =
            self.function.hook_visible + usize::from(line != 0) - usize::from(previous != 0);
        self.function
            .code
            .last_mut()
            .expect("emitted hook line")
            .hook = line;
    }
    fn normalized_number(&self, number: f64) -> f64 {
        if number == 0.0
            && let Some(negative) = self.function.zero_sign
        {
            if negative { -0.0 } else { 0.0 }
        } else {
            number
        }
    }
    fn constant(&mut self, mut k: Constant) -> Result<u32, Error> {
        if let Constant::Number(number) = &mut k {
            *number = self.normalized_number(*number);
        }
        self.function
            .constants
            .intern(k)?
            .ok_or_else(|| self.error(ErrorKind::Limit, "too many constants"))
    }
    fn string_constant(&mut self, spelling: &Lexeme) -> Result<u32, Error> {
        self.function
            .constants
            .source_string(spelling, self.source)?
            .ok_or_else(|| self.error(ErrorKind::Limit, "too many constants"))
    }
    fn load_string(&mut self, dst: u8, spelling: &Lexeme) -> Result<(), Error> {
        let id = self.string_constant(spelling)?;
        self.emit(Op::Load, dst, 0, id)?;
        Ok(())
    }
    fn load(&mut self, dst: u8, k: Constant) -> Result<(), Error> {
        let initial_nil = matches!(k, Constant::Nil)
            && self.function.hook_visible == 0
            && self.function.loops.is_empty()
            && self.function.debug_named[usize::from(dst) / 64] & (1u64 << (dst % 64)) == 0;
        let id = self.constant(k)?;
        self.emit(Op::Load, dst, u16::from(initial_nil), id)?;
        if initial_nil {
            self.debug_line(0);
        }
        Ok(())
    }
    fn local_register(&self, id: ExprId) -> Option<u8> {
        let (expr, span) = &self.ast.exprs[id];
        let slot = span.binding.local_index()?;
        let local = &self.function.locals[slot];
        debug_assert!(matches!(expr, Expr::Name(name) if {
            let name = name.bytes(self.source);
            local.name.equals(&self.names, name, name_head(name))
        }));
        Some(local.reg)
    }
    fn resolve_name(&mut self, id: ExprId) -> Result<Target, Error> {
        if let Some(reg) = self.local_register(id) {
            return Ok(Target::Local(reg));
        }
        let (Expr::Name(name), span) = &self.ast.exprs[id] else {
            unreachable!("name binding requires a name expression");
        };
        if span.binding == NameBinding::UPVALUE {
            let up = self
                .upvalue(name.bytes(self.source))?
                .expect("parsed upvalue has a binding");
            return Ok(Target::Upvalue(up));
        }
        self.string_constant(name).map(Target::Global)
    }
    fn bind(&mut self, name: &[u8], reg: u8) -> Result<(), Error> {
        if self.function.locals.len() >= 200 {
            return Err(self.error(ErrorKind::Limit, "too many local variables"));
        }
        let debug = self.debug_local(name, reg)?;
        let name = self.intern_name(name)?;
        self.function.locals.try_push(Local {
            name,
            reg,
            debug,
            captured: false,
        })?;
        self.live_register(u16::from(reg));
        Ok(())
    }
    fn debug_local(&mut self, name: &[u8], reg: u8) -> Result<usize, Error> {
        self.function.debug_named[usize::from(reg) / 64] |= 1u64 << (reg % 64);
        let index = self.function.debug_locals.len();
        self.function.debug_locals.try_push(DebugLocal {
            name: DebugName::store(&mut self.function.debug_names, name)?,
            start: self.function.code.len() as u32,
            end: u32::MAX,
            register: u16::from(reg),
        })?;
        Ok(index)
    }
    fn end_locals(&mut self, retained: usize) {
        let end = self.function.code.len() as u32;
        for local in &self.function.locals[retained..] {
            self.function.debug_locals[local.debug].end = end;
            self.function.debug_named[usize::from(local.reg) / 64] &= !(1u64 << (local.reg % 64));
        }
        self.function.locals.truncate(retained);
    }
    fn finish_loop(&mut self) {
        for pc in self
            .function
            .loops
            .pop()
            .expect("compiler loop stack must be balanced")
            .1
        {
            self.patch(pc);
        }
    }
    fn multiple(&self, id: ExprId) -> bool {
        matches!(
            self.ast.exprs[id].0,
            Expr::Call(..) | Expr::Method(..) | Expr::Vararg
        )
    }

    fn record_call(
        &mut self,
        mut function: ExprId,
        method: Option<&[u8]>,
        base: u8,
    ) -> Result<(), Error> {
        let mut constant = None;
        let (name, kind) = if let Some(name) = method {
            (name, NameKind::Method)
        } else {
            loop {
                match self.ast.exprs[function].0 {
                    Expr::Paren(inner) => function = inner,
                    Expr::Binary(Kind::Or, left, right, ..)
                        if matches!(self.ast.exprs[left].0, Expr::Binary(Kind::And, ..)) =>
                    {
                        function = right
                    }
                    _ => break,
                }
            }
            match &self.ast.exprs[function].0 {
                Expr::Name(name) => {
                    let name = name.bytes(self.source);
                    let kind = if self.local_register(function).is_some() {
                        NameKind::Local
                    } else if self
                        .function
                        .upvalues
                        .iter()
                        .any(|(n, _)| n.equals(&self.names, name, name_head(name)))
                    {
                        NameKind::Upvalue
                    } else {
                        NameKind::Global
                    };
                    (name, kind)
                }
                Expr::Index(_, key) => {
                    let name = if let Expr::String(name) = &self.ast.exprs[*key].0 {
                        constant = self.function.constants.emitted_string(name);
                        name.bytes(self.source)
                    } else {
                        b"?"
                    };
                    (name, NameKind::Field)
                }
                _ => (b"".as_slice(), NameKind::Unknown),
            }
        };
        let start = u32::try_from(self.function.debug_temporaries.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many debug temporaries"))?;
        let base = u32::from(base);
        for word in 0..base.div_ceil(64) {
            let first = word * 64;
            let below_base = u64::MAX >> (64 - (base - first).min(64));
            let word = word as usize;
            let mut visible = self.function.live[word]
                & !self.function.debug_hidden[word]
                & !self.function.debug_named[word]
                & below_base;
            while visible != 0 {
                self.function
                    .debug_temporaries
                    .try_push((first + visible.trailing_zeros()) as u8)?;
                visible &= visible - 1;
            }
        }
        let end = u32::try_from(self.function.debug_temporaries.len())
            .map_err(|_| self.error(ErrorKind::Limit, "too many debug temporaries"))?;
        let name = if let Some(constant) = constant {
            DebugName::constant(constant)
        } else {
            DebugName::store(&mut self.function.debug_names, name)?
        };
        self.function.debug_calls.try_push(DebugCall {
            pc: self.function.code.len() as u32,
            name,
            kind,
            temporaries: start..end,
        })?;
        Ok(())
    }

    fn discharge_line(&mut self, mut id: ExprId, line: u32, usage: LineUse) {
        loop {
            match &self.ast.exprs[id].0 {
                Expr::Paren(inner) | Expr::Binary(Kind::And | Kind::Or, _, inner, _) => id = *inner,
                Expr::Name(_) => {
                    let local = self.local_register(id).is_some();
                    if matches!(usage, LineUse::Condition) && local {
                        self.debug_line(0);
                    } else if matches!(usage, LineUse::Copy) || !local {
                        self.debug_line(line);
                    }
                    return;
                }
                Expr::Nil | Expr::Boolean(_) | Expr::Number(_) | Expr::String(_) => {
                    if !matches!(usage, LineUse::Operand) {
                        self.debug_line(line);
                    }
                    return;
                }
                Expr::Index(..) => {
                    self.debug_line(line);
                    return;
                }
                _ => return,
            }
        }
    }
}
