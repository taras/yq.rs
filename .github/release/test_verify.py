import base64
import hashlib
import json
import io
import tarfile
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import verify


class ReleaseIntegrity(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.artifacts = Path(self.directory.name)
        self.patch = patch.object(verify, 'ARTIFACTS', self.artifacts)
        self.patch.start()
        self.addCleanup(self.patch.stop)
        version = verify.metadata()['version']
        self.manifest = {
            'version': version,
            'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=verify.ROOT, text=True).strip(),
            'npm': f'yq.rs-{version}.tgz',
            'crate': f'yq-core-{version}.crate',
            'native_binaries': verify.native_records(),
        }
        self.names = ['yq.wasm', self.manifest['npm'], self.manifest['crate'], 'release-notes.md', 'release.json']
        self.names += [binary['binary'] for binary in self.manifest['native_binaries']]
        for name in self.names:
            (self.artifacts / name).write_bytes(name.encode())
        (self.artifacts / 'release.json').write_text(json.dumps(self.manifest))
        self.checksums()

    def checksums(self):
        (self.artifacts / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256((self.artifacts / name).read_bytes()).hexdigest()}  {name}\n' for name in self.names))

    def test_tampered_or_missing_platform_artifact_cannot_publish(self):
        self.assertEqual(verify.release_manifest(), self.manifest)
        binary = self.artifacts / self.manifest['native_binaries'][0]['binary']
        original = binary.read_bytes()
        binary.write_bytes(b'tampered')
        with self.assertRaisesRegex(SystemExit, 'checksum mismatch'):
            verify.release_manifest()
        binary.write_bytes(original)
        self.names.remove(binary.name)
        self.checksums()
        with self.assertRaisesRegex(SystemExit, 'incomplete'):
            verify.release_manifest()

    def test_manifest_cannot_substitute_an_unverified_package(self):
        self.manifest['native_binaries'][0]['binary'] = '../unverified.tgz'
        (self.artifacts / 'release.json').write_text(json.dumps(self.manifest))
        self.checksums()
        with self.assertRaisesRegex(SystemExit, 'unexpected native binaries'):
            verify.release_manifest()

    def test_registry_retries_require_identical_bytes(self):
        package = {'name': 'yq.rs', 'filename': self.manifest['npm']}
        integrity = 'sha512-' + base64.b64encode(hashlib.sha512((self.artifacts / package['filename']).read_bytes()).digest()).decode()
        with patch.object(verify, 'registry_json', return_value={'dist': {'integrity': integrity}}):
            self.assertTrue(verify.npm_publication(package['name'], package['filename'], self.manifest['version']))
        with patch.object(verify, 'registry_json', return_value={'dist': {'integrity': 'sha512-different'}}):
            with self.assertRaisesRegex(SystemExit, 'different bytes'):
                verify.npm_publication(package['name'], package['filename'], self.manifest['version'])


class PackedBinaries(unittest.TestCase):
    def archive(self, destination, omitted=None, mode=0o755):
        with tarfile.open(destination, 'w:gz') as archive:
            metadata = json.dumps({'name': 'yq.rs', 'exports': {'.': './dist/inline.js'}}).encode()
            info = tarfile.TarInfo('package/package.json')
            info.size = len(metadata)
            archive.addfile(info, io.BytesIO(metadata))
            for binary in verify.native_records():
                if binary['platform'] == omitted:
                    continue
                info = tarfile.TarInfo('package/' + binary['package_path'])
                info.mode = mode
                info.size = 3
                archive.addfile(info, io.BytesIO(b'bin'))

    def test_every_platform_including_windows_is_required(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'package.tgz'
            self.archive(path)
            verify.verify_package(path)
            for platform in ['win32-x64', 'win32-arm64']:
                with self.subTest(platform=platform):
                    self.archive(path, omitted=platform)
                    with self.assertRaisesRegex(SystemExit, 'Missing packed native binary.*yq.exe'):
                        verify.verify_package(path)

    def test_lost_executable_mode_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'package.tgz'
            self.archive(path, mode=0o644)
            with self.assertRaisesRegex(SystemExit, 'mode 0755'):
                verify.verify_package(path)
