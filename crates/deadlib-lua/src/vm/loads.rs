//! Resumable source/binary validation through protected loading frames.
use super::*;
use crate::memory::bytes::ByteOutput;
use crate::memory::compilation::Workspace;
use crate::memory::fallible::{self, SliceExt, VecExt};

mod bindings;
pub(super) use bindings::BindingPool;
mod constructor_templates;
mod eof;
mod literal_tables;
mod reader;
pub(super) mod reader_intervals;
mod source_templates;

/// A genuine, non-callable collector object for an unfinished/retained prototype.
#[derive(Debug)]
pub(super) struct ParserPrototype {
    pub(super) source: StringId,
    // Counts ordinals in the reader's table. Only string constants contribute
    // an edge to the prototype's collector roots.
    pub(super) constant_count: usize,
    pub(super) constant_strings: Buffer<StringId>,
    pub(super) children: Buffer<ParserPrototypeId>,
    pub(super) debug_strings: Buffer<StringId>,
}
impl ParserPrototype {
    pub(super) fn heap_bytes(&self) -> usize {
        (self.constant_strings.capacity() + self.debug_strings.capacity()) * size_of::<StringId>()
            + self.children.capacity() * size_of::<ParserPrototypeId>()
    }
    pub(super) fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        std::iter::once(Value::String(self.source))
            .chain(self.constant_strings.iter().copied().map(Value::String))
            .chain(self.children.iter().copied().map(Value::ParserPrototype))
            .chain(self.debug_strings.iter().copied().map(Value::String))
    }
}
#[derive(Debug)]
pub(super) struct NativeRoots {
    pub active: Buffer<(Option<TableId>, ParserPrototypeId)>,
    pub started: bool,
    pub prototypes: Buffer<ParserPrototypeId>,
}
impl NativeRoots {
    fn new_in(memory: Memory) -> Self {
        Self {
            active: Buffer::new_in(memory.clone()),
            started: false,
            prototypes: Buffer::new_in(memory),
        }
    }
    pub(super) fn storage_bytes(&self) -> usize {
        self.active.capacity() * size_of::<(Option<TableId>, ParserPrototypeId)>()
            + self.prototypes.capacity() * size_of::<ParserPrototypeId>()
    }
}

/// Reusable compiler storage and lookup hints for loading programs.
/// Budgeted compilations charge and release fresh parser and compiler storage.
#[derive(Default)]
pub(super) struct Spare {
    parser: crate::parser::Spare,
    compiler: crate::compiler::Spare,
    source_templates: source_templates::Templates,
    reader_intervals: reader_intervals::Cache,
    constructors: constructor_templates::Templates,
    literal_tables: literal_tables::Templates,
    recent_sources: [Option<(SourceKey, u64)>; 8],
    next_source: usize,
    constant_strings: [Option<(ShortConstant, StringId)>; 32],
    pub(super) bindings: BindingPool,
}

impl Spare {
    pub(super) fn collect_with_load_buffers<R>(
        &mut self,
        collect: impl FnOnce(&mut BindingPool) -> R,
    ) -> R {
        let bindings = &mut self.bindings;
        self.compiler.collect_with_code_pool(|| collect(bindings))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SourceKey {
    source: StringId,
    name: StringId,
    depth: usize,
    locale: u64,
}

/// Fixed-width contents of a short string. The two words cover every byte,
/// including embedded zeros, without loading the interned string's buffer.
#[derive(Clone, Copy, PartialEq, Eq)]
struct ShortConstant {
    first: u64,
    last: u64,
    len: u8,
}

impl ShortConstant {
    fn new(bytes: &[u8]) -> Option<Self> {
        let len = bytes.len();
        let (first, last) = match len {
            0 => (0, 0),
            1..=3 => (
                u64::from(bytes[0]) | (u64::from(bytes[len / 2]) << 8),
                u64::from(bytes[len - 1]),
            ),
            4..=7 => (
                u64::from(u32::from_le_bytes(*bytes.first_chunk().unwrap())),
                u64::from(u32::from_le_bytes(*bytes.last_chunk().unwrap())),
            ),
            8..=16 => (
                u64::from_le_bytes(*bytes.first_chunk().unwrap()),
                u64::from_le_bytes(*bytes.last_chunk().unwrap()),
            ),
            _ => return None,
        };
        Some(Self {
            first,
            last,
            len: len as u8,
        })
    }

    fn slot(self) -> usize {
        ((self.first ^ self.last.rotate_left(19) ^ u64::from(self.len))
            .wrapping_mul(0x9e3779b185ebca87)
            >> 59) as usize
    }
}
impl std::fmt::Debug for Spare {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Spare").finish_non_exhaustive()
    }
}
impl Spare {
    fn find_text(
        &mut self,
        source: &[u8],
        name: &[u8],
        depth: usize,
        locale: u64,
        preamble: Option<&source_templates::Preamble>,
    ) -> Option<Result<Program, Error>> {
        preamble
            .and_then(|preamble| {
                self.source_templates
                    .find(source, name, depth, locale, preamble)
            })
            .map(|variant| variant.publish())
            .or_else(|| self.literal_tables.find(source, name, depth, locale))
            .or_else(|| self.constructors.find(source, name, depth, locale))
    }

    fn finish_text(
        &mut self,
        mut ast: crate::parser::Ast,
        source: &[u8],
        name: &[u8],
        depth: usize,
        locale: u64,
        candidate: Option<source_templates::Candidate>,
    ) -> Result<Program, Error> {
        let result =
            crate::compiler::compile_in(&mut ast, source, name, &mut self.compiler, locale);
        if let (Some(candidate), Ok(program)) = (candidate, &result) {
            self.source_templates.remember(
                source,
                depth,
                locale,
                candidate,
                program,
                &ast,
                self.compiler.folded_literals(),
            );
        }
        if let Ok(program) = &result {
            self.literal_tables
                .remember(source, depth, locale, program, &ast);
            self.constructors
                .remember(source, depth, locale, program, &ast);
        }
        self.parser.keep_ast(ast);
        result
    }
}
/// Sources above this size release their storage instead of retaining it.
const SPARE_SOURCE_LIMIT: usize = 64 * 1024;

impl Vm {
    /// Compile source or a native Lua chunk using this VM's compiler workspace.
    ///
    /// The returned program is verified and independent of the VM. Subsequent
    /// calls can reuse temporary parser and emitter buffers for small sources.
    ///
    /// # Errors
    /// Returns a syntax, bytecode, compilation-limit or memory error.
    pub fn compile_source(&mut self, source: &[u8], name: &[u8]) -> Result<Program, Error> {
        Workspace::new(self.registers.memory())?
            .run(|| compile(source, name, 0, &mut self.compile_spare))
    }

