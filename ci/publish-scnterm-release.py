#!/usr/bin/env python3
"""校验构建产物并更新当前仓库的固定 Release。"""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

TAG = 'latest'
from scnterm_release_assets import EXPECTED_FILES


def gh(*args, payload=None, missing_ok=False):
    command = ['gh', *args]
    if payload is not None:
        command += ['--input', '-']
    result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                            text=True, capture_output=True)
    if result.returncode:
        if missing_ok and '(HTTP 404)' in result.stderr:
            return None
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout) if result.stdout.strip() else None


def collect_assets(root, platforms):
    """拒绝缺少平台、空文件和重名产物，避免不完整发布。"""
    if not platforms or len(set(platforms)) != len(platforms):
        raise ValueError('发布平台列表为空或重复')
    files = {}
    for platform in platforms:
        required = EXPECTED_FILES[platform]
        directory = root / platform
        actual = {p.name: p for p in directory.iterdir() if p.is_file()}
        if set(actual) != set(required):
            raise ValueError(f'{platform} 产物不匹配：缺少 {set(required) - set(actual)}，多出 {set(actual) - set(required)}')
        for name, path in actual.items():
            if name in files or path.is_symlink() or path.stat().st_size == 0:
                raise ValueError(f'产物重名、为空或为符号链接：{path}')
            files[name] = path
    return files


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def publish(root):
    repo = os.environ['GITHUB_REPOSITORY']
    commit = os.environ['GITHUB_SHA']
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repo):
        raise ValueError('仓库名称无效')
    if not re.fullmatch(r'[0-9a-f]{40}', commit):
        raise ValueError('提交编号无效')
    if os.environ['GITHUB_REF'] != 'refs/heads/personal':
        raise ValueError('仅允许 personal 分支发布')
    platforms = os.environ['EXPECTED_PLATFORMS'].split(',')
    files = collect_assets(root, platforms)
    endpoint = f'repos/{repo}'
    current = gh('api', f'{endpoint}/git/ref/heads/personal')['object']['sha']
    if current != commit:
        print('personal 已有更新提交，跳过本次旧提交的发布。')
        return False

    # 只有全部平台通过后才进入发布任务；首次创建时保持草稿直到产物上传完成。
    release = gh('api', f'{endpoint}/releases/tags/{TAG}', missing_ok=True)
    if release is not None and release.get('immutable'):
        raise RuntimeError('Release 为不可修改状态，无法更新')
    if release is None:
        release = gh('api', '--method', 'POST', f'{endpoint}/releases', payload={
            'tag_name': TAG, 'target_commitish': commit,
            'name': 'scnterm', 'draft': True, 'prerelease': False,
        })
    release_id = release['id']
    run_url = f'https://github.com/{repo}/actions/runs/{os.environ["GITHUB_RUN_ID"]}'
    with tempfile.TemporaryDirectory(prefix='scnterm-release-') as temp:
        temp = Path(temp)
        info = temp / 'build-info.json'
        info.write_text(json.dumps({
            'repository': repo, 'commit': commit, 'platforms': platforms,
            'run_url': run_url,
            'sha256': {name: sha256(path) for name, path in sorted(files.items())},
        }, ensure_ascii=False, indent=2) + '\n')
        files[info.name] = info
        sums = temp / 'SHA256SUMS'
        sums.write_text(''.join(f'{sha256(path)}  {name}\n' for name, path in sorted(files.items())))
        files[sums.name] = sums
        subprocess.run(['gh', 'release', 'upload', TAG, '--repo', repo, '--clobber',
                        *[str(path) for _, path in sorted(files.items())]], check=True)
        remote = gh('api', f'{endpoint}/releases/{release_id}')
        assets = {asset['name']: asset for asset in remote['assets']}
        for name, path in files.items():
            asset = assets.get(name)
            if asset is None or asset['state'] != 'uploaded' or asset['size'] != path.stat().st_size:
                raise RuntimeError(f'远程产物缺失或不完整：{name}')
            if asset.get('digest') and asset['digest'] != 'sha256:' + sha256(path):
                raise RuntimeError(f'远程产物校验失败：{name}')
        for name, asset in assets.items():
            if name not in files:
                gh('api', '--method', 'DELETE', f'{endpoint}/releases/assets/{asset["id"]}')

    # Release 的 target_commitish 不会移动已存在的标签，因此单独更新 latest 引用。
    ref = gh('api', f'{endpoint}/git/ref/tags/{TAG}', missing_ok=True)
    if ref is None:
        gh('api', '--method', 'POST', f'{endpoint}/git/refs', payload={
            'ref': f'refs/tags/{TAG}', 'sha': commit,
        })
    else:
        gh('api', '--method', 'PATCH', f'{endpoint}/git/refs/tags/{TAG}', payload={
            'sha': commit, 'force': True,
        })
    body = (
        '基于 WezTerm 的 scnterm 自动构建版本。\n\n'
        f'- 源码提交：[{commit}](https://github.com/{repo}/commit/{commit})\n'
        f'- 构建平台：{", ".join(platforms)}\n'
        f'- [构建与测试记录]({run_url})\n'
        '- 文件校验值见 `SHA256SUMS`，构建信息见 `build-info.json`。\n\n'
        '可执行文件和配置接口沿用 WezTerm 命名。此页面持续更新为最新通过构建和测试的版本。\n'
    )
    gh('api', '--method', 'PATCH', f'{endpoint}/releases/{release_id}', payload={
        'tag_name': TAG, 'target_commitish': commit, 'name': 'scnterm',
        'body': body, 'draft': False, 'prerelease': False, 'make_latest': 'true',
    })
    print(f'已更新 https://github.com/{repo}/releases/tag/{TAG}，提交 {commit}')
    return True


if __name__ == '__main__':
    publish(Path(sys.argv[1]))
