from pathlib import Path
import re,json,sys
p=Path(__file__).parent/sys.argv[1]/'run.log';lines=p.read_text().splitlines();name=''
for i,line in enumerate(lines):
 if line.startswith("thread '"):name=line.split("'")[1]
 if line.startswith(('assertion','called `Result')):print(name,line[:600])
 if line.startswith('  left:') and i+1<len(lines) and lines[i+1].startswith(' right:'):
  a,b=line,lines[i+1]
  if 'SongLuaOverlayState {' in a:
   rows=lambda s:[dict(x.split(': ',1) for x in re.split(r', (?=\w+: )',v)) for v in re.findall(r'SongLuaOverlayState \{ (.*?) \}',s)]
   a,b=rows(a),rows(b);print('state_counts',len(a),len(b))
   for j,(x,y)in enumerate(zip(a,b)):
    delta={k:[x.get(k),y.get(k)] for k in x.keys()|y.keys() if x.get(k)!=y.get(k)}
    if delta:print(j,json.dumps(delta))
  else:print(a[:1800],b[:1800])
