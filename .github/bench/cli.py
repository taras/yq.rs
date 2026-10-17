"""Process-level CLI benchmark. Run with --help for required paths."""
import argparse
import json
import hashlib
import os
from pathlib import Path
import platform
import random
import re
import statistics
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--go', required=True)
parser.add_argument('--native', default='target/release/yq')
parser.add_argument('--node', default='node')
parser.add_argument('--bun', default='bun')
parser.add_argument('--entry', default='dist/cli.mjs')
parser.add_argument('--runs', type=int, default=11)
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
destination = root / 'target/bench'
destination.mkdir(parents=True, exist_ok=True)
commands = {
    'Go': [str(Path(args.go).resolve())],
    'Rust native': [str(Path(args.native).resolve())],
    'npm native (Node)': [args.node, str(Path(args.entry).resolve())],
    'wasm (Node)': [args.node, str((root / 'dist/cli.mjs').resolve())],
    'wasm (Bun)': [args.bun, str((root / 'dist/cli.mjs').resolve())],
}
inputs = {}
for size in [1024, 10_000_000]:
    header = 'name: Ada\nitems:\n'
    row = '  - value: ' + 'x' * 980 + '\n'
    data = header + row * ((size - len(header)) // len(row))
    data += '#' + 'p' * (size - len(data) - 2) + '\n'
    path = destination / f'input-{size}.yml'
    path.write_text(data)
    inputs[str(size)] = path

def run(name, query, measure=False):
    env = {**os.environ, 'YQ_FORCE_WASM': '1' if name.startswith('wasm') else '0'}
    command = commands[name] + query
    if measure:
        flag = '-l' if platform.system() == 'Darwin' else '-v'
        command = ['/usr/bin/time', flag] + command
    start = time.perf_counter_ns()
    result = subprocess.run(command, env=env, capture_output=True, check=True)
    elapsed = (time.perf_counter_ns() - start) / 1_000_000
    if measure:
        stderr = result.stderr.decode()
        pattern = r'(\d+)\s+maximum resident set size' if platform.system() == 'Darwin' else r'Maximum resident set size \(kbytes\):\s*(\d+)'
        match = re.search(pattern, stderr)
        if not match:
            raise RuntimeError(stderr)
        rss = int(match[1]) * (1 if platform.system() == 'Darwin' else 1024)
        return rss
    return elapsed, result.stdout

results = {}
randomizer = random.Random(42)
for workload, query in [('startup', ['--version'])] + [(size, ['.name', str(path)]) for size, path in inputs.items()]:
    samples = {name: [] for name in commands}
    memory = {name: [] for name in commands}
    for name in commands:
        for _ in range(2):
            _, output = run(name, query)
            if workload != 'startup' and output != b'Ada\n':
                raise RuntimeError(f'{name}: incorrect benchmark output: {output!r}')
    for _ in range(args.runs):
        order = list(commands)
        randomizer.shuffle(order)
        for name in order:
            elapsed, _ = run(name, query)
            samples[name].append(elapsed)
    for name in commands:
        memory[name] = [run(name, query, True) for _ in range(3)]
    results[workload] = {name: {'median_ms': statistics.median(samples[name]), 'peak_rss_bytes': statistics.median(memory[name]), 'samples_ms': samples[name]} for name in commands}
    print(workload, json.dumps(results[workload]), flush=True)
report = {
    'date': time.strftime('%Y-%m-%d'), 'machine': platform.platform(),
    'cpu': subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip() if platform.system() == 'Darwin' else platform.processor(),
    'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    'dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=root)),
    'node': subprocess.check_output([args.node, '--version'], text=True).strip(),
    'bun': subprocess.check_output([args.bun, '--version'], text=True).strip(),
    'go': subprocess.check_output([args.go, '--version'], text=True).strip(),
    'runs': args.runs, 'results': results,
    'sources': {path: hashlib.sha256((root / path).read_bytes()).hexdigest() for path in ['crates/yq-cli/src/main.rs', 'bin/yq.js', 'bin/yq-wasm.js']},
}
(destination / 'cli-results.json').write_text(json.dumps(report, indent=2) + '\n')
