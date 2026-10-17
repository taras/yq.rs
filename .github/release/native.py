import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = json.loads((ROOT / 'npm/platforms.json').read_text())


def package_metadata(platform, addon=False):
    return json.loads((ROOT / 'npm' / (platform['id'] + ('-napi' if addon else '')) / 'package.json').read_text())


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
        if not texts and package.get('repository') == 'https://github.com/napi-rs/napi-rs':
            commit = json.loads((source / '.cargo_vcs_info.json').read_text())['git']['sha1']
            fallback = ROOT / 'npm/licenses/napi-rs' / f'{commit}.LICENSE'
            if fallback.is_file():
                texts = [fallback]
        if not texts:
            raise SystemExit(f"Missing dependency license text: {package['name']} {package['version']}")
        folder = f"{package['name']}-{package['version']}"
        for text in set(texts):
            relative = text.relative_to(source) if text.is_relative_to(source) else Path("LICENSE")
            target = destination / 'licenses' / folder / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(text, target)
        lines.append(f"- {package['name']} {package['version']}: {package['license']}; {package.get('repository') or ''}; licenses/{folder}/")
    (destination / 'licenses').mkdir(exist_ok=True)
    (destination / 'licenses/README.md').write_text('\n'.join(lines) + '\n')


def build_package(platform_id, binary, addon=False):
    platform = next(item for item in PLATFORMS if item['id'] == platform_id)
    meta = package_metadata(platform, addon)
    destination = ROOT / 'target/native-packages' / meta['name']
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    (destination / 'package.json').write_text(json.dumps(meta, indent=2) + '\n')
    name = 'yq.node' if addon else ('yq.exe' if platform['os'] == 'win32' else 'yq')
    shutil.copyfile(binary, destination / name)
    (destination / name).chmod(0o755)
    for name in ['LICENSE', 'THIRD_PARTY_NOTICES.md']:
        shutil.copyfile(ROOT / name, destination / name)
    shutil.copytree(ROOT / 'crates/yq-core/licenses', destination / 'crates/yq-core/licenses')
    collect_licenses(destination)
    artifacts = ROOT / 'target/native-artifacts'
    artifacts.mkdir(parents=True, exist_ok=True)
    packed = json.loads(subprocess.check_output(([os.environ.get('COMSPEC', 'cmd.exe'), '/c', 'npm'] if os.name == 'nt' else ['npm']) + ['pack', '--ignore-scripts', '--json', '--cache', str(ROOT / 'target/npm-cache'), '--pack-destination', str(artifacts)], cwd=destination))[0]
    raw = f"yq-{platform_id}{'.node' if addon else ('.exe' if platform['os'] == 'win32' else '')}"
    shutil.copyfile(binary, artifacts / raw)
    (artifacts / raw).chmod(0o755)
    print(artifacts / packed['filename'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['matrix', 'package'])
    parser.add_argument('--addon', action='store_true')
    parser.add_argument('platform', nargs='?')
    parser.add_argument('binary', nargs='?')
    args = parser.parse_args()
    if args.command == 'matrix':
        print(json.dumps({'include': PLATFORMS}, separators=(',', ':')))
    else:
        build_package(args.platform, Path(args.binary).resolve(), args.addon)