    pub(super) fn bind_constant_string(&mut self, bytes: &[u8]) -> Result<Value, Error> {
        let Some(key) = ShortConstant::new(bytes) else {
            return self.try_intern(bytes);
        };
        let slot = key.slot();
        if let Some((cached, id)) = self.compile_spare.constant_strings[slot]
            && cached == key
            && self.strings.bytes.contains(id.0)
        {
            // The hint owns no string. A hit inherits the usual protection for
            // a reference created during an incremental collection.
            self.strings.bytes.preserve_new_reference(id.0);
            return Ok(Value::String(id));
        }
        let value = self.try_intern(bytes)?;
        let Value::String(id) = value else {
            unreachable!("interned string")
        };
        self.compile_spare.constant_strings[slot] = Some((key, id));
        Ok(value)
    }

    pub(super) fn load_string_program(
        &mut self,
        source: StringId,
        name: StringId,
        depth: usize,
        binary: bool,
    ) -> Result<FunctionId, Error> {
        let bytes = &self.strings.bytes[slot_index(source.0)];
        let memory = self.registers.memory();
        let key = (!binary && bytes.len() <= SPARE_SOURCE_LIMIT && memory.budget_usage().is_none())
            .then(|| SourceKey {
                source,
                name,
                depth,
                locale: crate::locale::revision(),
            });
        if self.can_reuse_loaded_program()
            && let Some(key) = key
            && let Some(program) =
                self.compile_spare
                    .recent_sources
                    .iter()
                    .flatten()
                    .find_map(|&(cached, program)| {
                        (cached == key && self.programs.contains(program)).then_some(program)
                    })
        {
            // A hint owns neither the source nor the program. Only reuse a
            // binding between collections, when its constant strings are live.
            // A new entry closure still gets this thread's environment.
            let program = slot_index(program);
            let prototype = self.programs[program].data.entry as usize;
            self.functions
                .try_reserve(1)
                .map_err(|_| self.memory_failure())?;
            self.reserve_gc_new_value()?;
            let function = self.insert_closure(Closure {
                program,
                prototype,
                upvalues: Buffer::new_in(self.functions.memory().clone()),
                env: self.globals,
            });
            self.gc_new_value(Value::Function(function));
            return Ok(function);
        }

        let program = Workspace::new(memory)?.run(|| {
            compile(
                bytes,
                &self.strings.bytes[slot_index(name.0)],
                depth,
                &mut self.compile_spare,
            )
        })?;
        let function = self.load_fresh(&program)?;
        if let Some(key) = key
            && key.locale == crate::locale::revision()
        {
            let index = self.functions[slot_index(function.0)].program;
            let owner = self
                .programs
                .key(index)
                .expect("loaded program has an identity");
            let spare = &mut self.compile_spare;
            spare.recent_sources[spare.next_source] = Some((key, owner));
            spare.next_source = (spare.next_source + 1) % spare.recent_sources.len();
        }
        Ok(function)
    }

