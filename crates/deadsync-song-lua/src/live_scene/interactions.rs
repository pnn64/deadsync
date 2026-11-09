//! Closed input snapshots share the live scene's verified actor inventory.
use super::*;

impl SongLiveScene {
    pub fn compile_actor_interactions(&self) -> Option<CompiledActorInteractions> {
        if self.ended || self.tracked.len() != self.tracked_states.len() {
            return None;
        }
        let mut guard = self.runtime.access();
        let runtime = &mut *guard;
        let functions = runtime.input.snapshot_functions(&mut runtime.vm)?;
        let Value::Table(batch) = runtime.vm.rooted(self.retained).ok()? else {
            return None;
        };
        let root = runtime.vm.raw_table_get(batch, Value::Number(1.)).ok()?;
        let tracked = self
            .tracked
            .iter()
            .zip(&self.tracked_states)
            .map(|((target, actor), state)| (*target, *actor, *state))
            .collect::<Vec<_>>();
        runtime.actors.compile_interaction_snapshot(
            &mut runtime.vm,
            &functions,
            root,
            &self.actors,
            &self.states,
            &tracked,
        )
    }
    pub fn compile_actor_inputs(&self) -> Option<CompiledActorInputs> {
        if self.ended || self.tracked.len() != self.tracked_states.len() {
            return None;
        }
        let mut guard = self.runtime.access();
        let runtime = &mut *guard;
        let functions = runtime.input.snapshot_functions(&mut runtime.vm)?;
        let tracked = self
            .tracked
            .iter()
            .zip(&self.tracked_states)
            .map(|((target, actor), state)| (*target, *actor, *state))
            .collect::<Vec<_>>();
        runtime.actors.compile_input_snapshot(
            &mut runtime.vm,
            &functions,
            &self.actors,
            &self.states,
            &tracked,
        )
    }
}
