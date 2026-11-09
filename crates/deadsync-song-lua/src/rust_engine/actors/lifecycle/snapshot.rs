// Reuse the cached FrameNode
// hierarchy; do not reconstruct a differently ordered actor tree in the factory.
// FrameNode retains native recurring and texture job flags. Capture
// recurring after wrapper planning, and texture after child planning, matching
// collect_compile_update_jobs. transition and columns already exist in Plan.
use super::*;
use crate::rust_engine::actors::input_snapshot::update::SourceJob;

pub(in crate::rust_engine::actors) fn callback_jobs(
    plan: &Plan,
    root: TableId,
    mut advance_ready: impl FnMut(TableId) -> bool,
) -> Option<Vec<SourceJob>> {
    fn find(node: &FrameNode, root: TableId) -> Option<&FrameNode> {
        if node.actor == root {
            return Some(node);
        }
        node.wrappers
            .iter()
            .chain(&node.children)
            .find_map(|node| find(node, root))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Preserve the cached traversal's parent, wrapper and callback slot identities."
    )]
    fn visit(
        node: &FrameNode,
        parent: Option<usize>,
        wrappers: bool,
        root: bool,
        seen: &mut HashSet<TableId>,
        slots: &mut usize,
        output: &mut Vec<SourceJob>,
        advance_ready: &mut impl FnMut(TableId) -> bool,
    ) -> Option<()> {
        if !seen.insert(node.actor) {
            return None;
        }
        let slot = *slots;
        *slots += 1;
        output.push(SourceJob::Enter {
            actor: node.actor,
            parent,
            wrappers,
        });
        for wrapper in &node.wrappers {
            visit(
                wrapper,
                Some(slot),
                true,
                false,
                seen,
                slots,
                output,
                advance_ready,
            )?;
        }
        if node.recurring || node.transition || !advance_ready(node.actor) {
            return None;
        }
        for child in &node.children {
            visit(
                child,
                Some(slot),
                false,
                false,
                seen,
                slots,
                output,
                advance_ready,
            )?;
        }
        if node.texture {
            return None;
        }
        output.push(SourceJob::Callback {
            actor: node.actor,
            slot,
        });
        // Native slicing ends at the requested root's Callback. Its own note
        // columns come after that boundary. A descendant's columns occur inside
        // the slice and are not supported by the existing callback unit.
        if !root && !node.columns.is_empty() {
            return None;
        }
        Some(())
    }
    let node = find(&plan.node, root)?;
    let mut output = Vec::new();
    visit(
        node,
        None,
        false,
        true,
        &mut HashSet::new(),
        &mut 0,
        &mut output,
        &mut advance_ready,
    )?;
    Some(output)
}

pub(in crate::rust_engine::actors) fn plan_roots(plan: &Plan) -> TableId {
    plan.roots
}
pub(in crate::rust_engine::actors) fn cached_for_root(
    store: &Shared,
    root: TableId,
) -> Option<Arc<Plan>> {
    fn contains(node: &FrameNode, actor: TableId) -> bool {
        node.actor == actor
            || node
                .wrappers
                .iter()
                .chain(&node.children)
                .any(|child| contains(child, actor))
    }
    let guard = store.lock().unwrap();
    guard.update_plans.get(&root).cloned().or_else(|| {
        guard
            .update_plans
            .values()
            .find(|plan| contains(&plan.node, root))
            .cloned()
    })
}