    pub(super) fn load_string_result(
        &mut self,
        source: StringId,
        name: StringId,
        depth: usize,
        binary: bool,
    ) -> Result<(Value, Option<Value>), Error> {
        match self.load_string_program(source, name, depth, binary) {
            Ok(function) => Ok((Value::Function(function), None)),
            Err(error) => {
                let value = if error.kind() == ErrorKind::Memory {
                    Value::String(self.memory_error)
                } else {
                    let message = message(&self.strings.bytes[slot_index(name.0)], &error, binary)?;
                    self.try_intern(&message)?
                };
                Ok((Value::Nil, Some(value)))
            }
        }
    }
}

pub(super) fn compile(
    source: &[u8],
    name: &[u8],
    depth: usize,
    spare: &mut Spare,
) -> Result<Program, Error> {
    if source.first() == Some(&27) {
        return crate::chunk51::compile(source, name, depth);
    }
    if crate::memory::compilation::active() || source.len() > SPARE_SOURCE_LIMIT {
        return crate::parser::Parser::with_native_depth(depth)?
            .resume(source, true)
            .and_then(|ast| crate::compiler::compile(ast.expect("closed source"), source, name));
    }
    if source.len() < 128
        && depth <= 96
        && source
            .iter()
            .position(|&byte| !crate::locale::ascii_space(byte))
            .is_some_and(|start| source[start..].starts_with(b"return"))
        && let Some(program) =
            crate::compiler::compile_literal_return(source, name, &mut spare.compiler)?
    {
        return Ok(program);
    }
    let parse_locale = crate::locale::revision();
    let preamble = source_templates::Preamble::scan(source);
    if let Some(program) = spare.find_text(source, name, depth, parse_locale, preamble.as_ref()) {
        return program;
    }
    // Bound the work of trying direct emission before a full parse.
    if depth <= 96
        && source.len() <= 2048
        && let Some(program) = crate::compiler::compile_direct(source, name, &mut spare.compiler)?
    {
        return Ok(program);
    }
    let mut parser = crate::parser::Parser::try_new_in(&mut spare.parser, depth)?;
    let parsed = parser.resume(source, true);
    let candidate = if parsed.is_ok() {
        preamble.and_then(|preamble| preamble.check_resources(&parser))
    } else {
        None
    };
    parser.recycle(&mut spare.parser);
    let ast = parsed?.expect("closed source");
    spare.finish_text(ast, source, name, depth, parse_locale, candidate)
}

pub(super) fn protected(function: FrameKind) -> bool {
    matches!(
        function,
        FrameKind::Native(LOAD_READER | LOAD_FILE | DO_FILE | CONT_FILE | CONT_HOST_LOAD)
    )
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FileReader {
    pub key: u64,
    owned: bool,
    phase: u8,
}
impl FileReader {
    pub(super) fn borrowed(self) -> Self {
        Self {
            owned: false,
            ..self
        }
    }
    /// Advance the executable-header/binary-reopen prelude. False requests
    /// another prelude step; prefix contains the optional synthetic newline.
    pub(super) fn prepare(
        &mut self,
        stream: &mut io::Stream,
        name: &[u8],
        prefix: &mut impl ByteOutput,
        binary: &mut bool,
    ) -> Result<bool, (i32, bool)> {
        if self.phase == 0 {
            self.phase = if stream.peek().map_err(|e| (e, false))? == Some(b'#') {
                1
            } else {
                2
            };
        }
        if self.phase == 1 {
            if stream
                .skip_until(b'\n', io::LINE_BUFFER, true)
                .map_err(|e| (e, false))?
            {
                self.phase = 2;
                prefix.extend_bytes(b"\n").map_err(|_| (12, false))?;
            }
            return Ok(false);
        }
        if self.phase == 2 {
            *binary = stream.peek().map_err(|e| (e, false))? == Some(27);
            if self.owned && *binary {
                // Reopen raw bytes before locating a signature behind a header.
                let _ = stream.close();
                *stream = io::open_input(&name[1..], true)
                    .map_err(|_| (12, true))?
                    .map_err(|e| (e, true))?;
                prefix.clear_bytes();
                self.phase = 3;
            } else {
                self.phase = 4;
            }
        }
        if self.phase == 3 {
            if !stream
                .skip_until(27, io::LINE_BUFFER, false)
                .map_err(|e| (e, false))?
            {
                return Ok(false);
            }
            self.phase = 4;
        }
        Ok(true)
    }
}

#[derive(Debug)]
struct CapturedInput {
    source: StringId,
    read: usize,
    variant: Option<source_templates::Variant>,
}

#[derive(Debug)]
pub(super) struct Loading {
    pub owner: usize,
    workspace: Workspace,
    pub(super) parser: crate::parser::Parser,
    pub(super) decoder: Option<Box<crate::chunk51::Decoder>>,
    native_depth: usize,
    parse_locale: u64,
    pub(super) binary_mode: bool,
    source: Buffer<u8>,
    captured: Option<CapturedInput>,
    trace: reader_intervals::Trace,
    pub(super) name: Box<[u8]>,
    awaiting: bool,
    closed: bool,
    probed: bool,
    pub native: Option<NativeRoots>,
    pub file: Option<FileReader>,
}
pub(super) fn storage_bytes(jobs: &Buffer<Loading>) -> usize {
    jobs.capacity() * std::mem::size_of::<Loading>()
        + jobs
            .iter()
            .map(|j| {
                j.source.capacity()
                    + j.name.len()
                    + j.parser.heap_bytes()
                    + j.trace.storage_bytes()
                    + j.native.as_ref().map_or(0, |r| {
                        r.active.capacity() * size_of::<(Option<TableId>, ParserPrototypeId)>()
                            + r.prototypes.capacity() * size_of::<ParserPrototypeId>()
                    })
                    + j.decoder
                        .as_ref()
                        .map_or(0, |d| std::mem::size_of_val(d.as_ref()) + d.heap_bytes())
            })
            .sum::<usize>()
}

pub(super) fn message(name: &[u8], error: &Error, binary: bool) -> Result<Vec<u8>, Error> {
    if error.kind() == ErrorKind::Memory {
        return Err(Error::memory());
    }
    if binary {
        return error.message_bytes().try_to_vec();
    }
    let mut chunk = [0; 80];
    let used = crate::bytecode::chunk_id_into(name, &mut chunk);
    let mut message = chunk[..used].try_to_vec()?;
    message
        .try_extend_from_slice(fallible::format(format_args!(":{}: ", error.line()))?.as_bytes())?;
    message.try_extend_from_slice(error.message_bytes())?;
    Ok(message)
}
impl Loading {
    pub(super) fn source(&self) -> &[u8] {
        &self.source
    }
    pub(super) fn root_storage_bytes(&self) -> usize {
        self.native.as_ref().unwrap().storage_bytes() + self.trace.storage_bytes()
    }
    pub(super) fn replayed_interval(
        &self,
    ) -> Option<(crate::memory::Shared<reader_intervals::Plan>, usize, f64)> {
        self.trace.replayed_interval()
    }
    pub(super) fn record_interval(&mut self, events: &[crate::parser::NativeRootEvent]) {
        self.trace.record(
            &self.source,
            self.native_depth,
            self.parser.nesting(),
            self.closed,
            events,
        );
    }
    pub(super) fn shift_native_depth(&mut self, change: u16) {
        self.native_depth = (self.native_depth as u16).wrapping_add(change) as usize;
        self.parser.shift_native_depth(change);
        if let Some(decoder) = &mut self.decoder {
            decoder.shift_native_depth(change);
        }
    }
    pub(super) fn roots(&self) -> impl Iterator<Item = Value> + '_ {
        self.native
            .iter()
            .flat_map(|r| &r.active)
            .flat_map(|&(table, prototype)| {
                table
                    .map(Value::Table)
                    .into_iter()
                    .chain(std::iter::once(Value::ParserPrototype(prototype)))
            })
            .chain(
                self.captured
                    .iter()
                    .map(|input| Value::String(input.source)),
            )
    }
    fn arena_bytes(&self) -> usize {
        self.parser.arena_bytes()
            + self
                .decoder
                .as_ref()
                .map_or(0, |d| std::mem::size_of_val(d.as_ref()) + d.heap_bytes())
    }
    fn resume(&mut self, spare: &mut Spare) -> Result<Option<Program>, Error> {
        self.workspace.clone().run(|| self.resume_inner(spare))
    }
    fn resume_inner(&mut self, spare: &mut Spare) -> Result<Option<Program>, Error> {
        if self.binary_mode {
            let native = self.native.is_some();
            if self.decoder.is_none() {
                let mut decoder = crate::chunk51::Decoder::try_new(self.native_depth)?;
                if native {
                    decoder.enable_native_roots();
                }
                self.decoder = Some(crate::memory::fallible::boxed(decoder)?);
            }
            let decoder = self.decoder.as_mut().unwrap();
            return match decoder.resume(&self.source, self.closed) {
                Ok(false) => Ok(None),
                Ok(true) => decoder.program(&self.name).map(Some),
                Err(error) => Err(crate::chunk51::named_error(&self.name, error)),
            };
        }
        // An empty initial lookahead makes luaX_setinput request another block
        // before open_func has installed the first text prototype's roots.
        if self.source.is_empty() && !self.closed {
            return Ok(None);
        }
        if let Some(roots) = &mut self.native {
            roots.started = true;
        }
        let first = matches!(self.trace, reader_intervals::Trace::Waiting);
        if first {
            self.trace = if !self.closed
                && self.parser.reusable()
                && self.parser.native_roots_enabled()
                && !crate::memory::compilation::active()
                && self.parse_locale == crate::locale::revision()
            {
                spare
                    .reader_intervals
                    .begin(&self.source, &self.name, self.native_depth)
            } else {
                reader_intervals::Trace::None
            };
            if matches!(self.trace, reader_intervals::Trace::Replay(_)) {
                return Ok(None);
            }
        }
        if let reader_intervals::Trace::Replay(playback) = &mut self.trace {
            if playback.step(&self.source, self.closed, self.native_depth) {
                return if self.closed {
                    playback.publish().map(Some)
                } else {
                    Ok(None)
                };
            }
            let reader_intervals::Trace::Replay(playback) = std::mem::take(&mut self.trace) else {
                unreachable!("active reader playback");
            };
            playback.reconstruct(&mut self.parser, &self.source, self.native_depth)?;
        }
        let Some(mut ast) = self.parser.resume(&self.source, self.closed)? else {
            return Ok(None);
        };
        if !self.parser.reusable()
            || (self.native.is_some() && self.source.len() > SPARE_SOURCE_LIMIT)
        {
            return crate::compiler::compile(ast, &self.source, &self.name).map(Some);
        }
        if self.closed
            && self.source.len() <= SPARE_SOURCE_LIMIT
            && self.parse_locale == crate::locale::revision()
            && !crate::memory::compilation::active()
        {
            // Observable readers still drive the parser through every chunk.
            // Once it accepts EOF, code can reuse a verified source template.
            let preamble = source_templates::Preamble::scan(&self.source);
            if let Some(program) = spare.find_text(
                &self.source,
                &self.name,
                self.native_depth,
                self.parse_locale,
                preamble.as_ref(),
            ) {
                spare.parser.keep_ast(ast);
                return program.map(Some);
            }
            let candidate = preamble.and_then(|preamble| preamble.check_resources(&self.parser));
            return spare
                .finish_text(
                    ast,
                    &self.source,
                    &self.name,
                    self.native_depth,
                    self.parse_locale,
                    candidate,
                )
                .map(Some);
        }
        let program = crate::compiler::compile_in(
            &mut ast,
            &self.source,
            &self.name,
            &mut spare.compiler,
            self.parse_locale,
        );
        spare.parser.keep_ast(ast);
        program.map(Some)
    }
    fn read_file(&mut self, stream: &mut io::Stream, whole: bool) -> Result<(), (i32, bool)> {
        self.workspace
            .clone()
            .run(|| self.read_file_inner(stream, whole))
    }
    fn read_file_inner(&mut self, stream: &mut io::Stream, whole: bool) -> Result<(), (i32, bool)> {
        let file = self.file.as_mut().expect("active file reader");
        if !file.prepare(stream, &self.name, &mut self.source, &mut self.binary_mode)? {
            return Ok(());
        }
        // A file this load opened is closed when the load ends, so reading it
        // whole is unobservable and lets the parser see complete source.
        // Shared streams such as stdin keep the reference block size, as do
        // loads under an instruction limit, which charges each block.
        let request = if file.owned && whole {
            usize::MAX / 2
        } else {
            io::LINE_BUFFER
        };
        let before = self.source.len();
        stream
            .read_chars(&mut self.source, request)
            .map_err(|e| (e, false))?;
        self.closed = self.source.len() - before < request;
        Ok(())
    }
}

impl Vm {
    pub(super) fn start_native_load(
        &mut self,
        owner: usize,
        name: &[u8],
        depth: usize,
        file: Option<FileReader>,
    ) -> Result<usize, Error> {
        let index = self.loads.len();
        let capacity = self.loads.capacity();
        let prepared = (|| -> Result<Loading, Error> {
            let workspace = Workspace::new(self.registers.memory())?;
            workspace.run(|| {
                self.loads
                    .try_reserve(1)
                    .map_err(|_| self.memory_failure())?;
                let mut parser =
                    if file.is_none() && self.registers.memory().uses_global_allocator() {
                        crate::parser::Parser::try_new_in(&mut self.compile_spare.parser, depth)?
                    } else {
                        crate::parser::Parser::with_native_depth(depth)?
                    };
                parser.enable_native_roots()?;
                Ok(Loading {
                    owner,
                    workspace: workspace.clone(),
                    parser,
                    decoder: None,
                    native_depth: depth,
                    parse_locale: crate::locale::revision(),
                    binary_mode: false,
                    source: self.try_take_buffer(0)?,
                    captured: None,
                    trace: reader_intervals::Trace::None,
                    name: name.try_boxed()?,
                    awaiting: false,
                    closed: false,
                    probed: false,
                    native: Some(NativeRoots::new_in(self.registers.memory().clone())),
                    file,
                })
            })
        })();
        self.gc_note_control((self.loads.capacity() - capacity) * std::mem::size_of::<Loading>());
        let job = match prepared {
            Ok(job) => job,
            Err(error) => {
                if let Some(file) = file {
                    self.close_file_reader(file);
                }
                return Err(error);
            }
        };
        self.loads.push(job);
        self.gc_note_control(name.len());
        Ok(index)
    }
    pub(super) fn feed_native_load(
        &mut self,
        index: usize,
        bytes: Option<&[u8]>,
    ) -> Result<bool, Error> {
        let job = &mut self.loads[index];
        let first = !job.probed;
        job.probed = true;
        let bytes = bytes.unwrap_or_default();
        if first {
            job.binary_mode = bytes.first() == Some(&27);
        } else if bytes.is_empty() {
            job.closed = true;
        }
        let capacity = job.source.capacity();
        job.workspace
            .run(|| job.source.try_extend_from_slice(bytes))?;
        let growth = job.source.capacity() - capacity;
        self.gc_note_control(growth);
        Ok(first)
    }
    pub(super) fn resume_native_load(&mut self, index: usize) -> Result<Option<Program>, Error> {
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let result = job.resume(&mut self.compile_spare);
        let growth = job.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        result
    }
    pub(super) fn native_load_message(
        &self,
        index: usize,
        error: &Error,
    ) -> Result<Vec<u8>, Error> {
        let job = &self.loads[index];
        message(&job.name, error, job.binary_mode)
    }
    pub(super) fn end_native_load(&mut self, owner: usize) {
        if let Some(job) = self.loads.pop_if(|job| job.owner == owner) {
            if job.binary_mode || job.source.len() <= SPARE_SOURCE_LIMIT {
                job.parser.recycle(&mut self.compile_spare.parser);
            }
            self.recycle_buffer(job.source);
        }
    }
    /// Load a file, or stdin when `path` is absent, through the native Lua loader.
    ///
    /// This uses the VM's file streams, header handling and source/binary verifier.
    /// It does not consult the mutable global `loadfile` function. The returned
    /// function is retained in the result registers until the next VM mutation;
    /// pin it if retaining it across executions or collections.
    ///
    /// # Errors
    /// Returns the loader's error object for file, syntax or bytecode errors.
    pub fn load_file(&mut self, path: Option<&[u8]>) -> Result<FunctionId, Error> {
        let argument = path.map(|p| self.try_intern(p)).transpose()?;
        // Host loading is a parser operation, not an observable Lua builtin
        // invocation. The ordinary global loadfile still delivers its hooks.
        let empty = hooks::Hooks::new_in(self.registers.memory().clone());
        let hooks = std::mem::replace(&mut self.hooks, empty);
        let result = self
            .call_unprotected(Value::Native(LOAD_FILE), argument.as_slice())
            .map(|_| ());
        self.hooks = hooks;
        result?;
        match self.registers[0] {
            Value::Function(function) => Ok(function),
            Value::Nil => {
                let value = self.registers[1];
                let Value::String(_) = value else {
                    unreachable!("file loader returns a string diagnostic")
                };
                Err(self.host_error(value))
            }
            _ => unreachable!("file loader returns a function or nil"),
        }
    }

