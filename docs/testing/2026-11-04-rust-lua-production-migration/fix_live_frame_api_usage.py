from pathlib import Path
import re
r=Path('/mnt/c/GitHub/deadsync/crates/deadsync-song-lua')
p=r/'src/rust_engine/live_frames.rs';s=p.read_text()
s=s.replace('use deadlib_lua::{ErrorKind, NativeStack};','use deadlib_lua::NativeStack;')
s=re.sub(r'stack\.pop\((\d+)\)',r'stack.set_top(stack.top() - \1)',s)
a=s.index('        if !context.live {');b=s.index('        let catalog',a)
s=s[:a]+'''        if !context.live {
            return Err(frame_error(self, b"song live frames require a live context"));
        }
'''+s[b:]
a=s.index('    fn owners(');b=s.index('    /// Prepare before',a)
s=s[:a]+'''    fn owners(&self, runtime: &mut RustSongRuntime) -> Result<Value, Error> {
        let Some(owner) = self.owners else {
            return Err(frame_error(runtime, b"released song live frames"));
        };
        runtime.vm.rooted(owner)
    }

'''+s[b:]
s=s.replace('runtime.vm.unpin(self.owners.take().unwrap())','runtime.vm.unpin(self.owners.take().unwrap()).map(|_| ())')
s+='''
fn frame_error(runtime: &mut RustSongRuntime, message: &[u8]) -> Error {
    match runtime.vm.with_native_stack(|stack| Ok(stack.error(message))) {
        Ok(error) | Err(error) => error,
    }
}
'''
p.write_text(s)
p=r/'src/rust_engine/actors/live_frames.rs';s=p.read_text().replace('use super::*;','use super::*;\nuse crate::LUA_PLAYERS;');p.write_text(s)
