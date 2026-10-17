import base64
import hashlib
import json
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
            'native_packages': verify.native_records(version),
        }
        self.names = ['yq.wasm', self.manifest['npm'], self.manifest['crate'], 'release-notes.md', 'release.json']
        self.names += [name for package in self.manifest['native_packages'] for name in [package['filename'], package['binary']]]
        for name in self.names:
            (self.artifacts / name).write_bytes(name.encode())
        (self.artifacts / 'release.json').write_text(json.dumps(self.manifest))
        self.checksums()

    def checksums(self):
        (self.artifacts / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256((self.artifacts / name).read_bytes()).hexdigest()}  {name}\n' for name in self.names))

    def test_tampered_or_missing_platform_artifact_cannot_publish(self):
        self.assertEqual(verify.release_manifest(), self.manifest)
        binary = self.artifacts / self.manifest['native_packages'][0]['binary']
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
        self.manifest['native_packages'][0]['filename'] = '../unverified.tgz'
        (self.artifacts / 'release.json').write_text(json.dumps(self.manifest))
        self.checksums()
        with self.assertRaisesRegex(SystemExit, 'unexpected native packages'):
            verify.release_manifest()

    def test_registry_retries_require_identical_bytes(self):
        package = self.manifest['native_packages'][0]
        integrity = 'sha512-' + base64.b64encode(hashlib.sha512((self.artifacts / package['filename']).read_bytes()).digest()).decode()
        with patch.object(verify, 'registry_json', return_value={'dist': {'integrity': integrity}}):
            self.assertTrue(verify.npm_publication(package['name'], package['filename'], self.manifest['version']))
        with patch.object(verify, 'registry_json', return_value={'dist': {'integrity': 'sha512-different'}}):
            with self.assertRaisesRegex(SystemExit, 'different bytes'):
                verify.npm_publication(package['name'], package['filename'], self.manifest['version'])
