"""Measure isolated library-process RSS; requires CLI benchmark inputs first."""
import argparse
import json
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--node', default='node')
parser.add_argument('--bun', default='bun')
parser.add_argument('--runs', type=int, default=3)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
engines = [('Node baseline', args.node, 'baseline'), ('Node addon', args.node, 'native'), ('Node wasm', args.node, 'wasm'), ('Bun baseline', args.bun, 'baseline'), ('Bun wasm', args.bun, 'wasm')]
results = {}
digests = {}
for name, runtime, backend in engines:
    results[name] = {}
    for workload in ['loaded'] if backend == 'baseline' else ['loaded', 'repeated', 'large']:
        samples = []
        for _ in range(args.runs):
            command = [runtime, '--expose-gc', str(root / '.github/bench/memory.mjs'), backend, workload]
            flag = '-l' if platform.system() == 'Darwin' else '-v'
            process = subprocess.run(['/usr/bin/time', flag] + command, cwd=root, capture_output=True, text=True, check=True)
            sample = json.loads(process.stdout)
            pattern = r'(\d+)\s+maximum resident set size' if platform.system() == 'Darwin' else r'Maximum resident set size \(kbytes\):\s*(\d+)'
            sample['peakRss'] = int(re.search(pattern, process.stderr)[1]) * (1 if platform.system() == 'Darwin' else 1024)
            if backend != 'baseline':
                if workload in digests and digests[workload] != sample['digest']:
                    raise RuntimeError(f'{name}: different output in {workload}')
                digests[workload] = sample['digest']
            samples.append(sample)
        results[name][workload] = {'median': {key: statistics.median(sample[key] for sample in samples) for key in ['baselineRss', 'loadedRss', 'afterWorkRss', 'afterGcRss', 'peakRss']}, 'samples': samples}
        print(name, workload, json.dumps(results[name][workload]['median']), flush=True)
report = {'date': time.strftime('%Y-%m-%d'), 'machine': platform.platform(), 'runs': args.runs, 'results': results}
(root / 'target/bench/memory-results.json').write_text(json.dumps(report, indent=2) + '\n')
