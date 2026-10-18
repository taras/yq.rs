"""Set the Cargo and npm versions before preparing a release."""
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
version = sys.argv[1]
if not re.fullmatch(r'\d+\.\d+\.\d+', version):
    raise SystemExit('Expected X.Y.Z')
cargo = ROOT / 'Cargo.toml'
cargo.write_text(re.sub(r'(?m)^version = "[^"]+"$', f'version = "{version}"', cargo.read_text(), count=1))
package_path = ROOT / 'package.json'
package = json.loads(package_path.read_text())
package['version'] = version
package_path.write_text(json.dumps(package, indent=2) + '\n')
lock_path = ROOT / 'package-lock.json'
lock = json.loads(lock_path.read_text())
lock['version'] = version
lock['packages']['']['version'] = version
lock_path.write_text(json.dumps(lock, indent=2) + '\n')
subprocess.run(['cargo', 'update', '--workspace', '--offline'], cwd=ROOT, check=True)
subprocess.run([sys.executable, str(ROOT / '.github/release/verify.py'), 'metadata'], cwd=ROOT, check=True)
