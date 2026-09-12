// Frozen from b63f6a0c9d58926bb863e2aae5cbc7e94690d476; only test visibility is adapted.
use super::*;

pub(super) fn push_unique_actor_child(out: &mut Vec<Table>, seen: &mut Vec<usize>, child: Table) {
    let ptr = child.to_pointer() as usize;
    if seen.contains(&ptr) {
        return;
    }
    seen.push(ptr);
    out.push(child);
}

pub(super) fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        push_unique_actor_child(&mut out, &mut seen, child);
    }
    for pair in actor_children(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        let Value::Table(child) = value else {
            continue;
        };
        if actor_is_child_group(&child)? {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    push_unique_actor_child(&mut out, &mut seen, group_child);
                }
            }
        } else {
            push_unique_actor_child(&mut out, &mut seen, child);
        }
    }
    Ok(out)
}
