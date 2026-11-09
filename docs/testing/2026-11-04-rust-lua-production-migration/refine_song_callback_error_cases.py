from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync');d=r/'docs/testing/2026-11-04-rust-lua-production-migration';p=r/'crates/deadsync-song-lua/tests/rust_song_diagnostics.rs';raw=p.read_bytes();(d/'song-callback-errors-tests-before-v2.rs').write_bytes(raw);s=raw.decode();nl='\r\n' if '\r\n' in s else '\n';s=s.replace('\r\n','\n')
s=s.replace('fn compile(source: &str) -> (Output, Output) {','fn compile_results(source: &str) -> (Result<Vec<Output>, String>, Result<Vec<Output>, String>) {')
# Keep both results even when native rejects preparation, to check the Rust
# error/fallback boundary too rather than stopping at the first unwrap.
a=s.index('    let native = song::compile_song_lua_layers_with_default_host(');b=s.index('fn header(',a)
old=s[a:b]
new=old.replace('    )\n    .unwrap();','    );')
assert new!=old
start=new.index('    (\n        native.into_iter().next().unwrap(),')
new=new[:start]+'''    (native,rust)
}
fn compile(source: &str) -> (Output, Output) {
    let (native,rust)=compile_results(source);
    (native.unwrap().into_iter().next().unwrap(),rust.unwrap().into_iter().next().unwrap())
}
'''
s=s[:a]+new+s[b:]
old="""        return Def.ActorFrame{Name='cycle',OnCommand=function(self)self:queuecommand('Pulse')end,
            PulseCommand=function(self)"""
new="""        local started=false
        return Def.ActorFrame{Name='cycle',OnCommand=function(self)
                self:SetUpdateFunction(function(self,dt)
                    if dt>0 and not started then started=true;self:queuecommand('Pulse')end
                end)
            end,
            PulseCommand=function(self)"""
assert s.count(old)==1;s=s.replace(old,new)
p.write_bytes(s.replace('\n',nl).encode())
p=r/'crates/deadsync-song-lua/src/rust_engine/actors/compiler.rs';raw=p.read_bytes();nl='\r\n' if b'\r\n' in raw else '\n';s=raw.decode().replace('\r\n','\n')
a=s.index('        Query::Draw => {');b=s.index('        Query::End => {',a)
s=s[:a]+'''        Query::Draw => {
            if let Value::Table(root) = value
                && let Err(error) = draw::scene(call, &read.store, root)
            {
                diagnostics::report(call, &read.store, root, diagnostics::Callback::Draw, error)?;
            }
        }
'''+s[b:];p.write_bytes(s.replace('\n',nl).encode());print('Preserved native-rejected queue fixture, moved recurrent probe to runtime, fixed new lint')