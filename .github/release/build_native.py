import shutil
import subprocess
import sys

from native import ROOT, PLATFORMS, binary_name

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
artifacts = ROOT / 'target/native-artifacts'
artifacts.mkdir(parents=True, exist_ok=True)
destination = artifacts / binary_name(platform)
shutil.copyfile(binary, destination)
destination.chmod(0o755)
print(destination)
