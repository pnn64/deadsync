from pathlib import Path
import json,re,math
root=Path('target/deadlib-audio-reference');text=(root/'degenerate-books-validation-20261027/release.log').read_text()
text=re.sub(r'test [a-zA-Z_][a-zA-Z_0-9:]* \.\.\. (?:ok|ignored[^\n]*)\n','',text)
counts={label:(int(cases),int(packets),int(discarded)) for label,cases,packets,discarded in re.findall(r'(?m)^([^:\r\n]+): (\d+) cases, (\d+) packets, (\d+) discarded\r?$',text)}
pcm={label:(int(samples),float(peak),float(rms),float(signal)) for label,samples,peak,rms,signal in re.findall(r'(?m)^([^:\r\n]+): (\d+) samples, peak error ([\d.eE+-]+), RMS ([\d.eE+-]+), signal peak ([\d.eE+-]+)',text)}
nonfinite={label:int(n) for label,n in re.findall(r'(?m)^([^:\r\n]+): (\d+) nonfinite samples with matching IEEE classes',text)}
summary={}
for prefix in ['empty-','scalar-','nonfinite-reference','floor-zero-later-floor-eof','book-golden-']:
 labels=sorted(k for k in counts if k.startswith(prefix));assert labels and all(k in pcm for k in labels)
 setups={k.split('-candidate-')[0] for k in labels}
 group={'setups':len(setups),'batches':len(labels),'cases':sum(counts[k][0] for k in labels),'packets':sum(counts[k][1] for k in labels),'discarded':sum(counts[k][2] for k in labels),'finite_samples':sum(pcm[k][0] for k in labels),'nonfinite_samples':sum(nonfinite.get(k,0) for k in labels),'maximum_peak_error':max(pcm[k][1] for k in labels),'maximum_batch_rms':max(pcm[k][2] for k in labels),'aggregate_rms':math.sqrt(sum(pcm[k][2]**2*pcm[k][0] for k in labels)/sum(pcm[k][0] for k in labels))}
 if prefix=='empty-':assert group['setups']==276
 if prefix=='scalar-':assert group['setups']==36
 summary[prefix.rstrip('-')]=group
assert summary['nonfinite-reference']['cases']==11 and summary['nonfinite-reference']['nonfinite_samples']==16256
(root/'degenerate-books-20261027/matrix-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print(json.dumps(summary,indent=2))
