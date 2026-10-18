import base64
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import tarfile

from native import PLATFORMS, binary_name, package_path, stage_binaries, collect_licenses
import tomllib
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
ARTIFACTS = ROOT / "target/release-artifacts"


def emit(values):
    for name, value in values.items():
        print(f"{name}={value}")
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for name, value in values.items():
                output.write(f"{name}={value}\n")


def metadata():
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    npm = json.loads((ROOT / "package.json").read_text())
    lock = json.loads((ROOT / "package-lock.json").read_text())
    version = cargo["workspace"]["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit("Releases require an X.Y.Z version")
    if any(value != version for value in [npm["version"], lock["version"], lock["packages"][""]["version"]]):
        raise SystemExit("Cargo, npm and npm lockfile versions differ")
    tag = os.environ.get("RELEASE_TAG", "")
    if tag and tag != f"v{version}":
        raise SystemExit(f"Tag {tag!r} must match v{version}")
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", toolchain):
        raise SystemExit("Rust toolchain must be pinned to an exact release")
    msrv = tomllib.loads((ROOT / "crates/yq-core/Cargo.toml").read_text())["package"]["rust-version"]
    if msrv.count(".") == 1:
        msrv += ".0"
    return {"version": version, "rust": toolchain, "msrv": msrv, **{key: value for key, value in json.loads((ROOT / ".github/release/versions.json").read_text()).items() if isinstance(value, str)}}


def sizes():
    data = (ROOT / "js/yq.wasm").read_bytes()
    values = {"wasm_bytes": len(data), "gzip_bytes": len(gzip.compress(data, compresslevel=9, mtime=0))}
    if values["wasm_bytes"] > 2_000_000 or values["gzip_bytes"] > 700_000:
        raise SystemExit(f"Wasm size budget exceeded: {values}")
    return values


def bundle():
    version = metadata()["version"]
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    stage_binaries()
    collect_licenses(ROOT / 'dist')
    packed = json.loads(subprocess.check_output(["npm", "pack", "--ignore-scripts", "--json", "--pack-destination", str(ARTIFACTS)], cwd=ROOT))[0]
    verify_package(ARTIFACTS / packed['filename'])
    shutil.copy2(ROOT / "js/yq.wasm", ARTIFACTS / "yq.wasm")
    crate = f"yq-core-{version}.crate"
    shutil.copy2(ROOT / "target/package" / crate, ARTIFACTS / crate)
    fixture = ROOT / "crates/yq-core/tests/fixtures"
    reference = re.search(r"`([0-9a-f]{40})`", (fixture / "README.md").read_text()).group(1)
    report = (ROOT / "target/conformance-report.txt").read_text().splitlines()[0]
    match = re.fullmatch(r"conformance: (\d+)/(\d+) passed, (\d+) skipped", report)
    if not match or int(match[1]) < int((ROOT / "crates/yq-core/tests/conformance_baseline.txt").read_text()):
        raise SystemExit("Missing full conformance report or baseline regression")
    if int(match[2]) + int(match[3]) != len(json.loads((fixture / "scenarios.json").read_text())):
        raise SystemExit("Conformance report does not cover the full fixture")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    native_binaries = native_records()
    for binary in native_binaries:
        shutil.copy2(ROOT / 'target/native-artifacts' / binary['binary'], ARTIFACTS / binary['binary'])
        with tarfile.open(ARTIFACTS / packed['filename'], 'r:gz') as archive:
            packaged = archive.extractfile('package/' + binary['package_path']).read()
        if packaged != (ARTIFACTS / binary['binary']).read_bytes():
            raise SystemExit(f"Packed binary differs from native build: {binary['platform']}")
    manifest = {"native_binaries": native_binaries, "version": version, "commit": commit, "npm": packed["filename"], "crate": crate, "yq_reference": reference, "conformance": report, **sizes()}
    (ARTIFACTS / "release.json").write_text(json.dumps(manifest, indent=2) + "\n")
    npm_name = json.loads((ROOT / "package.json").read_text())["name"]
    (ARTIFACTS / "release-notes.md").write_text(f"Release v{version}\n\nGo yq fixture reference: `{reference}`.\n\n{report}.\n\nWasm: {manifest['wasm_bytes']:,} bytes; {manifest['gzip_bytes']:,} bytes gzipped.\n\nPackages: `yq-core` and `{npm_name}`, version {version}.\n")
    names = ["yq.wasm", packed["filename"], crate, "release.json", "release-notes.md"]
    names += [binary['binary'] for binary in native_binaries]
    (ARTIFACTS / "SHA256SUMS").write_text("".join(f"{hashlib.sha256((ARTIFACTS / name).read_bytes()).hexdigest()}  {name}\n" for name in names))


def registry_json(url):
    request = urllib.request.Request(url, headers={"User-Agent": "yq.rs-release"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def native_records():
    return [{'platform': platform['id'], 'binary': binary_name(platform), 'package_path': package_path(platform)} for platform in PLATFORMS]


def inspect_package(path):
    with tarfile.open(path, 'r:gz') as archive:
        package = json.load(archive.extractfile('package/package.json'))
        files = [{'path': member.name.removeprefix('package/'), 'mode': member.mode, 'size': member.size} for member in archive.getmembers() if member.isfile()]
    return {'package': package, 'files': files}


def verify_package(path):
    packed = inspect_package(path)
    if packed['package'].get('optionalDependencies') or './native' in packed['package'].get('exports', {}):
        raise SystemExit('Package must bundle CLI binaries without platform dependencies or an addon entry')
    files = {item['path']: item for item in packed['files']}
    for binary in native_records():
        member = files.get(binary['package_path'])
        if not member or not member['size']:
            raise SystemExit(f"Missing packed native binary: {binary['package_path']}")
        if member['mode'] & 0o777 != 0o755:
            raise SystemExit(f"Packed native binary must have mode 0755: {binary['package_path']}")
    return packed


def release_manifest():
    manifest = json.loads((ARTIFACTS / "release.json").read_text())
    version = metadata()["version"]
    npm_name = json.loads((ROOT / "package.json").read_text())["name"]
    expected_npm = f"{npm_name.lstrip('@').replace('/', '-')}-{version}.tgz"
    if manifest["crate"] != f"yq-core-{version}.crate" or manifest["npm"] != expected_npm:
        raise SystemExit("Release manifest contains unexpected package filenames")
    expected_names = {"yq.wasm", expected_npm, manifest["crate"], "release.json", "release-notes.md"}
    if manifest.get('native_binaries') != native_records():
        raise SystemExit('Release manifest contains unexpected native binaries')
    expected_names.update(binary['binary'] for binary in manifest['native_binaries'])
    checked = set()
    for line in (ARTIFACTS / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ")
        if name not in expected_names or name in checked or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise SystemExit(f"Invalid release checksum entry: {name}")
        if hashlib.sha256((ARTIFACTS / name).read_bytes()).hexdigest() != digest:
            raise SystemExit(f"Release artifact checksum mismatch: {name}")
        checked.add(name)
    if checked != expected_names:
        raise SystemExit("Release checksum manifest is incomplete")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if manifest["version"] != version or manifest["commit"] != commit:
        raise SystemExit("Release artifact version/commit does not match checkout")
    return manifest


def npm_publication(name, filename, version):
    npm = registry_json(f"https://registry.npmjs.org/{urllib.parse.quote(name, safe='')}/{version}")
    expected = "sha512-" + base64.b64encode(hashlib.sha512((ARTIFACTS / filename).read_bytes()).digest()).decode()
    if npm and npm['dist']['integrity'] != expected:
        raise SystemExit(f'npm already has different bytes for {name}@{version}')
    return npm


def registries():
    manifest = release_manifest()
    version = metadata()['version']
    crate = registry_json(f"https://crates.io/api/v1/crates/yq-core/{version}")
    npm_name = json.loads((ROOT / 'package.json').read_text())['name']
    npm = npm_publication(npm_name, manifest['npm'], version)
    if crate and crate["version"]["checksum"] != hashlib.sha256((ARTIFACTS / manifest["crate"]).read_bytes()).hexdigest():
        raise SystemExit("crates.io already has different bytes for this version")
    emit({"cargo_exists": str(bool(crate)).lower(), "npm_exists": str(bool(npm)).lower(), "npm_file": manifest["npm"]})


def wait_npm(packages, version):
    deadline = time.monotonic() + int(os.environ.get('YQ_PUBLISH_TIMEOUT_SECONDS', '3600'))
    pending = list(packages)
    while pending:
        pending = [package for package in pending if not npm_publication(package['name'], package['filename'], version)]
        if not pending:
            return
        if time.monotonic() >= deadline:
            raise SystemExit('npm has not exposed these verified packages yet: ' + ', '.join(package['name'] for package in pending))
        print('Waiting for npm publication: ' + ', '.join(package['name'] for package in pending), flush=True)
        time.sleep(30)


if __name__ == "__main__":
    command = sys.argv[1]
    if command == "metadata":
        emit(metadata())
    elif command == "size":
        emit(sizes())
    elif command == "bundle":
        bundle()
    elif command == "registries":
        registries()
    elif command == "inspect-package":
        print(json.dumps(inspect_package(sys.argv[2])))
    elif command == "check-package":
        verify_package(sys.argv[2])
    elif command == "wait-npm":
        manifest = release_manifest()
        wait_npm([{'name': json.loads((ROOT / 'package.json').read_text())['name'], 'filename': manifest['npm']}], metadata()['version'])
    else:
        raise SystemExit(f"Unknown command: {command}")
