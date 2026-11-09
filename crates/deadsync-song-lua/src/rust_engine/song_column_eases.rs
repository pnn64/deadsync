//! Existing compiler note-column endpoint sampling and ordered restoration.
use super::*;

impl RustSongRuntime {
    pub fn read_song_note_column_transform_samples(
        &mut self,
    ) -> Result<Vec<crate::SongLuaColumnTransformSample>, String> {
        self.actors
            .read_column_samples(&mut self.vm, None)
            .map_err(|e| e.to_string())
    }
    pub fn read_song_note_column_transform_samples_for_fields(
        &mut self,
        fields: &[Value],
    ) -> Result<Vec<crate::SongLuaColumnTransformSample>, String> {
        self.actors
            .read_column_samples(&mut self.vm, Some(fields))
            .map_err(|e| e.to_string())
    }

    /// Only columns and clocks are restored here. Script cells, RNG and other
    /// actor writes retain the native endpoint behavior; the enclosing compiler
    /// pass owns any broader snapshot. Seconds entries still use these beat
    /// clocks, matching the existing production helper.
    pub fn capture_song_note_column_function_eases(
        &mut self,
        function: Value,
        params: crate::SongLuaColumnOffsetBuildParams,
        endpoints: [f32; 2],
    ) -> Result<Vec<crate::SongLuaColumnOffsetWindow>, String> {
        let pin = self.vm.pin(function).map_err(|e| e.to_string())?;
        let result = (|| {
            let end = crate::song_lua_span_end(params.start, params.limit, params.span_mode)
                .max(params.start);
            let from = self.capture_column_endpoint(function, endpoints[0], params.start)?;
            let to = self.capture_column_endpoint(function, endpoints[1], end)?;
            Ok(crate::column_transform_windows_from_samples(
                &from, &to, params,
            ))
        })();
        self.vm.unpin(pin).map_err(|e| e.to_string())?;
        result
    }
    fn capture_column_endpoint(
        &mut self,
        function: Value,
        value: f32,
        beat: f32,
    ) -> Result<Vec<crate::SongLuaColumnTransformSample>, String> {
        let previous = self.values().map_err(|e| e.to_string())?;
        self.set_beat(beat).map_err(|e| e.to_string())?;
        // A snapshot failure leaves the selected clock, as native. Body and
        // sampling failures still attempt ordinary column restoration first.
        let session = self
            .actors
            .begin_column_capture(&mut self.vm)
            .map_err(|e| e.to_string())?;
        let mut returned = None;
        let body = self
            .vm
            .call(function, &[number(value)])
            .map(|values| values.first().copied().unwrap_or(Value::Nil))
            .and_then(|value| self.vm.pin(value).map(|pin| returned = Some(pin)));
        let result = (|| {
            let samples = self.read_song_note_column_transform_samples();
            // Fresh VM entries allow cleanup after a fatal instruction abort.
            self.actors
                .restore_column_capture(&mut self.vm, &session)
                .map_err(|e| e.to_string())?;
            self.set_values(previous.0, previous.1)
                .map_err(|e| e.to_string())?;
            body.map_err(|e| e.to_string())?;
            samples
        })();
        session.release(&mut self.vm).map_err(|e| e.to_string())?;
        if let Some(pin) = returned {
            self.vm.unpin(pin).map_err(|e| e.to_string())?;
        }
        result
    }
}
