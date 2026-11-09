from pathlib import Path
import difflib,json,sys
p=Path(__file__).parent/sys.argv[1]/'run.log';lines=p.read_text().splitlines();name=''
for i,line in enumerate(lines):
 if line.startswith("thread '"):name=line.split("'")[1]
 if line.startswith(('assertion','called `Result')):print(name,line[:1000])
 if line.startswith('  left:') and i+1<len(lines) and lines[i+1].startswith(' right:'):
  a,b=line[7:],lines[i+1][7:]
  try:a,b=json.loads(a),json.loads(b)
  except ValueError:pass
  changes=[(i,j,k,l) for op,i,j,k,l in difflib.SequenceMatcher(None,a,b,autojunk=False).get_opcodes() if op!='equal']
  for i,j,k,l in changes[:8]:print('rust',a[max(0,i-100):min(len(a),j+100)],'native',b[max(0,k-100):min(len(b),l+100)])
