import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tarfile

root = Path(__file__).resolve().parents[2]
versions = json.loads((root / ".github/release/versions.json").read_text())
host = f"{platform.system()}-{platform.machine()}"
if host not in versions["binaryen_assets"]:
    raise SystemExit(f"No pinned Binaryen asset for {host}")
asset = versions["binaryen_assets"][host]
destination = root / "target/binaryen"
destination.mkdir(parents=True, exist_ok=True)
archive = destination / asset["name"]
if not archive.exists():
    url = f"https://github.com/WebAssembly/binaryen/releases/download/version_{versions['binaryen']}/{asset['name']}"
    subprocess.run(["curl", "--fail", "--location", "--retry", "3", "--output", str(archive), url], check=True)
if hashlib.sha256(archive.read_bytes()).hexdigest() != asset["sha256"]:
    raise SystemExit(f"Binaryen checksum mismatch: {archive}")
with tarfile.open(archive) as package:
    package.extractall(destination, filter="data")
print(destination / f"binaryen-version_{versions['binaryen']}/bin")