    fn file_load_raise(&self, value: Value) -> Error {
        // dofile calls lua_error on loadfile's diagnostic: that is a runtime
        // raise, including after a protected parser returned LUA_ERRMEM.
        if value == Value::String(self.memory_error) {
            return Error::raised_static(value, "not enough memory");
        }
        if matches!(value, Value::String(_)) {
            self.host_error(value)
        } else {
            Error::raised_static(value, "Lua error")
        }
    }
    fn file_load_result(
        &mut self,
        id: u64,
        frame: Frame,
        result: Result<FunctionId, Value>,
    ) -> Result<Option<usize>, Error> {
        let base = frame.base + frame.argc();
        match result {
            Ok(function) => {
                self.reserve_registers(base + 1)?;
                self.resize_registers(self.registers.len().max(base + 1));
                self.registers[base] = Value::Function(function);
                if id == LOAD_FILE {
                    return self.return_frame(frame, base, 1);
                }
                self.frames.last_mut().expect("active dofile").pc = 4;
                if let Some(actual) = self.start_call(base, 0, OPEN, false)? {
                    self.finish_call(base, actual, OPEN);
                }
                Ok(None)
            }
            Err(value) if id == DO_FILE => {
                self.frames.last_mut().expect("active dofile").pc = 4;
                Err(self.file_load_raise(value))
            }
            Err(value) => {
                self.reserve_registers(base + 2)?;
                self.resize_registers(self.registers.len().max(base + 2));
                self.registers[base] = Value::Nil;
                self.registers[base + 1] = value;
                self.return_frame(frame, base, 2)
            }
        }
    }
    pub(super) fn file_load_error(
        &mut self,
        name: &[u8],
        what: &[u8],
        code: i32,
    ) -> Result<Value, Error> {
        if code == 12 {
            return Err(self.memory_failure());
        }
        let mut bytes = self.try_take_buffer(name.len() + 64)?;
        bytes.try_extend_from_slice(b"cannot ")?;
        bytes.try_extend_from_slice(what)?;
        bytes.try_push(b' ')?;
        bytes.try_extend_from_slice(name.get(1..).unwrap_or_default())?;
        bytes.try_extend_from_slice(b": ")?;
        bytes.try_extend_from_slice(&io::message(code)?)?;
        self.try_finish_buffer(bytes)
    }
    fn finish_file_load(&mut self) {
        let job = self.loads.pop().expect("completed file reader");
        job.parser.recycle(&mut self.compile_spare.parser);
        if let Some(file) = job.file {
            self.close_file_reader(file);
        }
        if self.buffers.len() < self.buffers.capacity() {
            self.buffers.push(job.source);
        }
    }
    pub(super) fn open_file_reader(
        &mut self,
        path: Option<&[u8]>,
    ) -> Result<Result<FileReader, i32>, Error> {
        match path {
            None => Ok(Ok(FileReader {
                key: self.standard_stream(0).get(),
                owned: false,
                phase: 0,
            })),
            Some(path) => match io::open_input(path, false)? {
                Ok(stream) => {
                    let key = self
                        .io
                        .files
                        .try_insert(stream)
                        .map_err(|_| self.memory_failure())?;
                    Ok(Ok(FileReader {
                        key,
                        owned: true,
                        phase: 0,
                    }))
                }
                Err(code) => Ok(Err(code)),
            },
        }
    }
    pub(super) fn close_file_reader(&mut self, file: FileReader) {
        if file.owned {
            self.io.files.remove(file.key);
        }
    }
    pub(super) fn prepare_file_reader(
        &mut self,
        file: &mut FileReader,
        name: &[u8],
    ) -> Result<Vec<u8>, (i32, bool)> {
        let stream = &mut self.io.files[slot_index(file.key)];
        let before = stream.heap_bytes();
        let mut prefix = Vec::new();
        let mut binary = false;
        let result = loop {
            match file.prepare(stream, name, &mut prefix, &mut binary) {
                Ok(true) => break Ok(prefix),
                Ok(false) => {}
                Err(error) => break Err(error),
            }
        };
        self.io.files.note_alloc(
            self.io.files[slot_index(file.key)]
                .heap_bytes()
                .saturating_sub(before),
        );
        result
    }
    pub(super) fn read_file_reader(&mut self, file: FileReader) -> Result<Option<Vec<u8>>, i32> {
        let stream = &mut self.io.files[slot_index(file.key)];
        let before = stream.heap_bytes();
        let mut bytes = Vec::new();
        let result = stream.read_chars(&mut bytes, io::LINE_BUFFER);
        let growth = stream.heap_bytes().saturating_sub(before);
        self.io.files.note_alloc(growth);
        result?;
        Ok((!bytes.is_empty()).then_some(bytes))
    }
    pub(super) fn file_load_step(
        &mut self,
        id: u64,
        mut frame: Frame,
    ) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        if frame.pc == 2 {
            if id == DO_FILE {
                return Err(self.file_load_raise(self.registers[frame.base + 1]));
            }
            return self.return_frame(frame, frame.base, 2);
        }
        if frame.pc == 4 {
            let base = frame.base + frame.argc();
            return self.return_frame(frame, base, self.top - base);
        }
        if frame.pc == 0 {
            let args = frame.vararg_start - 1;
            let workspace = Workspace::new(self.registers.memory())?;
            let (name, opened) = workspace.run(|| -> Result<_, Error> {
                Ok(if self.argument(args, frame.argc(), 0) == Value::Nil {
                    (b"=stdin".try_to_vec()?, self.open_file_reader(None)?)
                } else {
                    let path = self.string_argument(args, frame.argc(), 0)?;
                    self.registers[frame.vararg_start] = Value::String(path);
                    let path = io::c_bytes(&self.strings.bytes[slot_index(path.0)]);
                    let mut name = fallible::with_capacity(path.len() + 1)?;
                    name.try_push(b'@')?;
                    name.try_extend_from_slice(path)?;
                    let opened = self.open_file_reader(Some(&name[1..]))?;
                    (name, opened)
                })
            })?;
            // Preserve all original arguments while the loaded chunk executes.
            self.registers.copy_within(
                frame.vararg_start..frame.vararg_start + frame.argc(),
                frame.base,
            );
            frame.vararg_start = frame.base;
            frame.pc = 1;
            self.frames[owner] = frame;
            let file = match opened {
                Ok(file) => file,
                Err(code) => {
                    let value = self.file_load_error(&name, b"open", code)?;
                    return self.file_load_result(id, frame, Err(value));
                }
            };
            let before = self.loads.capacity();
            let prepared = workspace.run(|| -> Result<Loading, Error> {
                self.loads
                    .try_reserve(1)
                    .map_err(|_| self.memory_failure())?;
                let depth = self.native_depth(owner + 1);
                let parser = if crate::memory::compilation::active() {
                    crate::parser::Parser::with_native_depth(depth)?
                } else {
                    crate::parser::Parser::try_new_in(&mut self.compile_spare.parser, depth)?
                };
                Ok(Loading {
                    owner,
                    workspace: workspace.clone(),
                    parser,
                    decoder: None,
                    native_depth: self.native_depth(owner + 1),
                    parse_locale: crate::locale::revision(),
                    binary_mode: false,
                    source: self.try_take_buffer(0)?,
                    captured: None,
                    trace: reader_intervals::Trace::None,
                    name: name.try_into_boxed_slice()?,
                    awaiting: false,
                    closed: false,
                    probed: false,
                    native: None,
                    file: Some(file),
                })
            });
            self.gc_note_control((self.loads.capacity() - before) * std::mem::size_of::<Loading>());
            match prepared {
                Ok(job) => self.loads.push(job),
                Err(error) => {
                    self.close_file_reader(file);
                    return Err(error);
                }
            }
        }
        let index = self.loads.len() - 1;
        debug_assert_eq!(self.loads[index].owner, owner);
        let file = self.loads[index].file.expect("active file reader");
        let stream = &mut self.io.files[slot_index(file.key)];
        let before = stream.heap_bytes();
        let source_before = self.loads[index].source.capacity();
        let read = self.loads[index].read_file(stream, self.budget.is_none());
        let growth = stream.heap_bytes().saturating_sub(before);
        self.io.files.note_alloc(growth);
        self.gc_note_control(self.loads[index].source.capacity() - source_before);
        if let Err((code, reopen)) = read {
            let name = self.loads[index].name.try_to_vec()?;
            let value = self.file_load_error(&name, if reopen { b"reopen" } else { b"read" }, code);
            self.finish_file_load();
            return self.file_load_result(id, frame, Err(value?));
        }
        if matches!(self.loads[index].file.expect("file reader").phase, 1 | 3) {
            return Ok(None);
        }
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let parsed = job.resume(&mut self.compile_spare);
        let growth = job.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        let compiled = match parsed {
            Ok(Some(program)) => Some(Ok(program)),
            Err(error) => Some(Err(error)),
            Ok(None) => None,
        };
        if let Some(compiled) = compiled {
            let result = match compiled {
                Ok(program) => self.load_fresh(&program).map_err(|error| {
                    error
                        .value()
                        .expect("binding failure has fixed Lua diagnostic")
                }),
                Err(error) => {
                    if error.kind() == ErrorKind::Memory {
                        Err(Value::String(self.memory_error))
                    } else {
                        let message = message(
                            &self.loads[index].name,
                            &error,
                            self.loads[index].binary_mode,
                        )?;
                        Err(self.try_intern(&message)?)
                    }
                }
            };
            self.finish_file_load();
            return self.file_load_result(id, frame, result);
        }
        Ok(None)
    }
    pub(super) fn trim_loads(&mut self) {
        // Cleanup also runs while reporting an allocation failure. Keep the
        // existing control storage and inspect each owner's frame directly.
        let mut jobs = std::mem::take(&mut self.loads);
        jobs.retain(|j| {
            let keep = self.frames.get(j.owner).is_some_and(|frame| {
                protected(self.body_kind(j.owner, frame.function)) && matches!(frame.pc, 1 | 3)
            });
            if !keep
                && let Some(file) = j.file
                && file.owned
            {
                self.io.files.remove(file.key);
            }
            keep
        });
        self.loads = jobs;
    }
    pub(super) fn parser_nesting_for(&self, thread: ThreadId, end: usize) -> usize {
        let loads = if thread == self.thread {
            &self.loads
        } else {
            &self.threads[slot_index(thread.0)].loads
        };
        loads
            .iter()
            .filter(|j| j.owner < end)
            .map(|j| {
                j.decoder.as_ref().map_or_else(
                    || j.trace.nesting().unwrap_or_else(|| j.parser.nesting()),
                    |d| d.nesting(),
                )
            })
            .sum()
    }
    pub(super) fn load_step(&mut self, mut frame: Frame) -> Result<Option<usize>, Error> {
        let owner = self.frames.len() - 1;
        let base = frame.base;
        if frame.pc == 2 {
            return self.return_frame(frame, base + 3, 2);
        }
        if frame.pc == 0 {
            let args = frame.vararg_start - 1;
            let workspace = Workspace::new(self.registers.memory())?;
            let name = workspace.run(|| -> Result<_, Error> {
                Ok(match self.argument(args, frame.argc(), 1) {
                    Value::Nil => b"=(load)".try_to_vec()?,
                    _ => {
                        let id = self.string_argument(args, frame.argc(), 1)?;
                        self.strings.bytes[slot_index(id.0)].try_to_vec()?
                    }
                })
            })?;
            let reader = self.argument(args, frame.argc(), 0);
            if !matches!(reader, Value::Function(_) | Value::Native(_)) {
                return Err(self.argument_type_error(args, frame.argc(), 0, "function"));
            }
            let supplied_name = self.argument(args, frame.argc(), 1);
            let supplied_name = match supplied_name {
                Value::Number(n) => self.intern_number(n)?,
                v => v,
            };
            // lua_settop(3) retains an already supplied third argument until
            // the generic reader replaces its reserved result slot.
            let reserved = self.argument(args, frame.argc(), 2);
            self.registers[frame.vararg_start..frame.vararg_start + frame.argc()].fill(Value::Nil);
            self.registers[base] = reader;
            self.registers[base + 1] = supplied_name;
            self.registers[base + 2] = reserved;
            frame.vararg_start = base;
            frame.vararg_count = 3;
            frame.pc = 1;
            self.frames[owner] = frame;
            let index = self.start_native_load(owner, &name, self.native_depth(owner + 1), None)?;
            self.loads[index].trace = reader_intervals::Trace::Waiting;
        }
        let index = self.loads.len() - 1;
        debug_assert_eq!(self.loads[index].owner, owner);
        if self.loads[index].awaiting {
            self.loads[index].awaiting = false;
            // f_parser probes with luaZ_lookahead before luaY_parser fetches
            // its first character. An empty first probe is not sticky EOF.
            let first = !self.loads[index].probed;
            self.loads[index].probed = true;
            let result = self.registers[base + frame.argc()];
            let result = if let Value::Number(n) = result {
                self.intern_number(n)?
            } else {
                result
            };
            match result {
                Value::Nil => {
                    // generic_reader leaves its EOF nil on the parser stack.
                    self.loads[index].closed = !first;
                    frame.vararg_count += 1;
                }
                Value::String(id) => {
                    self.registers[base + 2] = result;
                    if first && let Some(compiled) = self.try_complete_reader(index, base, id)? {
                        self.check_calls()?;
                        self.registers[base + frame.argc()] = Value::Nil;
                        frame.vararg_count += 1;
                        self.truncate_registers(base + frame.argc());
                        return self.finish_reader(index, frame, compiled, false);
                    }
                    if first
                        && self.strings.bytes[slot_index(id.0)]
                            .first()
                            .is_some_and(|&first| first != 27)
                        && self.reader_can_omit_roots(index, self.registers[base])
                    {
                        self.loads[index].parser.disable_native_roots();
                        if self.prepare_captured_input(index, self.registers[base], id)? {
                            self.truncate_registers(base + frame.argc());
                            return self.call_load_reader(index, frame);
                        }
                    }
                    if self.loads[index].captured.is_some()
                        && !self.strings.bytes[slot_index(id.0)].is_empty()
                    {
                        if self.match_captured_fragment(index, id) {
                            self.truncate_registers(base + frame.argc());
                            return self.call_load_reader(index, frame);
                        }
                        self.replay_captured_prefix(index)?;
                    }
                    let bytes = &self.strings.bytes[slot_index(id.0)];
                    let job = &mut self.loads[index];
                    if first {
                        job.binary_mode = bytes.first() == Some(&27);
                    }
                    let capacity = job.source.capacity();
                    job.workspace
                        .run(|| job.source.try_extend_from_slice(bytes))?;
                    if bytes.is_empty() && !first {
                        job.closed = true;
                    }
                    let growth = job.source.capacity() - capacity;
                    self.gc_note_control(growth);
                }
                _ => {
                    return Err(Error::runtime_static(
                        "reader function must return a string",
                    ));
                }
            }
            self.truncate_registers(base + frame.argc());
        }
        if let Some(input) = &self.loads[index].captured {
            debug_assert!(self.loads[index].closed);
            let bytes = &self.strings.bytes[slot_index(input.source.0)];
            if input.read == bytes.len() {
                // Wait for the reader's actual EOF before publishing code.
                let compiled = if let Some(variant) = &input.variant {
                    variant.publish()
                } else {
                    let job = &mut self.loads[index];
                    let candidate = source_templates::Preamble::scan(bytes)
                        .and_then(|preamble| preamble.check_resources(&job.parser));
                    self.compile_spare.finish_text(
                        job.parser.take_reader_input(),
                        bytes,
                        &job.name,
                        job.native_depth,
                        job.parse_locale,
                        candidate,
                    )
                };
                self.frames[owner] = frame;
                return self.finish_reader(index, frame, compiled, false);
            }
            self.replay_captured_prefix(index)?;
        }
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let parsed = job.resume(&mut self.compile_spare);
        let growth = job.arena_bytes().saturating_sub(before);
        let has_parser_roots = job.binary_mode || job.parser.native_roots_enabled();
        self.gc_note_control(growth);
        let mut top = frame.argc();
        if has_parser_roots {
            self.apply_native_load_roots(index, base, &mut top, base)?;
        }
        frame.vararg_count = top as u32;
        self.frames[owner] = frame;
        let compiled = match parsed {
            Ok(Some(program)) => Some(Ok(program)),
            Err(error) => Some(Err(error)),
            Ok(None) => None,
        };
        if let Some(compiled) = compiled {
            return self.finish_reader(index, frame, compiled, has_parser_roots);
        }
        self.call_load_reader(index, frame)
    }

