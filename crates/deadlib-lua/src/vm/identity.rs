//! Stable opaque object addresses, independent of moving arena storage.
use super::*;

#[cfg(test)]
mod allocation_tests;

impl Vm {
    /// Return a stable opaque pointer identity for a live object in this VM.
    /// Tables, Lua/native functions and threads have allocator-owned tokens
    /// prepared before they are published. Querying them never allocates.
    /// Userdata returns its aligned payload address; light userdata returns its
    /// unmanaged address. Strings, other scalars and collected handles yield None.
    /// Opaque tokens are for comparison/debugging and must not be dereferenced.
    /// Userdata and unmanaged addresses keep their usual payload/host contracts.
    /// Retaining it does not keep the Lua object alive; it may be reused after GC.
    pub fn object_pointer(&mut self, value: Value) -> Option<usize> {
        if !self.is_live_value(value) {
            return None;
        }
        match value {
            Value::Table(id) => self.tables.pointer(id.0),
            Value::Function(id) => self.functions.pointer(id.0),
            Value::Native(id) if id >= strings::ITERATOR_FIRST => self.native_closures.pointer(id),
            Value::Native(id) => {
                Some(&self.builtin_envs[id as usize] as *const Option<TableId> as usize)
            }
            Value::Thread(id) => self.threads.pointer(id.0),
            Value::Userdata(id) => {
                Some(self.userdata[slot_index(id.0)].bytes.as_ref().as_ptr() as usize)
            }
            Value::LightUserdata(pointer) => Some(pointer as usize),
            _ => None,
        }
    }
}
pub(super) fn pointer_display(pointer: usize) -> impl std::fmt::Display {
    struct Pointer(usize);
    impl std::fmt::Display for Pointer {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let pointer = self.0;
            if cfg!(all(windows, target_env = "msvc")) {
                write!(
                    formatter,
                    "{pointer:0width$X}",
                    width = std::mem::size_of::<usize>() * 2
                )
            } else if cfg!(target_os = "linux") && pointer == 0 {
                formatter.write_str("(nil)")
            } else {
                write!(formatter, "0x{pointer:x}")
            }
        }
    }
    Pointer(pointer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreachable_identities_are_reused_without_relocating_survivors() {
        let mut vm = Vm::new();
        let retained = Value::Table(vm.new_table());
        vm.set_global(b"retained", retained).unwrap();
        let pointer = vm.object_pointer(retained).unwrap();
        let mut previous = 0;
        for cycle in 0..4 {
            for _ in 0..1000 {
                let value = Value::Table(vm.new_table());
                vm.object_pointer(value).unwrap();
            }
            vm.collect_garbage().unwrap();
            let storage = vm.tables.storage_bytes();
            if cycle > 0 {
                assert_eq!(storage, previous);
            }
            previous = storage;
            assert_eq!(vm.object_pointer(retained), Some(pointer));
        }
    }
}
