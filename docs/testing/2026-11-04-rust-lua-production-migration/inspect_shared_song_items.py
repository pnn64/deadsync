import json
from pathlib import Path
d = Path(__file__).resolve().parent
files = json.loads((d / 'song-native-item-inventory-v1.json').read_text())
compile = next(file for file in files if file['file'] == 'compile.rs')
items = {item['name']: item for item in compile['items'] if item['name']}
todo = ['merge_runtime_messages', 'split_compiled_song_lua']
seen = set()
while todo:
    name = todo.pop()
    if name in seen:
        continue
    seen.add(name)
    item = items[name]
    dependencies = sorted({path.split('::')[0] for path in item['paths']} & items.keys())
    print(json.dumps({'name': name, 'kind': item['kind'], 'line': item['line'],
                      'dependencies': dependencies}))
    todo.extend(name for name in dependencies if name not in seen and name != 'DefaultCompiledSongLua')