    fn call_load_reader(&mut self, index: usize, frame: Frame) -> Result<Option<usize>, Error> {
        let frames = self.frames.len();
        let thread = &self.threads[slot_index(self.thread.0)];
        let inherited = usize::from(thread.depth) + usize::from(thread.native_bias) + 1;
        // Each frame contributes at most one native boundary, and each
        // adjacent pair at most one iterator boundary. Count exact depth
        // only when this bound cannot prove room for the reader call.
        let entered = frames
            .saturating_mul(2)
            .saturating_add(self.parser_nesting_for(self.thread, frames))
            .saturating_add(inherited);
        if entered >= usize::from(limits::MAX_C_CALL_DEPTH) {
            self.check_c_call()?;
        }
        self.loads[index].awaiting = true;
        let call = frame.base + frame.argc();
        self.reserve_registers(call + 1)?;
        self.resize_registers(self.registers.len().max(call + 1));
        self.registers[call] = self.registers[frame.base];
        // Preparing the result slot leaves the checked C depth valid.
        if let Some(actual) = self.start_lua_call(call, 0, 1, false)? {
            self.finish_call(call, actual, 1);
        }
        Ok(None)
    }

    #[cold]
    fn prepare_captured_input(
        &mut self,
        index: usize,
        reader: Value,
        fragment: StringId,
    ) -> Result<bool, Error> {
        // A parked streaming parser can contribute to the reader's C depth.
        // Leave headroom for the grammar and the reader's own built-in calls.
        if self.loads[index].native_depth + 2 * usize::from(crate::parser::MAX_RECURSIVE_DEPTH) + 4
            >= usize::from(limits::MAX_C_CALL_DEPTH)
        {
            return Ok(false);
        }
        let fragment = &self.strings.bytes[slot_index(fragment.0)];
        let Some(source) = reader::captured_source(self, reader, fragment) else {
            return Ok(false);
        };
        let read = fragment.len();
        let bytes = &self.strings.bytes[slot_index(source.0)];
        let job = &mut self.loads[index];
        let before = job.arena_bytes();
        let variant = source_templates::Preamble::scan(bytes).and_then(|preamble| {
            self.compile_spare.source_templates.find(
                bytes,
                &job.name,
                job.native_depth,
                job.parse_locale,
                &preamble,
            )
        });
        // A valid complete parse lets us defer parsing the matching prefix.
        // Each reader call still runs and must independently supply its bytes.
        let prepared = variant.is_some() || job.parser.prepare_reader_input(bytes)?;
        let growth = job.arena_bytes().saturating_sub(before);
        self.gc_note_control(growth);
        if prepared {
            let value = Value::String(source);
            self.reserve_gc_value(value)?;
            self.loads[index].captured = Some(CapturedInput {
                source,
                read,
                variant,
            });
            self.gc_new_value(value);
        }
        Ok(prepared)
    }

