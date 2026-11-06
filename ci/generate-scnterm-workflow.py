#!/usr/bin/env python3
"""复用上游构建步骤，生成 scnterm 的单 Release 发布工作流。"""

from copy import deepcopy
import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / '.github/workflows/scnterm-release.yml'
ENABLED_PLATFORMS = ('ubuntu26.04', 'macos', 'windows')
CHECK_ONLY_PLATFORMS = ('ubuntu22.04', 'ubuntu24.04')
SPEC = importlib.util.spec_from_file_location('upstream_workflows', ROOT / 'ci/generate-workflows.py')
upstream = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(upstream)


def build_job(target, publish):
    if publish:
        job, _ = target.continuous()
    else:
        job = upstream.Job(
            runs_on=target.os,
            container=target.container,
            steps=target.prep_environment() + target.build_all_release() + target.test_all(),
            env=target.env,
        )
    target.global_env()
    job.env['CARGO_BUILD_JOBS'] = '3'
    if target.name == 'macos':
        # 使用 Intel 标准构建机器直接运行 x86_64 测试，仍编译双架构通用包。
        job.runs_on = 'macos-15-intel'
    # 容器内无需挂载 FUSE 即可执行 linuxdeploy。
    if target.app_image:
        job.env['APPIMAGE_EXTRACT_AND_RUN'] = '1'
        job.env['SCNTERM_RELEASE_TAG'] = 'latest'
        job.env['SCNTERM_APPIMAGE_NAME'] = 'scnterm-ubuntu26.04-x64.AppImage'
    steps = []
    for step in job.steps:
        if isinstance(step, upstream.RunStep):
            if step.name == 'Workaround git permissions issue':
                step.run = 'git config --global --add safe.directory "$GITHUB_WORKSPACE"'
            if step.name == 'Package':
                # 个人仓库不依赖上游的 Apple 签名凭据。
                step.env = None
            step.run = step.run.replace('cargo build ', 'cargo build --locked ')
            step.run = step.run.replace('cargo nextest run ', 'cargo nextest run --locked ')
        if isinstance(step, upstream.CheckoutStep):
            step.params['persist-credentials'] = False
        if isinstance(step, upstream.CacheStep):
            step.params['key'] = f"scnterm-{target.name}-${{{{ hashFiles('**/Cargo.lock') }}}}"
        if isinstance(step, upstream.ActionStep) and step.action.startswith('actions/upload-artifact@'):
            step.params['if-no-files-found'] = 'error'
            step.params['path'] = 'release-assets/*'
            python = 'python' if target.name == 'windows' else 'python3'
            steps.append(upstream.RunStep('整理下载文件', f'{python} ci/prepare-release-assets.py {target.name}'))
        steps.append(step)
    job.steps = steps
    return job


def generate():
    selected = ENABLED_PLATFORMS + CHECK_ONLY_PLATFORMS
    if len(set(selected)) != len(selected):
        raise ValueError('发布与仅检查的平台列表不能重复')
    targets = [deepcopy(t) for t in upstream.TARGETS if t.name in selected]
    if set(t.name for t in targets) != set(selected):
        raise ValueError('平台列表包含未知名称')
    with OUTPUT.open('w') as f:
        f.write('''# 由 ci/generate-scnterm-workflow.py 生成。
name: scnterm 自动构建与发布

on:
  push:
    branches: [personal]
  workflow_dispatch:

permissions:
  contents: read

jobs:
  validate:
    name: 检查发布脚本与工作流
    if: github.repository == 'scnabla/scnterm' && github.ref == 'refs/heads/personal'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false
      - run: python3 -B -m unittest discover -s ci/tests -p 'test_scnterm_*.py' -v
      - run: python3 -B ci/generate-scnterm-workflow.py
      - run: git diff --exit-code -- .github/workflows/scnterm-release.yml
''')
        names = []
        for target in targets:
            name = target.name.replace('.', '_')
            names.append(name)
            job = build_job(target, target.name in ENABLED_PLATFORMS)
            f.write(f'  {name}:\n')
            f.write("    if: github.repository == 'scnabla/scnterm' && github.ref == 'refs/heads/personal'\n")
            f.write(f'    name: {target.name}\n')
            f.write('    needs: validate\n')
            f.write(f'    runs-on: {upstream.yv(job.runs_on)}\n')
            f.write('    timeout-minutes: 120\n')
            if job.container:
                f.write(f'    container: {upstream.yv(job.container)}\n')
            f.write('    env:\n')
            for key, value in job.env.items():
                f.write(f'      {key}: {upstream.yv(value)}\n')
            job.render(f, 3)
            f.write('\n')
        f.write('  publish:\n')
        f.write('    name: 更新 Release\n')
        f.write('    needs: [' + ', '.join(names) + ']\n')
        f.write("    if: github.repository == 'scnabla/scnterm' && github.ref == 'refs/heads/personal'\n")
        f.write('''    runs-on: ubuntu-latest
    timeout-minutes: 30
    permissions:
      contents: write
    concurrency:
      group: scnterm-release-publish
      cancel-in-progress: false
    steps:
      - uses: actions/checkout@v5
        with:
          persist-credentials: false
      - uses: actions/download-artifact@v8
        with:
          path: release-artifacts
      - name: 校验产物并更新固定 Release
        env:
          GH_TOKEN: ${{ github.token }}
''')
        f.write('          EXPECTED_PLATFORMS: ' + ','.join(ENABLED_PLATFORMS) + '\n')
        f.write('        run: python3 ci/publish-scnterm-release.py release-artifacts\n')


if __name__ == '__main__':
    generate()
