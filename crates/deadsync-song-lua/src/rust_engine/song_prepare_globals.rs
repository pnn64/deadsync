//! The existing compiler's deep schedule snapshot around overlay extraction.
use super::song_compiler_roots::Roots;
use super::*;
use deadlib_lua::NativeStack;
use std::{path::Path, sync::Arc};

const KEYS: [&str; 6] = [
    "prefix_globals",
    "mods",
    "mod_time",
    "mods_ease",
    "mod_perframes",
    "mod_actions",
];
type Kind<S, V> = crate::SongLuaOverlayKind<S, V, deadlib_present::actors::TextAttribute>;
impl RustSongRuntime {
    /// Static compiler preparation. Clone raw keys/values without metatables or
    /// alias sharing, collect overlays, then restore the six bindings before
    /// propagating collector errors. The live compiler uses the direct collector.
    pub fn read_song_preparation_overlay_actors<S, V>(
        &mut self,
        root: Value,
        context: &SongLuaCompileContext,
        slots: impl Fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>,
        layer: impl Fn(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
        skipped: impl FnMut(String),
    ) -> Result<SongOverlayActors<Kind<S, V>>, String> {
        let mut roots = Roots::default();
        let result = (|| {
            roots.keep(&mut self.vm, root)?;
            let globals = self.compiler_globals()?;
            roots.keep(&mut self.vm, globals)?;
            let mut saved = Vec::new();
            for key in KEYS {
                let value = self.compiler_value_field(globals, key)?;
                let input = roots.keep(&mut self.vm, value)?;
                let value = self.compiler_clone_value(value)?;
                saved.push(roots.keep(&mut self.vm, value)?);
                roots.release(&mut self.vm, input)?;
            }
            let output = self.read_song_overlay_actors(root, context, slots, layer, skipped);
            let restored = (|| {
                for (key, index) in KEYS.into_iter().zip(saved) {
                    let value = roots.value(&self.vm, index)?;
                    let set = self
                        .vm
                        .new_host_function_with_data(
                            |call| {
                                let key = call.function_data::<String>().unwrap().clone();
                                super::set(call, call.captures()[0], &key, call.captures()[1])
                            },
                            &[globals, value],
                            key.to_owned(),
                        )
                        .map_err(|e| e.to_string())?;
                    self.vm.call(set, &[]).map_err(|e| e.to_string())?;
                    roots.release(&mut self.vm, index)?;
                }
                Ok::<(), String>(())
            })();
            if let Err(error) = restored {
                if let Ok(mut output) = output {
                    self.release_song_overlay_actors(&mut output)
                        .map_err(|e| e.to_string())?;
                }
                return Err(error);
            }
            output
        })();
        roots.clear(&mut self.vm)?;
        result
    }
    fn compiler_clone_value(&mut self, value: Value) -> Result<Value, String> {
        let clone = self
            .vm
            .new_host_function(
                |call| {
                    let value = call.captures()[0];
                    let mut stack = call.native_stack()?;
                    stack.push(value)?;
                    clone_value(&mut stack, 1)?;
                    stack.finish(1)
                },
                &[value],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.vm.call(clone, &[]).map_err(|e| e.to_string())?[0])
    }
    pub(super) fn compiler_value_field(
        &mut self,
        source: Value,
        name: &str,
    ) -> Result<Value, String> {
        let read = self
            .vm
            .new_host_function_with_data(
                |call| {
                    let key = call.function_data::<String>().unwrap().clone();
                    let value = super::field(call, call.captures()[0], &key)?;
                    call.push(value)
                },
                &[source],
                name.to_owned(),
            )
            .map_err(|e| e.to_string())?;
        Ok(self.vm.call(read, &[]).map_err(|e| e.to_string())?[0])
    }
}
fn clone_value(stack: &mut NativeStack<'_, '_>, source: isize) -> Result<(), Error> {
    let value = stack.value(source).unwrap();
    if !matches!(value, Value::Table(_)) {
        return stack.push(value);
    }
    stack.create_table(0, 0)?;
    let destination = stack.top_offset();
    stack.push(Value::Nil)?;
    while stack.next_table(source)? {
        let value = stack.top_offset();
        clone_value(stack, value - 1)?;
        clone_value(stack, value)?;
        stack.set_table(destination, true)?;
        stack.set_top(stack.top() - 1)?;
    }
    Ok(())
}
