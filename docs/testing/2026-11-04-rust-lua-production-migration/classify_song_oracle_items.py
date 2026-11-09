"""Read-only dependency classification for the existing native binding items."""
import hashlib
import json
from collections import defaultdict
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
src = r / 'crates/deadsync-song-lua/src'
out = d / 'song-oracle-item-classification-v2.json'
assert not out.exists()
files = json.loads((d / 'song-native-item-inventory-v4.json').read_text())
forced = {'input', 'actor_input', 'compile', 'raw_vm', 'actor_classes', 'actor_methods', 'legacy'}
protected = {'rust_engine', 'playback', 'facade', 'compiled_layers', 'input_types'}
def module(file):
    parts = list(Path(file).with_suffix('').parts)
    if parts[-1] in {'mod', 'lib'}:
        parts.pop()
    return tuple(parts)
nodes = []
scopes = defaultdict(lambda: defaultdict(list))
aliases = defaultdict(lambda: defaultdict(list))
wildcards = defaultdict(list)
macros = defaultdict(list)
modules = {module(file['file']) for file in files}
for file in files:
    data = (src / file['file']).read_bytes()
    assert len(data) == file['bytes'], file['file']
    scope = module(file['file'])
    for item in file['items']:
        node = dict(item, file=file['file'], scope=scope, id=len(nodes))
        node['unit_only'] = 'cfg (test)' in item['attributes']
        nodes.append(node)
        if item['kind'] not in {'use', 'impl'} and item['name'] and not node['unit_only']:
            scopes[scope][item['name']].append(node['id'])
        if item['kind'] == 'macro':
            macros[scope].append(node['id'])
        for leaf in item['imports']:
            if leaf['alias'] == '*':
                wildcards[scope].append(leaf['path'])
            elif not node['unit_only']:
                name = leaf['alias']
                if name == 'self' and len(leaf['path']) > 1:
                    name = leaf['path'][-2]
                aliases[scope][name].append(leaf['path'])

def resolve(scope, path, seen=frozenset(), generated=False):
    path = tuple(path)
    key = (scope, path)
    if not path or key in seen:
        return set(), False
    seen = seen | {key}
    if path[0] == 'mlua':
        return set(), True
    if path[0] == 'crate':
        return resolve((), path[1:], seen, generated)
    if path[0] == 'self':
        return resolve(scope, path[1:], seen, generated)
    if path[0] == 'super':
        return resolve(scope[:-1], path[1:], seen, generated)
    if scope and scope[0] in protected:
        return set(), False
    if scope and scope[0] in forced:
        return set(), True
    # Local module declarations and use aliases are resolved before root-level
    # module fallback. External crates other than mlua cannot be native seeds.
    found = set(scopes[scope].get(path[0], []))
    native = False
    if scope + (path[0],) in modules:
        target = scope + (path[0],)
        if target and target[0] in forced:
            return found, True
        if len(path) > 1:
            return resolve(target, path[1:], seen, generated)
    result = set()
    for target in aliases[scope].get(path[0], []):
        target = tuple(target)
        if target and target[-1] == 'self':
            target = target[:-1]
        more, flag = resolve(scope, target + path[1:], seen, generated)
        result |= more
        native |= flag
    if result or native:
        return result, native
    if found:
        return found, False
    for target in wildcards[scope]:
        more, flag = resolve(scope, tuple(target) + path, seen, generated)
        result |= more
        native |= flag
    if result or native:
        return result, native
    if scope and (path[0],) in modules:
        return resolve((), path, seen, generated)
    if generated and len(path) == 1 and '_' in path[0] and path[0][0].islower():
        result |= {index for index in macros[scope] if path[0] in nodes[index]['macro_identifiers']}
    return result, native

for node in nodes:
    scope = node['scope']
    deps = set()
    native = node['unit_only'] or bool(scope and scope[0] in forced)
    if node['kind'] == 'mod' and not scope and node['name'] in forced:
        native = True
    leaf_rows = []
    for leaf in node['imports']:
        ids, flag = resolve(scope, leaf['path'], generated=True)
        deps |= ids
        native |= flag
        leaf_rows.append(dict(leaf, dependencies=sorted(ids), seed=flag))
    for path in node['paths']:
        parts = path.split('::')
        if parts[0] in node.get('locals', []):
            continue
        ids, flag = resolve(scope, parts)
        deps |= ids
        native |= flag
    if node['kind'] == 'macro':
        for name in node['macro_identifiers']:
            if name not in node.get('locals', []):
                ids, flag = resolve(scope, [name])
                deps |= ids
                native |= flag
    deps.discard(node['id'])
    if scope and scope[0] in protected:
        deps.clear()
        native = False
    node.update(dependencies=sorted(deps), seed=native, native=native, import_leaves=leaf_rows)
changed = True
while changed:
    changed = False
    for node in nodes:
        if not node['native'] and any(nodes[index]['native'] for index in node['dependencies']):
            node['native'] = True
            changed = True
for node in nodes:
    for leaf in node['import_leaves']:
        leaf['native'] = leaf['seed'] or any(nodes[index]['native'] for index in leaf['dependencies'])
summary = []
for file in files:
    selected = [node for node in nodes if node['file'] == file['file']]
    summary.append(dict(file=file['file'], items=len(selected), native=sum(node['native'] for node in selected),
                        sha256=hashlib.sha256((src / file['file']).read_bytes()).hexdigest()))
out.write_text(json.dumps(dict(files=summary, forced_modules=sorted(forced), nodes=nodes), indent=2) + '\n')
print(json.dumps({'files': len(files), 'items': len(nodes), 'native_candidates': sum(node['native'] for node in nodes)}))
for row in summary:
    print(f"{row['native']:4}/{row['items']:4} {row['file']}")
