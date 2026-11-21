from pathlib import Path
from collections import Counter, defaultdict
import csv
import hashlib
import json
import shutil
import struct
import subprocess

ROOT=Path.cwd()
OUT=ROOT/'target/png-perf-20261119'
OUT.mkdir(parents=True,exist_ok=True)
DEST=OUT/'assets'
DEST.mkdir(exist_ok=True)

listed=subprocess.run(['git','ls-files','-z','--','assets','courses','crates/deadsync-song-lua/assets'],check=True,capture_output=True).stdout
paths=sorted(name.decode() for name in listed.split(b'\0') if name and Path(name.decode()).suffix.lower()=='.png')
entries=[]
invalid=[]
channels={0:1,2:3,3:1,4:2,6:4}
for name in paths:
    data=(ROOT/name).read_bytes()
    if data[:8]!=b'\x89PNG\r\n\x1a\n':
        invalid.append(name)
        continue
    width,height,depth,color,compression,filter_method,interlace=struct.unpack('>IIBBBBB',data[16:29])
    at=8
    chunks=Counter()
    metadata=0
    idat=0
    while at+12<=len(data):
        length=int.from_bytes(data[at:at+4],'big')
        kind=data[at+4:at+8].decode('ascii')
        assert at+length+12<=len(data),(name,kind,length)
        chunks[kind]+=1
        if kind=='IDAT': idat+=length
        elif kind not in ('IHDR','PLTE','IEND'): metadata+=length
        at+=length+12
        if kind=='IEND': break
    group='/'.join(name.split('/')[:2]) if name.startswith('assets/') else ('courses' if name.startswith('courses/') else 'song-lua')
    raw=((width*channels[color]*depth+7)//8+1)*height
    entries.append({'source':name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'width':width,'height':height,'pixels':width*height,'color_type':color,'bit_depth':depth,'interlaced':bool(interlace),'chunks':dict(chunks),'idat_bytes':idat,'metadata_bytes':metadata,'encoded_over_row_bytes':idat/raw,'group':group})

by_source={entry['source']:entry for entry in entries}
selected=[]
seen=set()
def add(entry,reason):
    if entry['sha256'] in seen or len(selected)>=32: return
    seen.add(entry['sha256'])
    selected.append({**entry,'selection_reason':reason})

historical=['assets/fonts/cjk/_jfonts 16px [kanji-jis1] 32x32 (doubleres).png','assets/graphics/content_browser/nobanner.png','assets/graphics/icon/icon-16.png','assets/graphics/icon/icon-32.png','assets/graphics/logo.png']
for source in historical:
    add(by_source[source],'retain historical benchmark control')
formats=defaultdict(list)
groups=defaultdict(list)
for entry in entries:
    formats[(entry['color_type'],entry['bit_depth'],entry['interlaced'])].append(entry)
    groups[entry['group']].append(entry)
for key,items in sorted(formats.items()):
    ordered=sorted(items,key=lambda e:(e['pixels'],e['source']))
    add(ordered[len(ordered)//2],f'median pixel count for color/depth/interlace {key}')
for group,items in sorted(groups.items()):
    ordered=sorted(items,key=lambda e:(e['pixels'],e['source']))
    add(ordered[-1],f'largest pixel count in {group}')
    add(ordered[len(ordered)//2],f'median pixel count in {group}')
for field in ['metadata_bytes','idat_bytes','width','height']:
    add(max(entries,key=lambda e:(e[field],e['source'])),f'largest {field}')
for kind in ['tRNS','iCCP','iTXt','tEXt','acTL']:
    items=[entry for entry in entries if kind in entry['chunks']]
    if items: add(max(items,key=lambda e:(e['pixels'],e['source'])),f'largest pixels carrying {kind}')
add(min(entries,key=lambda e:(e['pixels'],e['source'])),'smallest pixel count')
ordered=sorted(entries,key=lambda e:(e['encoded_over_row_bytes'],e['pixels'],e['source']))
for quantile in [0,1,2,3,4,5,6,7,8,9,10]:
    add(ordered[(len(ordered)-1)*quantile//10],f'compressed-size/row-size decile {quantile}')
for entry in sorted(entries,key=lambda e:(e['sha256'],e['source'])):
    add(entry,'stable content-hash fill to 32 distinct PNG streams')

for i,entry in enumerate(selected):
    target=DEST/f'asset-{i:02d}.png'
    shutil.copyfile(ROOT/entry['source'],target)
    entry['benchmark_path']=target.relative_to(ROOT).as_posix()
    assert hashlib.sha256(target.read_bytes()).hexdigest()==entry['sha256']

available={}
for name in ['png-decode-corpus-20261117','png-kodak-20261117']:
    directory=ROOT/'target'/name
    files=sorted(directory.glob('*.png')) if directory.is_dir() else []
    available[name]={'path':directory.relative_to(ROOT).as_posix(),'exists':directory.is_dir(),'png_files':len(files),'bytes':sum(path.stat().st_size for path in files)}

summary={'scope':['assets','courses','crates/deadsync-song-lua/assets'],'png_paths':len(paths),'valid_signatures':len(entries),'non_png_signatures':invalid,'unique_png_bytes':len({entry['sha256'] for entry in entries}),'groups':dict(Counter(entry['group'] for entry in entries)),'formats':{str(key):len(value) for key,value in sorted(formats.items())},'total_file_bytes':sum(entry['bytes'] for entry in entries),'total_pixels':sum(entry['pixels'] for entry in entries),'selected_files':len(selected),'selected_pixels':sum(entry['pixels'] for entry in selected),'available_corpora':available}
rules=['Select PNG signatures from tracked files under the three runtime asset roots; no decoding or timing is performed.','Retain the five historical controls, then choose each format median, each workflow group largest and median, metadata/IDAT/dimension extremes and relevant chunk carriers.','Add compression-ratio deciles, then stable content-hash order if needed. Stop at 32 distinct SHA256 values; ties are resolved by original path.','Copies are byte-identical; source paths, dimensions, format, chunk counts and SHA256 hashes are recorded per selected file.']
(OUT/'inventory.json').write_text(json.dumps({'summary':summary,'files':entries},indent=2)+'\n')
(OUT/'assets-manifest.json').write_text(json.dumps({'summary':summary,'selection_rules':rules,'files':selected},indent=2)+'\n')
(OUT/'assets-original-paths.txt').write_text('\n'.join(entry['source'] for entry in selected)+'\n')
print('SUMMARY',json.dumps(summary,sort_keys=True))
for entry in selected:
    print('SELECTED',entry['benchmark_path'],entry['source'],f"{entry['width']}x{entry['height']}",f"color{entry['color_type']}/depth{entry['bit_depth']}/adam7{int(entry['interlaced'])}",entry['selection_reason'])
