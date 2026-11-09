"""Add the existing combined owner and extract the unchanged plan cache builder."""
import base64,hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2];a=r/'crates/deadsync-song-lua/src/rust_engine/actors'
frozen=json.loads((d/'actor-input-snapshots-focus-v2-song-linux-debug/sources.json').read_text())
for name,digest in frozen.items():assert hashlib.sha256((r/name).read_bytes()).hexdigest()==digest,name
archive=d/'actor-input-snapshots-before-combined-sources.json';assert not archive.exists();archive.write_text(json.dumps(frozen,indent=2)+'\n')
p=a/'lifecycle.rs';raw=p.read_bytes();text=raw.decode().replace('\r\n','\n')
saved=d/'actor-input-snapshots-before-combined-lifecycle.rs.txt';assert not saved.exists();saved.write_bytes(raw)
def once(value,before,after):
    assert value.count(before)==1,before;return value.replace(before,after,1)
text=once(text,'use super::*;\n','use super::*;\nmod snapshot;\npub(super) use snapshot::{callback_jobs, cached_for_root, plan_roots};\n')
text=once(text,'    transition: bool,\n','    transition: bool,\n    recurring: bool,\n    texture: bool,\n')
needle='''        let mut nodes = Vec::new();
        for child in children {'''
replacement='''        let recurring = match sprites::field(call, actor, "__songlua_recurring_update_command")? {
            Value::Nil => false,
            value => {
                let value = bytes(call, value)?;
                std::str::from_utf8(&value).map_err(|e| call.error(e.to_string().as_bytes()))?;
                true
            }
        };
        let transition = transitions::state(call, actor)?.is_some();
        let mut nodes = Vec::new();
        for child in children {'''
text=once(text,needle,replacement)
text=once(text,'        let mut columns = Vec::new();\n','''        let texture = store.lock().unwrap().actors.get(&actor)
            .is_some_and(|actor| !actor.model_materials.is_empty());
        let mut columns = Vec::new();
''')
text=once(text,'            transition: transitions::state(call, actor)?.is_some(),\n','            transition,\n            recurring,\n            texture,\n')
start=text.index('    let cached = store.lock().unwrap().update_plans.get(&actor).cloned();',text.index('pub(super) fn advance('))
end=text.index('    // An authored callback may invalidate',start)
body=text[start:end]
text=text[:start]+'    let plan = retained_plan(call, store, actor)?;\n'+text[end:]
text+='''
pub(super) fn retained_plan(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<Arc<Plan>, Error> {
'''+body+'''    Ok(plan)
}
'''
p.write_text(text)
snapshot=(d/'draft-actor-input-snapshot-plan.rs.txt').read_text()
snapshot=snapshot.replace('pub(super) fn callback_jobs(', 'pub(in crate::rust_engine::actors) fn callback_jobs(')
snapshot+='''
pub(in crate::rust_engine::actors) fn plan_roots(plan: &Plan) -> TableId { plan.roots }
pub(in crate::rust_engine::actors) fn cached_for_root(store: &Shared, root: TableId) -> Option<Arc<Plan>> {
    fn contains(node: &FrameNode, actor: TableId) -> bool {
        node.actor == actor || node.wrappers.iter().chain(&node.children).any(|child| contains(child, actor))
    }
    let guard = store.lock().unwrap();
    guard.update_plans.get(&root).cloned().or_else(||
        guard.update_plans.values().find(|plan| contains(&plan.node, root)).cloned())
}
'''
target=a/'lifecycle/snapshot.rs';assert not target.exists();target.parent.mkdir(exist_ok=True);target.write_text(snapshot)
owner=(d/'draft-actor-input-snapshot-update-owner.rs.txt').read_text()
owner=owner.replace('pub(in crate::rust_engine::actors) enum SourceJob', '#[derive(Clone, Copy)]\npub(in crate::rust_engine::actors) enum SourceJob')
target=a/'input_snapshot/update.rs';assert not target.exists()
target.write_text(owner+'\n'+(d/'draft-actor-input-snapshot-update-compiler.rs.txt').read_text())
print('Combined owner and plan cache extraction installed; APIs and tests still require wiring.')
