"""Mechanical scene/runtime ownership changes; raw originals are archived."""
from pathlib import Path
r=Path(__file__).resolve().parents[3]
p=r/'crates/deadsync-song-lua/src/rust_engine/live_scene.rs'
s=p.read_text()
s=s.replace('    RustSongRuntime,\n    SongLuaCompileContext,', '    SongLiveRuntime,\n    SongLuaCompileContext,', 1)
s=s.replace('    runtime: RustSongRuntime,\n    screen:', '    runtime: SongLiveRuntime,\n    screen:', 1)
a=s.index('    pub fn new(');b=s.index('        if !context.live',a)
s=s[:a]+'''    pub fn new(runtime: RustSongRuntime, context: &SongLuaCompileContext, b: SongLiveSceneBindings) -> Result<Self, Error> {
        Self::new_shared(SongLiveRuntime::new(runtime), context, b)
    }
    pub(super) fn new_shared(owner: SongLiveRuntime, context: &SongLuaCompileContext, b: SongLiveSceneBindings) -> Result<Self, Error> {
        let mut guard = owner.access();
        let mut runtime = &mut *guard;
'''+s[b:]
s=s.replace('        let mut scene = Self {\n            runtime,', '        drop(guard);\n        let mut scene = Self {\n            runtime: owner,',1)
s=s.replace('        scene.read_states()?;\n        let mut startup = scene.take_output()?;', '''        let owner = scene.runtime.clone();
        let mut guard = owner.access();
        scene.read_states(&mut guard)?;
        let mut startup = scene.take_output(&mut guard)?;''',1)
s=s.replace('pub fn runtime(&self) -> &RustSongRuntime {\n        &self.runtime\n    }','pub fn runtime(&self) -> std::sync::MutexGuard<\'_ , RustSongRuntime> {\n        self.runtime.access()\n    }',1)
s=s.replace('pub fn runtime_mut(&mut self) -> &mut RustSongRuntime {\n        &mut self.runtime\n    }','pub fn runtime_mut(&mut self) -> std::sync::MutexGuard<\'_ , RustSongRuntime> {\n        self.runtime.access()\n    }',1)
s=s.replace('        self.runtime.selected_gameplay_context(&self.context)','        self.runtime.access().selected_gameplay_context(&self.context)',1)
# Each entry locks the VM once. Helpers use that borrow and never reacquire it.
names=['prepare_gameplay_frame','update','input','draw','end','into_runtime']
for name in names:
    a=s.index('    pub fn '+name+'(')
    b=s.index('\n    pub',a+1) if '\n    pub' in s[a+1:] else s.index('\n    fn ',a+1)
    chunk=s[a:b];body=chunk.index(' {')+2
    chunk=chunk[:body]+'''\n        let owner = self.runtime.clone();
        let mut guard = owner.access();
        let runtime = &mut *guard;'''+chunk[body:]
    chunk=chunk.replace('&mut self.runtime','runtime').replace('self.runtime','runtime')
    chunk=chunk.replace('let owner = runtime.clone();','let owner = self.runtime.clone();',1)
    for helper in ['root','read_states','off','take_output']:
        chunk=chunk.replace('self.'+helper+'()', 'self.'+helper+'(runtime)')
    chunk=chunk.replace('self.has_custom_draw()?', 'Self::custom_draw(runtime)?')
    if name=='into_runtime':
        chunk=chunk.replace('Result<RustSongRuntime, Error>','Result<SongLiveRuntime, Error>')
        chunk=chunk.replace('        Ok(runtime)', '        drop(guard);\n        Ok(owner)')
    s=s[:a]+chunk+s[b:]
s=s.replace('    pub fn has_custom_draw(&mut self) -> Result<bool, Error> {\n        self.runtime.vm.with_native_stack', '''    pub fn has_custom_draw(&mut self) -> Result<bool, Error> {
        Self::custom_draw(&mut self.runtime.access())
    }
    fn custom_draw(runtime: &mut RustSongRuntime) -> Result<bool, Error> {
        runtime.vm.with_native_stack''',1)
a=s.index('    fn root(');b=s.index('\n}\nfn error',a)
chunk=s[a:b].replace('fn root(&self)', 'fn root(&self, runtime: &mut RustSongRuntime)')
for name in ['read_states','off','take_output']:
    chunk=chunk.replace('fn '+name+'(&mut self)', 'fn '+name+'(&mut self, runtime: &mut RustSongRuntime)')
chunk=chunk.replace('&mut self.runtime','runtime').replace('self.runtime','runtime').replace('self\n            .runtime','runtime')
s=s[:a]+chunk+s[b:]
p.write_text(s)
p=r/'crates/deadsync-song-lua/src/rust_engine/compiler/live.rs'
s=p.read_text().replace('retained: Option<RustSongRuntime>', 'retained: Option<SongLiveRuntime>')
s=s.replace('    let mut runtime = match retained {', '    let owner = match retained {',1)
s=s.replace('None => RustSongRuntime::new_with_noteskin_resolver(context, noteskins)\n            .map_err(|e| e.to_string())?,','None => SongLiveRuntime::new(RustSongRuntime::new_with_noteskin_resolver(context, noteskins)\n            .map_err(|e| e.to_string())?),',1)
s=s.replace('    begin_load(&mut runtime)?;', '    let mut guard = owner.access();\n    let mut runtime = &mut *guard;\n    begin_load(&mut runtime)?;',1)
s=s.replace('    out.live_scene = Some(\n        SongLiveScene::new(\n            runtime,','    drop(guard);\n    out.live_scene = Some(\n        SongLiveScene::new_shared(\n            owner,',1)
p.write_text(s)
# Explicitly bind guard lifetimes in existing tests; preserve their assertions.
for name in ['rust_live_scene.rs','rust_live_compiler.rs']:
    p=r/'crates/deadsync-song-lua/tests'/name;s=p.read_text()
    import re
    s=re.sub(r'(\s*)let vm = (.+?)\.runtime_mut\(\)\.vm_mut\(\);',r'\1let mut runtime = \2.runtime_mut();\1let vm = runtime.vm_mut();',s)
    s=s.replace('    let mut runtime = scene.into_runtime().unwrap();','    let owner = scene.into_runtime().unwrap();\n    let mut runtime = owner.access();')
    p.write_text(s)
print('Shared VM ownership applied; existing callback logic untouched.')
