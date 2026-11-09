"""Gate native binding items while preserving all existing executable bodies."""
import base64
import hashlib
import json
from collections import defaultdict
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
src = r / 'crates/deadsync-song-lua/src'
classification = json.loads((d / 'song-oracle-item-classification-v2.json').read_text())
out = d / 'song-oracle-isolation-edit-baseline.json'
assert not out.exists()
paths = [src / row['file'] for row in classification['files']]
manifest = src.parent / 'Cargo.toml'
paths.append(manifest)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert all(sha(src / row['file']) == row['sha256'] for row in classification['files'])
out.write_text(json.dumps({p.relative_to(r).as_posix(): dict(sha256=sha(p),
    base64=base64.b64encode(p.read_bytes()).decode()) for p in paths}, indent=2) + '\n')
forced = set(classification['forced_modules'])
grouped = defaultdict(list)
for node in classification['nodes']:
    grouped[node['file']].append(node)
guard = '#[cfg(any(test, feature = "legacy-oracle"))]'
def leaf_source(leaf, visibility):
    path = leaf['path'][:]
    alias = leaf['alias']
    if path and path[-1] == 'self':
        path.pop()
        if alias == 'self':
            alias = path[-1]
    if alias == '*':
        path.append('*')
    name = '::'.join(path)
    if alias != '*' and path[-1] != alias:
        name += ' as ' + alias
    return (visibility + ' ' if visibility else '') + 'use ' + name + ';'
changed = []
for file, nodes in grouped.items():
    path = src / file
    data = path.read_bytes()
    edits = []
    for node in nodes:
        if not node['native'] or node['unit_only']:
            continue
        # A guarded root module owns its complete original source subtree.
        if node['scope'] and node['scope'][0] in forced:
            continue
        leaves = node['import_leaves']
        if node['kind'] == 'use' and any(not leaf['native'] for leaf in leaves) and any(leaf['native'] for leaf in leaves):
            lines = []
            for leaf in leaves:
                lines.extend('#[' + attr + ']' for attr in node['attributes'])
                if leaf['native']:
                    lines.append(guard)
                lines.append(leaf_source(leaf, node['visibility']))
            edits.append((node['start'], node['end'], '\n'.join(lines).encode()))
        else:
            edits.append((node['start'], node['start'], (guard + '\n').encode()))
    for start, end, replacement in sorted(edits, reverse=True):
        data = data[:start] + replacement + data[end:]
    # Previous oracle bridges were tied to the removed experimental engine
    # feature. They now follow the explicit development-oracle configuration.
    data = data.replace(b'feature = "rust-engine"', b'any(test, feature = "legacy-oracle")')
    if data != path.read_bytes():
        path.write_bytes(data)
        changed.append(file)
text = manifest.read_text()
assert 'legacy-oracle =' not in text
text = text.replace('test-support = []', 'test-support = []\n# Development comparisons only; production always uses the Rust facade.\nlegacy-oracle = ["dep:mlua", "test-support"]', 1)
line = 'mlua = { workspace = true, features = ["lua51", "vendored", "send"] }'
assert text.count(line) == 1
text = text.replace(line, line[:-2] + ', optional = true }', 1)
text = text.replace('[dev-dependencies]', '[dev-dependencies]\n' + line, 1)
chunks = text.split('[[test]]')
for index in range(1, len(chunks)):
    import re
    match = re.search(r'name = "([^"]+)"', chunks[index])
    if match:
        test = src.parent / 'tests' / (match[1] + '.rs')
        body = test.read_text() if test.exists() else ''
        if 'test_support' in body or '::legacy::' in body or 'legacy::SongLuaLiveScene' in body:
            old = 'required-features = ["test-support"]'
            if old in chunks[index]:
                chunks[index] = chunks[index].replace(old, 'required-features = ["test-support", "legacy-oracle"]', 1)
text = '[[test]]'.join(chunks)
manifest.write_text(text)
# The mixed test helper module is not part of application instrumentation.
path = src / 'rust_engine.rs'
text = path.read_text()
needle = '#[cfg(feature = "test-support")]\npub mod test_support;'
assert text.count(needle) == 1
path.write_text(text.replace(needle, '#[cfg(all(feature = "test-support", any(test, feature = "legacy-oracle")))]\npub mod test_support;', 1))
(d / 'song-oracle-isolation-v1-edits.json').write_text(json.dumps(dict(
    files=changed, manifest=manifest.relative_to(r).as_posix(),
    policy='Production routing is always Rust. legacy-oracle only exposes native development comparisons; test-support alone does not enable mlua.'), indent=2) + '\n')
print(f'Isolated native items in {len(changed)} source files; made legacy Lua an optional oracle dependency.', flush=True)
