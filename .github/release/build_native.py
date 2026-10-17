import os
import subprocess
import sys

from native import ROOT, PLATFORMS, build_package

platform = next(item for item in PLATFORMS if item['id'] == sys.argv[1])
target = platform['target']
build_target = target + '.2.28' if target.endswith('-gnu') else target
cargo = 'zigbuild' if platform['os'] == 'linux' else 'build'
subprocess.run(['cargo', cargo, '--locked', '--release', '-p', 'yq-cli', '--target', build_target], cwd=ROOT, check=True)
binary = ROOT / 'target' / target / 'release' / ('yq.exe' if platform['os'] == 'win32' else 'yq')
subprocess.run([str(binary), '--version'], check=True)
result = subprocess.check_output([str(binary), '.name'], input=b'name: Ada\n')
if result != b'Ada\n':
    raise SystemExit('Native binary smoke test failed')
build_package(platform['id'], binary)

env = dict(os.environ)
if target.endswith('-musl'):
    env['RUSTFLAGS'] = '-C target-feature=-crt-static'
npm = [os.environ.get('COMSPEC', 'cmd.exe'), '/c', 'npm'] if os.name == 'nt' else ['npm']
subprocess.run(npm + ['ci', '--omit=optional', '--ignore-scripts'], cwd=ROOT, check=True)
command = npm + ['exec', '--', 'napi', 'build', '--platform', '--release', '--manifest-path', 'crates/yq-node/Cargo.toml', '--output-dir', 'target/napi', '--target', target, '--no-js', '--', '--locked']
if platform['os'] == 'linux':
    command.insert(command.index('--platform'), '--cross-compile')
subprocess.run(command, cwd=ROOT, env=env, check=True)
addon = ROOT / 'target/napi' / f"yq.{platform['id']}.node"
if not target.endswith('-musl'):
    subprocess.run(['node', '-e', 'const a = require(process.argv[1]); if (a.evaluate(".name", "name: Ada\\n").output !== "Ada\\n") { throw Error("addon smoke failed"); }', str(addon)], check=True)
build_package(platform['id'], addon, addon=True)
