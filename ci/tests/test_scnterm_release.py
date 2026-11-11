import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
SPEC = importlib.util.spec_from_file_location('publish_release', Path(__file__).resolve().parents[1] / 'publish-scnterm-release.py')
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
COMMIT = 'a' * 40


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'windows').mkdir()
        for name in publisher.EXPECTED_FILES['windows']:
            (self.root / 'windows' / name).write_bytes(b'package data')
        self.current = COMMIT
        self.release = None
        self.mutations = []
        self.upload_count = 0
        self.fail_upload = False
        self.corrupt_upload = False
        self.tag_commit = 'b' * 40
        self.metadata = None
        self.checksums = ''
        self.env = patch.dict(os.environ, {
            'GITHUB_REPOSITORY': 'scnabla/scnterm',
            'GITHUB_SHA': COMMIT,
            'GITHUB_REF': 'refs/heads/personal',
            'GITHUB_RUN_ID': '123',
            'EXPECTED_PLATFORMS': 'windows',
            'RELEASE_PLATFORMS': 'windows',
        })
        self.env.start()
        self.addCleanup(self.env.stop)
        self.api_patch = patch.object(publisher, 'gh', side_effect=self.api)
        self.api_patch.start()
        self.addCleanup(self.api_patch.stop)
        self.upload_patch = patch.object(publisher.subprocess, 'run', side_effect=self.upload)
        self.upload_patch.start()
        self.addCleanup(self.upload_patch.stop)

    def api(self, *args, payload=None, **kwargs):
        endpoint = args[-1]
        method = args[args.index('--method') + 1] if '--method' in args else 'GET'
        if method != 'GET':
            self.mutations.append((method, endpoint, payload))
        if endpoint.endswith('/git/ref/heads/personal'):
            return {'object': {'sha': self.current}}
        if endpoint.endswith('/releases/tags/latest'):
            return self.release
        if endpoint.endswith('/releases') and method == 'POST':
            self.release = {'id': 42, 'assets': [], **payload}
            return self.release
        if endpoint.endswith('/releases/42'):
            if method == 'PATCH':
                self.release.update(payload)
            return self.release
        if '/releases/assets/' in endpoint and method == 'DELETE':
            asset_id = int(endpoint.rsplit('/', 1)[-1])
            self.release['assets'] = [a for a in self.release['assets'] if a['id'] != asset_id]
            return None
        if '/releases/assets/' in endpoint and method == 'GET':
            self.assertIn('Accept: application/octet-stream', args)
            return self.metadata
        if endpoint.endswith('/git/ref/tags/latest'):
            return {'object': {'sha': self.tag_commit}}
        if endpoint.endswith('/git/refs/tags/latest') and method == 'PATCH':
            self.tag_commit = payload['sha']
            return {'object': {'sha': self.tag_commit}}
        raise AssertionError(args)

    def upload(self, args, **kwargs):
        self.assertEqual(args[:4], ['gh', 'release', 'upload', 'latest'])
        self.upload_count += 1
        if self.fail_upload:
            raise subprocess.CalledProcessError(1, args)
        current = {a['name']: a for a in self.release['assets']}
        for path in map(Path, args[7:]):
            content = path.read_bytes()
            current[path.name] = {
                'id': len(current) + 100,
                'name': path.name,
                'state': 'uploaded',
                'size': len(content),
                'digest': 'sha256:' + ('0' * 64 if self.corrupt_upload else hashlib.sha256(content).hexdigest()),
            }
            if path.name == 'build-info.json':
                self.assertEqual(json.loads(content)['commit'], COMMIT)
                self.metadata = json.loads(content)
            if path.name == 'SHA256SUMS':
                self.checksums = content.decode()
        self.release['assets'] = list(current.values())
        return subprocess.CompletedProcess(args, 0)

    def test_first_release_is_published_after_upload_and_tag_update(self):
        self.assertTrue(publisher.publish(self.root))
        self.assertFalse(self.release['draft'])
        self.assertFalse(self.release['prerelease'])
        self.assertEqual(self.tag_commit, COMMIT)
        self.assertEqual(self.release['id'], 42)
        self.assertEqual(len(self.release['assets']), 4)
        self.assertTrue(self.mutations[0][2]['draft'])
        self.assertIn(COMMIT, self.release['body'])

    def test_existing_release_reused_and_old_platform_asset_removed(self):
        self.release = {'id': 42, 'draft': False, 'assets': [
            {'id': 9, 'name': 'scnterm-macos-universal.zip', 'size': 1, 'state': 'uploaded'},
        ]}
        publisher.publish(self.root)
        self.assertEqual(self.release['id'], 42)
        self.assertFalse(any(method == 'POST' for method, _, _ in self.mutations))
        self.assertNotIn('scnterm-macos-universal.zip', [a['name'] for a in self.release['assets']])

    def seed_platform(self, platform, legacy=False):
        digest = hashlib.sha256(b'previous package').hexdigest()
        hashes = {name: digest for name in publisher.EXPECTED_FILES[platform]}
        self.release = {'id': 42, 'draft': False, 'assets': [
            {'id': i + 1, 'name': name, 'size': 16, 'state': 'uploaded', 'digest': 'sha256:' + digest}
            for i, name in enumerate(hashes)
        ] + [{'id': 50, 'name': 'build-info.json', 'size': 1, 'state': 'uploaded'}]}
        self.metadata = {'commit': 'b' * 40, 'run_url': 'https://example.test/previous',
                         'platforms': [platform], 'sha256': hashes}
        if not legacy:
            self.metadata['platform_builds'] = {
                platform: {'commit': 'b' * 40, 'run_url': 'https://example.test/previous'}
            }

    def test_windows_update_preserves_ubuntu_files_versions_and_checksums(self):
        self.seed_platform('ubuntu26.04')
        original = self.release['assets'][:-1].copy()
        with patch.dict(os.environ, {'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            self.assertTrue(publisher.publish(self.root))
        for asset in original:
            self.assertIn(asset, self.release['assets'])
            self.assertIn(asset['name'], self.checksums)
        self.assertEqual(self.metadata['platform_builds']['ubuntu26.04']['commit'], 'b' * 40)
        self.assertEqual(self.metadata['platform_builds']['windows']['commit'], COMMIT)
        self.assertIn('b' * 40, self.release['body'])
        self.assertEqual(self.metadata['platforms'], ['ubuntu26.04', 'windows'])

    def test_ubuntu_update_preserves_windows_and_migrates_legacy_metadata(self):
        self.seed_platform('windows', legacy=True)
        (self.root / 'ubuntu26.04').mkdir()
        for name in publisher.EXPECTED_FILES['ubuntu26.04']:
            (self.root / 'ubuntu26.04' / name).write_bytes(b'new ubuntu package')
        with patch.dict(os.environ, {'EXPECTED_PLATFORMS': 'ubuntu26.04',
                                     'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            self.assertTrue(publisher.publish(self.root))
        self.assertEqual(self.metadata['platform_builds']['windows']['commit'], 'b' * 40)
        self.assertEqual(self.metadata['platform_builds']['ubuntu26.04']['commit'], COMMIT)
        for name in publisher.EXPECTED_FILES['windows']:
            self.assertIn(name, self.checksums)

    def test_first_platform_can_publish_before_the_other_exists(self):
        with patch.dict(os.environ, {'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            self.assertTrue(publisher.publish(self.root))
        self.assertEqual(self.metadata['platforms'], ['windows'])

    def test_invalid_retained_metadata_stops_before_upload(self):
        self.seed_platform('ubuntu26.04')
        self.metadata['sha256'] = {}
        with patch.dict(os.environ, {'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            with self.assertRaisesRegex(ValueError, '校验值缺失'):
                publisher.publish(self.root)
        self.assertEqual(self.upload_count, 0)
        self.assertEqual(self.mutations, [])

    def test_sequential_platform_publications_merge_the_release(self):
        with patch.dict(os.environ, {'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            self.assertTrue(publisher.publish(self.root))
            (self.root / 'ubuntu26.04').mkdir()
            for name in publisher.EXPECTED_FILES['ubuntu26.04']:
                (self.root / 'ubuntu26.04' / name).write_bytes(b'ubuntu package')
            with patch.dict(os.environ, {'EXPECTED_PLATFORMS': 'ubuntu26.04'}):
                self.assertTrue(publisher.publish(self.root))
        expected = set(publisher.EXPECTED_FILES['windows']) | set(publisher.EXPECTED_FILES['ubuntu26.04'])
        self.assertEqual(set(self.metadata['sha256']), expected)
        self.assertEqual(set(self.metadata['platform_builds']), {'windows', 'ubuntu26.04'})
        self.assertEqual({a['name'] for a in self.release['assets']}, expected | {'build-info.json', 'SHA256SUMS'})

    def test_failed_update_does_not_remove_other_platform(self):
        self.seed_platform('ubuntu26.04')
        original = self.release['assets'].copy()
        self.fail_upload = True
        with patch.dict(os.environ, {'RELEASE_PLATFORMS': 'windows,ubuntu26.04'}):
            with self.assertRaises(subprocess.CalledProcessError):
                publisher.publish(self.root)
        self.assertEqual(self.release['assets'], original)
        self.assertEqual(self.mutations, [])

    def test_stale_commit_does_not_mutate_release(self):
        self.current = 'c' * 40
        self.assertFalse(publisher.publish(self.root))
        self.assertEqual(self.mutations, [])
        self.assertEqual(self.upload_count, 0)

    def test_missing_platform_asset_does_not_mutate_release(self):
        (self.root / 'windows' / publisher.EXPECTED_FILES['windows'][0]).unlink()
        with self.assertRaises(ValueError):
            publisher.publish(self.root)
        self.assertEqual(self.mutations, [])

    def test_upload_failure_does_not_advance_tag_or_publish_draft(self):
        self.fail_upload = True
        with self.assertRaises(subprocess.CalledProcessError):
            publisher.publish(self.root)
        self.assertEqual(self.tag_commit, 'b' * 40)
        self.assertTrue(self.release['draft'])

    def test_wrong_remote_digest_does_not_advance_tag(self):
        self.corrupt_upload = True
        with self.assertRaisesRegex(RuntimeError, '校验失败'):
            publisher.publish(self.root)
        self.assertEqual(self.tag_commit, 'b' * 40)
        self.assertTrue(self.release['draft'])

    def test_non_personal_branch_cannot_publish(self):
        with patch.dict(os.environ, {'GITHUB_REF': 'refs/heads/main'}):
            with self.assertRaises(ValueError):
                publisher.publish(self.root)
        self.assertEqual(self.mutations, [])


if __name__ == '__main__':
    unittest.main()