    fn match_captured_fragment(&mut self, index: usize, fragment: StringId) -> bool {
        let input = self.loads[index].captured.as_mut().expect("prepared input");
        let source = &self.strings.bytes[slot_index(input.source.0)];
        let bytes = &self.strings.bytes[slot_index(fragment.0)];
        if source[input.read..].starts_with(bytes) {
            input.read += bytes.len();
            true
        } else {
            false
        }
    }

    #[cold]
    fn replay_captured_prefix(&mut self, index: usize) -> Result<(), Error> {
        let job = &mut self.loads[index];
        let input = job.captured.take().expect("prepared input");
        // The bytes preceding the mismatch were checked at each reader boundary.
        let bytes = &self.strings.bytes[slot_index(input.source.0)][..input.read];
        let capacity = job.source.capacity();
        job.workspace.run(|| {
            job.source.try_extend_from_slice(bytes)?;
            if input.variant.is_none() {
                job.parser.discard_reader_input()?;
            }
            Ok(())
        })?;
        self.gc_note_control(self.loads[index].source.capacity() - capacity);
        Ok(())
    }
    fn reader_can_omit_roots(&self, index: usize, reader: Value) -> bool {
        let job = &self.loads[index];
        job.parser.reusable()
            && self.reader_context_allows_root_elision()
            && reader::cannot_inspect_stack(self, reader)
            && !self.load_has_inherited_error_handler(job.owner)
            && !self.gc_may_call_out()
    }

