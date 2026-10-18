import argparse
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = json.loads((ROOT / '.github/release/platforms.json').read_text())


def binary_name(platform):
    return f"yq-{platform['id']}{'.exe' if platform['os'] == 'win32' else ''}"


def package_path(platform):
    executable = 'yq.exe' if platform['os'] == 'win32' else 'yq'
    return f"dist/native/{platform['id']}/{executable}"


def collect_licenses(destination):
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT))
    lines = ['# Dependency licenses', '', 'Includes build/test dependencies as well as linked dependencies.', '']
    for package in metadata['packages']:
        if package['source'] is None:
            continue
        source = Path(package['manifest_path']).parent
        texts = [path for path in source.rglob('*') if path.is_file() and path.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE'))]
        if package.get('license_file'):
            texts.append(source / package['license_file'])
        if not texts:
            raise SystemExit(f"Missing dependency license text: {package['name']} {package['version']}")
        folder = f"{package['name']}-{package['version']}"
        for text in set(texts):
            target = destination / 'licenses' / folder / text.relative_to(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(text, target)
        lines.append(f"- {package['name']} {package['version']}: {package['license']}; {package.get('repository') or ''}; licenses/{folder}/")
    (destination / 'licenses').mkdir(exist_ok=True)
    (destination / 'licenses/README.md').write_text('\n'.join(lines) + '\n')


def stage_binaries():
    for platform in PLATFORMS:
        source = ROOT / 'target/native-artifacts' / binary_name(platform)
        if not source.is_file():
            raise SystemExit(f"Missing native build: {source.name}")
        destination = ROOT / package_path(platform)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
        destination.chmod(0o755)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['matrix', 'stage'])
    args = parser.parse_args()
    if args.command == 'matrix':
        print(json.dumps({'include': PLATFORMS}, separators=(',', ':')))
    else:
        stage_binaries()
        collect_licenses(ROOT / 'dist')
