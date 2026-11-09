//! Screen destination lookup at the existing gameplay transition boundary.
use super::*;

impl SongLiveScene {
    /// Read the current screen's stored override. An empty override retains the
    /// theme destination. Inspection must not invoke authored table getters.
    pub fn next_screen_name(&self) -> Result<Option<String>, Error> {
        let mut runtime = self.runtime.access();
        runtime.vm.with_native_stack(|stack| {
            let top = stack.top();
            let result = (|| {
                stack.push_bytes(b"__songlua_top_screen")?;
                stack.get_table(-10002, true)?;
                if !matches!(stack.value(-1), Some(Value::Table(_))) {
                    return Err(stack.error(b"table expected"));
                }
                let screen = stack.top_offset();
                stack.push_bytes(b"__songlua_next_screen_name")?;
                stack.get_table(screen, true)?;
                if stack.value(-1) == Some(Value::Nil) {
                    return Ok(None);
                }
                let bytes = match stack.to_bytes(-1)? {
                    Some(bytes) => bytes.to_vec(),
                    None => return Err(stack.error(b"string expected")),
                };
                let name = String::from_utf8(bytes)
                    .map_err(|error| stack.error(error.to_string().as_bytes()))?;
                Ok((!name.is_empty()).then_some(name))
            })();
            stack.set_top(top)?;
            result
        })
    }
}