    fn reader_context_allows_root_elision(&self) -> bool {
        self.hooks.mask == 0
            && self.budget.is_none()
            && !self.native_protected
            && self.native_bodies.is_empty()
            && self.gc.running == 0
            && !self.gc.closing
            && self.gc.finalizers.is_empty()
    }

    fn try_complete_reader(
        &mut self,
        index: usize,
        base: usize,
        source: StringId,
    ) -> Result<Option<Result<Program, Error>>, Error> {
        let job = &self.loads[index];
        let bytes = &self.strings.bytes[slot_index(source.0)];
        if !job.parser.reusable()
            || job.native_depth + usize::from(crate::parser::MAX_RECURSIVE_DEPTH) + 1
                >= usize::from(limits::MAX_C_CALL_DEPTH)
            || bytes.is_empty()
            || bytes.len() > SPARE_SOURCE_LIMIT
            || bytes[0] == 27
            || !self.reader_context_allows_root_elision()
            || !eof::returns_nil(self, self.registers[base])
            || self.gc_may_call_out()
        {
            return Ok(None);
        }

        // The EOF path cannot call out or change Lua state. A valid complete
        // parse can consume this borrowed block without publishing parser roots.
        let job = &mut self.loads[index];
        let preamble = source_templates::Preamble::scan(bytes);
        if let Some(preamble) = &preamble
            && let Some(variant) = self.compile_spare.source_templates.find(
                bytes,
                &job.name,
                job.native_depth,
                job.parse_locale,
                preamble,
            )
        {
            return Ok(Some(variant.publish()));
        }
        let Some(ast) = job.parser.try_complete_reader(bytes)? else {
            return Ok(None);
        };
        let candidate = preamble.and_then(|preamble| preamble.check_resources(&job.parser));
        let result = self.compile_spare.finish_text(
            ast,
            bytes,
            &job.name,
            job.native_depth,
            job.parse_locale,
            candidate,
        );
        Ok(Some(result))
    }

