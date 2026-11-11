"""检查各平台发布任务只依赖自身构建。"""

import importlib.util
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'scnterm_workflow', Path(__file__).resolve().parents[1] / 'generate-scnterm-workflow.py'
)
generator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(generator)


class WorkflowTests(unittest.TestCase):
    def test_platform_releases_do_not_wait_for_other_builds(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / 'workflow.yml'
            with patch.object(generator, 'OUTPUT', output):
                generator.generate()
            text = output.read_text()
        jobs = dict(re.findall(r'^  ([\w]+):\n(.*?)(?=^  [\w]+:\n|\Z)', text, re.M | re.S))
        self.assertNotIn('macos', jobs)
        self.assertNotIn('publish', jobs)
        for platform in ('windows', 'ubuntu26.04'):
            build = platform.replace('.', '_')
            job = jobs['publish_' + build]
            self.assertEqual(re.findall(r'^    needs: (.+)$', job, re.M), [build])
            self.assertIn(f'          name: {platform}\n', job)
            self.assertIn(f'          path: release-artifacts/{platform}\n', job)
            self.assertIn(f'          EXPECTED_PLATFORMS: {platform}\n', job)
            self.assertIn('          RELEASE_PLATFORMS: ubuntu26.04,windows\n', job)
            self.assertIn('      group: scnterm-release-publish\n', job)
        for build in ('ubuntu22_04', 'ubuntu24_04'):
            self.assertIn(build, jobs)
            self.assertNotIn('publish_' + build, jobs)


if __name__ == '__main__':
    unittest.main()
