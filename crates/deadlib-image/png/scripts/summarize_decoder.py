"""Compare decoder timings with source/corpus/binary and size invariants.

Rates use summed bytes / summed per-case median run-average nanoseconds.
Reference-relative changes mitigate shared-host drift; they do not remove it.
"""
from argparse import ArgumentParser
from pathlib import Path, PureWindowsPath
import csv,hashlib,json,statistics

def load(prefixes):
    metadata=[];runs=[];files=[]
    for prefix in prefixes:
        manifest=Path(str(prefix)+'-manifest.json')
        meta=json.loads(manifest.read_text(encoding='utf-8-sig'));metadata.append(meta)
        files.append({'path':str(manifest),'sha256':hashlib.sha256(manifest.read_bytes()).hexdigest()})
        assert len(meta['measurements'])==meta['runs']
        for measurement in meta['measurements']:
            output=Path(measurement['output'])
            if not output.is_file():output=manifest.parent/PureWindowsPath(measurement['output']).name
            data=list(csv.DictReader(output.open(encoding='utf-8-sig'),delimiter='\t'))
            rows={}
            for row in data:
                key=(row['case'],row['codec'],row['operation']);assert key not in rows
                assert int(row['ns_per_image'])>0 and int(row['iterations'])==meta['iterations']
                rows[key]=row
            assert rows;runs.append(rows)
            files.append({'path':str(output),'sha256':hashlib.sha256(output.read_bytes()).hexdigest()})
    first=metadata[0]
    def source(meta):return {(v['path'].replace('\\','/'),v['sha256'].lower())for v in meta['source_sha256']}
    def corpus(meta):return {(v['file'],v['bytes'],v['sha256'].lower())for v in meta['corpus_sha256']}
    for meta in metadata:
        assert source(meta)==source(first)
        assert corpus(meta)==corpus(first)
        for key in ['target','stage','iterations','cpu','affinity','benchmark_source_sha256','rustc','cargo','executable_sha256']:
            assert meta[key]==first[key],key
    for rows in runs:
        assert rows.keys()==runs[0].keys()
        for key,row in rows.items():
            for field in ['encoded_bytes','sample_bytes','image_pipeline_bytes']:
                assert row[field]==runs[0][key][field],(key,field)
    rates={}
    for codec,operation in sorted({key[1:]for key in runs[0]}):
        keys=[key for key in runs[0]if key[1:]==(codec,operation)]
        size=sum(int(runs[0][key]['sample_bytes'])for key in keys)
        ns=sum(statistics.median(int(run[key]['ns_per_image'])for run in runs)for key in keys)
        rates[codec+'/'+operation]=size*1e9/ns/1048576
    return {'manifests':first,'rows':runs[0],'rates_mib_s':rates,'process_runs':len(runs),'artifacts':files}

def main():
    parser=ArgumentParser(description=__doc__);parser.add_argument('--before',nargs='+',required=True,type=Path);parser.add_argument('--after',nargs='+',required=True,type=Path);parser.add_argument('--output',required=True,type=Path);args=parser.parse_args()
    assert not args.output.exists()
    before,after=load(args.before),load(args.after)
    assert before['rows'].keys()==after['rows'].keys()
    for key,row in before['rows'].items():
        for field in ['encoded_bytes','sample_bytes','image_pipeline_bytes']:assert row[field]==after['rows'][key][field],(key,field)
    for field in ['target','stage','iterations','cpu','affinity','benchmark_source_sha256','rustc','cargo','corpus_sha256']:assert before['manifests'][field]==after['manifests'][field],field
    changes={}
    for name,value in before['rates_mib_s'].items():
        if not name.startswith('deadlib-image-png/'):continue
        operation=name.split('/')[1]
        reference='png-0.18.1/decode'if operation.startswith('decode')else'flate2-1.1.10/inflate'
        changes[operation]={'before_mib_s':value,'after_mib_s':after['rates_mib_s'][name],
            'absolute_change_percent':(after['rates_mib_s'][name]/value-1)*100,
            'reference':reference,'reference_relative_change_percent':(after['rates_mib_s'][name]/after['rates_mib_s'][reference]/(value/before['rates_mib_s'][reference])-1)*100}
    result={'before':{k:v for k,v in before.items()if k not in ['rows','manifests']},'after':{k:v for k,v in after.items()if k not in ['rows','manifests']},'native_changes':changes,
        'case_count':len({key[0]for key in before['rows']}),'size_and_pipeline_capacity_invariants_verified':True,
        'scope':'Single-CPU shared-host measurements, including allocations. Reference normalization cannot eliminate scheduling/frequency/cache effects. No broad performance claim follows from one pair.',
        'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8',newline='\n');print(json.dumps(changes,indent=2))
if __name__=='__main__':main()