    fn finish_reader(
        &mut self,
        index: usize,
        mut frame: Frame,
        compiled: Result<Program, Error>,
        has_parser_roots: bool,
    ) -> Result<Option<usize>, Error> {
        let owner = self.loads[index].owner;
        let program = match compiled {
            Ok(program) => program,
            Err(error) if error.kind() == ErrorKind::Memory => return Err(self.memory_failure()),
            Err(error) => {
                let job = &self.loads[index];
                let diagnostic = message(&job.name, &error, job.binary_mode)?;
                let value = self.try_intern(&diagnostic)?;
                self.protected_failure(owner, value);
                return Ok(None);
            }
        };
        let function = self.load_fresh(&program)?;
        if has_parser_roots {
            self.retain_load_prototypes(index, function)?;
        }
        let job = &mut self.loads[index];
        self.compile_spare.reader_intervals.remember(
            &job.source,
            std::mem::take(&mut job.trace),
            &program,
        );
        self.end_native_load(owner);
        let result = frame.base + frame.argc();
        self.reserve_registers(result + 1)?;
        self.resize_registers(self.registers.len().max(result + 1));
        self.registers[result] = Value::Function(function);
        frame.vararg_count += 1;
        self.frames[owner] = frame;
        self.return_frame(frame, result, 1)
    }

    pub(super) fn retain_load_prototypes(
        &mut self,
        index: usize,
        function: FunctionId,
    ) -> Result<(), Error> {
        let mut prototypes = Buffer::new_in(self.programs.memory().clone());
        let parsed = &self.loads[index].native.as_ref().unwrap().prototypes;
        prototypes
            .try_reserve_exact(parsed.len())
            .map_err(|_| self.memory_failure())?;
        prototypes.extend_from_slice(parsed);
        let closure = &self.functions[slot_index(function.0)];
        let entry = closure.prototype;
        let loaded = closure.program;
        assert_eq!(
            prototypes.len(),
            self.programs[loaded].data.prototypes.len()
        );
        let root = prototypes[entry];
        self.reserve_gc_value(Value::ParserPrototype(root))?;
        let before =
            self.programs[loaded].parser_prototypes.capacity() * size_of::<ParserPrototypeId>();
        let bytes = prototypes.capacity() * size_of::<ParserPrototypeId>();
        self.programs.note_alloc(bytes);
        self.programs[loaded].parser_prototypes = prototypes;
        self.object_payload_bytes = self.object_payload_bytes - before + bytes;
        self.loads[index]
            .native
            .as_mut()
            .unwrap()
            .prototypes
            .clear();
        self.gc_new_value(Value::ParserPrototype(root));
        Ok(())
    }
}

#[cfg(test)]
mod allocation_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_templates_preserve_observed_parser_roots_and_reuse_code_after_eof() {
        let program = crate::compile(br#"
            local function read(number)
                local first = "local function version() return " .. number .. " end; local kept = 5; "
                local tail = "return version() + kept --" .. string.rep('x', 350)
                local reads = 0
                local fn = assert(load(function()
                    reads = reads + 1
                    if reads == 1 then return first end
                    if reads == 2 then
                        local _, roots = debug.getlocal(2, 4)
                        assert(type(roots) == 'table' and roots.kept == true)
                        roots.touched = number
                        return tail
                    end
                    local _, roots = debug.getlocal(2, 4)
                    assert(roots.touched == number)
                end, '@observed.lua'))
                assert(reads == 3 and fn() == number + 5)
                return fn
            end
            local first = read(19)
            local second = read(372)
            return first, second
        "#).unwrap();
        let mut vm = Vm::new();
        let [Value::Function(first), Value::Function(second)] = *vm.execute(&program).unwrap()
        else {
            panic!("two loaded functions");
        };
        let [first, second] = [first, second].map(|id| {
            let closure = &vm.functions[slot_index(id.0)];
            let program = &vm.programs[closure.program];
            assert!(!program.parser_prototypes.is_empty());
            program.data.prototypes[closure.prototype].code.as_ptr()
        });
        assert_eq!(first, second);
    }

    #[test]
    fn constant_binding_reuses_live_ids_and_rejects_recycled_slots() {
        let mut vm = Vm::new();
        let text = b"cold\0const\xffvalue";
        let first = vm.bind_constant_string(text).unwrap();
        assert_eq!(vm.bind_constant_string(text).unwrap(), first);
        let Value::String(first) = first else {
            unreachable!();
        };

        vm.collect_garbage().unwrap();
        assert!(!vm.strings.bytes.contains(first.0));
        let Value::String(other) = vm.try_intern(b"slot filled again").unwrap() else {
            unreachable!();
        };
        assert_eq!(slot_index(other.0), slot_index(first.0));

        let Value::String(rebound) = vm.bind_constant_string(text).unwrap() else {
            unreachable!();
        };
        assert_ne!(rebound, first);
        assert_ne!(rebound, other);
        assert_eq!(vm.strings.bytes[slot_index(rebound.0)].as_ref(), text);
    }
}
