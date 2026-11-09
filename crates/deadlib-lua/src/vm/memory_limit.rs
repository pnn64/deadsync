use super::*;

/// Usage of the VM's bounded allocation domain.
///
/// This covers state-owned buffers, tables, strings, roots, userdata and host
/// binding storage, plus a charge for each loaded program's immutable storage.
/// Each VM charges shared program data independently, once per loaded binding.
/// Loading through the VM also charges compiler allocation requests until the
/// compilation job ends, conservatively bounding workspace growth. Standalone
/// compilation and allocations indirectly owned by opaque Rust binding data
/// are not included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryUsage {
    /// Allocated capacities, shared programs and active compiler charges.
    pub bytes: usize,
    /// Largest simultaneous byte count observed by this domain.
    pub peak_bytes: usize,
    /// The configured byte ceiling.
    pub limit_bytes: usize,
}

impl Vm {
    /// Compile and load source or native Lua bytecode using this VM's budget.
    ///
    /// Compiler workspace is conservatively charged for its allocation requests
    /// until compilation ends. The loaded program is then charged once for its
    /// retained storage. The returned closure occupies the result registers;
    /// pin it before retaining it across later calls or collections.
    ///
    /// # Errors
    /// Returns syntax, bytecode or memory errors without running the chunk.
    pub fn try_load_source(&mut self, source: &[u8], name: &[u8]) -> Result<FunctionId, Error> {
        let program = crate::memory::compilation::Workspace::new(self.registers.memory())?
            .run(|| crate::compile_named(source, name))?;
        self.try_load_program(&program)
    }

    /// Create a VM whose state-owned allocation domain cannot exceed `bytes`.
    ///
    /// This is a limit on the storage described by [`MemoryUsage`], not a total
    /// process-memory limit. Use a separate bound for standalone compilation.
    /// Growth beyond the ceiling returns a Lua memory error; releasing storage
    /// remains possible. The limit includes standard-library initialization.
    ///
    /// # Errors
    /// Returns a memory error if initialization cannot fit or allocate. As with
    /// [`Self::try_new`], an initialization error contains no VM-owned values.
    pub fn try_new_with_memory_limit(bytes: usize) -> Result<Self, Error> {
        let memory = Memory::bounded(bytes)?;
        let mut vm = Self::try_bare_in(memory).map_err(Error::without_vm_values)?;
        vm.initializing = true;
        vm.open_libraries().map_err(Error::without_vm_values)?;
        vm.initializing = false;
        Ok(vm)
    }

    /// Read charged bytes and the high-water mark for a bounded VM.
    /// Ordinary unlimited VMs return `None`.
    pub fn memory_usage(&self) -> Option<MemoryUsage> {
        self.registers
            .memory()
            .budget_usage()
            .map(|(bytes, peak_bytes, limit_bytes)| MemoryUsage {
                bytes,
                peak_bytes,
                limit_bytes,
            })
    }
}
