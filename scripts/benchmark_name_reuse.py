"""Isolate profile-ID delegation while keeping the baseline Unicode helper fixed."""
import csv
import importlib.util
import json
import os
from pathlib import Path
import re
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('name_driver', ROOT / 'scripts/benchmark_name_comparison.py')
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)
ARCHIVE = Path(os.environ.get('DEADSYNC_NAME_COMPARISON_ARCHIVE', ROOT / 'target/name-comparison')) / 'reuse-baseline'


def run():
    ARCHIVE.mkdir(parents=True, exist_ok=True)
    paths = ['crates/deadsync-profile/src/lib.rs', 'crates/deadsync-profile/tests/name_comparison.rs', 'crates/deadsync-profile/tests/support/name_comparison_original.rs', 'tests/support/kernel_bench.rs', 'tests/support/perf.rs', Path(__file__).relative_to(ROOT).as_posix()]
    hashes = {}
    for relative in paths:
        target = ARCHIVE / 'source' / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((ROOT / relative).read_bytes())
        hashes[relative] = driver.sha(target)
    library = (ROOT / paths[0]).read_text(encoding='utf-8-sig')
    harness = (ROOT / paths[1]).read_text(encoding='utf-8-sig')
    source = 'use std::cmp::Ordering;\nuse std::hint::black_box;\n'
    for name, relative in [('original', paths[2]), ('bench', paths[3]), ('perf', paths[4])]:
        source += '#[allow(dead_code)]\n#[path=' + json.dumps((ROOT / relative).as_posix()) + '] mod ' + name + ';\n'
    source += 'mod favorites_view { pub use super::original::unicode_case_insensitive_cmp; }\n#[inline(always)]\n'
    source += driver.item(library, 'pub fn cmp_profile_ids_case_insensitive') + '\n'
    for marker in ['fn next', 'fn pairs']:
        source += driver.item(harness, marker) + '\n'
    source += '''fn main() {
    for kind in ["short","early","near","shared","uuid","unicode","equal","random"] {
        let inputs=pairs(kind,256);
        let functions:[fn(&str,&str)->Ordering;2]=[original::cmp_profile_ids_case_insensitive,cmp_profile_ids_case_insensitive];
        for (a,b) in &inputs {assert_eq!(functions[0](a,b),functions[1](a,b));}
        let label=format!("reuse/{kind}");
        let mut work=|current:bool| {let f=black_box(functions[usize::from(current)]);for (a,b) in black_box(&inputs) {black_box(f(black_box(a),black_box(b)));}};
        for (current,name) in [(false,"original"),(true,"current")] {
            let (_,churn)=perf::measure(||work(current));assert_eq!(churn,perf::Churn::default());println!("{label}/{name}: allocation {churn:?}");
        }
        bench::compare(&label,500,&mut work);
    }
}
'''
    generated = ARCHIVE / 'main.rs'
    generated.write_text(source, encoding='utf-8')
    binary = ARCHIVE / 'bench.exe'
    command = ['rustc','--edition','2024','-C','opt-level=3','-C','lto=fat','-C','target-feature=+crt-static',str(generated),'-o',str(binary)]
    subprocess.run(command, check=True, env=dict(os.environ, TEMP=str(ARCHIVE.parent / 'compile-temp'), TMP=str(ARCHIVE.parent / 'compile-temp')))
    rows = []
    logs = {}
    for run_index in range(5):
        result = subprocess.run([str(binary)], cwd=ROOT, capture_output=True, text=True, encoding='utf-8')
        log = ARCHIVE / f'bench-{run_index}.log'
        log.write_text(result.stdout + result.stderr, encoding='utf-8')
        assert result.returncode == 0, result.stderr
        logs[log.name] = driver.sha(log)
        pairs = re.findall(r'([\w/-]+): cycles/op ([\d.]+) -> ([\d.]+); ns/op ([\d.]+) -> ([\d.]+)', result.stdout)
        assert len(pairs) == 8
        churn = {label: dict(re.findall(r'(\w+): (\d+)', fields)) for label, fields in re.findall(r'([\w/-]+): allocation Churn \{ ([^}]+) \}', result.stdout)}
        for label,a,b,c,d in pairs:
            for variant,cpu,ns in [('original',a,c),('current',b,d)]:
                counts = {k:int(v) for k,v in churn[label+'/'+variant].items()}
                assert len(counts)==6 and not any(counts.values())
                rows.append(dict(run=run_index,workload=label,variant=variant,cycles_per_fixture=cpu,ns_per_fixture=ns,**counts))
        print(f'reuse baseline helper: {run_index+1}/5',flush=True)
    target = ROOT / 'docs/performance/2026-11-04-name-comparison-reuse'
    with target.with_suffix('.tsv').open('w',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=list(rows[0]),delimiter='\t');writer.writeheader();writer.writerows(rows)
    metadata=dict(command=command,sources=hashes,generated_source=str(generated),generated_sha256=driver.sha(generated),binary=str(binary),binary_sha256=driver.sha(binary),logs=logs,rustc=subprocess.check_output(['rustc','-Vv'],text=True,encoding='utf-8'),qualification='Primitive ablation of the exact profile-ID function body against the independently frozen baseline Unicode helper; no block or ASCII-admission optimization is included in the delegated helper.')
    target.with_suffix('.manifest.json').write_text(json.dumps(metadata,indent=2),encoding='utf-8')
    for label in sorted({r['workload'] for r in rows}):
        old=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='original'];new=[float(r['cycles_per_fixture']) for r in rows if r['workload']==label and r['variant']=='current']
        print(label,f'{statistics.median(a/b for a,b in zip(old,new)):.3f}x',flush=True)

if __name__=='__main__':run()
