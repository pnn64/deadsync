//! Stable opaque object addresses, independent of moving arena storage.
use super::*;

impl Vm {
    /// Return a stable opaque pointer identity for a live object in this VM.
    /// Tables, Lua/native functions and threads receive lazily owned tokens.
    /// Userdata returns its aligned payload address; light userdata returns its
    /// unmanaged address. Strings, other scalars and collected handles yield None.
    /// Opaque tokens are for comparison/debugging and must not be dereferenced.
    /// Userdata and unmanaged addresses keep their usual payload/host contracts.
    /// Retaining it does not keep the Lua object alive; it may be reused after GC.
    pub fn object_pointer(&mut self, value: Value) -> Option<usize> {
        if !self.is_live_value(value) {
            return None;
        }
        let key = match value {
            Value::Table(id) => (0, id.0),
            Value::Function(id) => (1, id.0),
            Value::Native(id) => (2, id),
            Value::Thread(id) => (3, id.0),
            Value::Userdata(id) => {
                return Some(self.userdata[slot_index(id.0)].bytes.as_ref().as_ptr() as usize);
            }
            Value::LightUserdata(pointer) => return Some(pointer as usize),
            _ => return None,
        };
        let before = self.object_pointers.capacity();
        let inserted = !self.object_pointers.contains_key(&key);
        let token = self
            .object_pointers
            .entry(key)
            .or_insert_with(|| Box::new(0));
        let pointer = token.as_ref() as *const usize as usize;
        let growth = (self.object_pointers.capacity() - before)
            * (std::mem::size_of::<((u8, u64), Box<usize>)>() + 1)
            + usize::from(inserted) * std::mem::size_of::<usize>();
        self.gc_note_control(growth);
        Some(pointer)
    }
    pub(super) fn prune_object_pointers(&mut self) {
        let mut pointers = std::mem::take(&mut self.object_pointers);
        pointers.retain(|&(kind, id), _| match kind {
            0 => self.tables.contains(id),
            1 => self.functions.contains(id),
            2 => self.is_live_value(Value::Native(id)),
            3 => self.threads.contains(id),
            _ => unreachable!(),
        });
        self.object_pointers = pointers;
    }
}
pub(super) fn pointer_text(pointer: usize) -> String {
    if cfg!(all(windows, target_env = "msvc")) {
        format!(
            "{pointer:0width$X}",
            width = std::mem::size_of::<usize>() * 2
        )
    } else if cfg!(target_os = "linux") && pointer == 0 {
        "(nil)".to_owned()
    } else {
        format!("0x{pointer:x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreachable_identity_tokens_are_pruned_without_relocating_survivors() {
        let mut vm = Vm::new();
        let retained = Value::Table(vm.new_table());
        vm.set_global(b"retained", retained).unwrap();
        let pointer = vm.object_pointer(retained).unwrap();
        for _ in 0..4 {
            for _ in 0..1000 {
                let value = Value::Table(vm.new_table());
                vm.object_pointer(value).unwrap();
            }
            vm.collect_garbage().unwrap();
            assert_eq!(vm.object_pointers.len(), 1);
            assert_eq!(vm.object_pointer(retained), Some(pointer));
        }
    }
}
