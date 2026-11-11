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


def retained_platforms(release, endpoint, active, updating):
    """保留本次未更新的平台及其原始版本、校验值，兼容旧构建信息。"""
    assets = {asset['name']: asset for asset in release['assets']}
    info = {}
    if 'build-info.json' in assets:
        info = gh('api', '-H', 'Accept: application/octet-stream',
                  f'{endpoint}/releases/assets/{assets["build-info.json"]["id"]}')
    builds, hashes = {}, {}
    for platform in active:
        if platform in updating:
            continue
        required = set(EXPECTED_FILES[platform])
        present = required & assets.keys()
        if not present:
            continue
        if present != required:
            raise ValueError(f'{platform} 原有产物不完整，保留现有 Release 并停止发布')
        build = info.get('platform_builds', {}).get(platform)
        if build is None and platform in info.get('platforms', []):
            build = {'commit': info.get('commit'), 'run_url': info.get('run_url')}
        if not build or not re.fullmatch(r'[0-9a-f]{40}', build.get('commit') or '') or not build.get('run_url'):
            raise ValueError(f'{platform} 原有版本信息缺失，无法准确保留')
        builds[platform] = build
        for name in required:
            asset = assets[name]
            digest = info.get('sha256', {}).get(name)
            if not re.fullmatch(r'[0-9a-f]{64}', digest or ''):
                raise ValueError(f'{name} 原有校验值缺失')
            if asset['state'] != 'uploaded' or asset['size'] <= 0:
                raise ValueError(f'{name} 原有产物不可用')
            if asset.get('digest') and asset['digest'] != 'sha256:' + digest:
                raise ValueError(f'{name} 原有校验值不匹配')
            hashes[name] = digest
    return builds, hashes


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
    active = os.environ.get('RELEASE_PLATFORMS', os.environ['EXPECTED_PLATFORMS']).split(',')
    if not active or len(set(active)) != len(active) or any(p not in EXPECTED_FILES for p in active):
        raise ValueError('启用的发布平台列表无效')
    if not set(platforms) <= set(active):
        raise ValueError('本次发布包含已停用的平台')
    files = collect_assets(root, platforms)
    endpoint = f'repos/{repo}'
    current = gh('api', f'{endpoint}/git/ref/heads/personal')['object']['sha']
    if current != commit:
        print('personal 已有更新提交，跳过本次旧提交的发布。')
        return False

    # 每个平台在自身构建与测试通过后独立发布；首次创建保持草稿直到上传完成。
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
    builds, hashes = retained_platforms(release, endpoint, active, platforms)
    for platform in platforms:
        builds[platform] = {'commit': commit, 'run_url': run_url}
    hashes.update({name: sha256(path) for name, path in files.items()})
    with tempfile.TemporaryDirectory(prefix='scnterm-release-') as temp:
        temp = Path(temp)
        info = temp / 'build-info.json'
        info.write_text(json.dumps({
            'schema_version': 2,
            'repository': repo, 'commit': commit, 'platforms': sorted(builds),
            'run_url': run_url,
            'platform_builds': builds,
            'sha256': dict(sorted(hashes.items())),
        }, ensure_ascii=False, indent=2) + '\n')
        files[info.name] = info
        sums = temp / 'SHA256SUMS'
        checksum_hashes = {**hashes, info.name: sha256(info)}
        sums.write_text(''.join(f'{digest}  {name}\n' for name, digest in sorted(checksum_hashes.items())))
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
        for name, digest in hashes.items():
            asset = assets.get(name)
            if not asset or asset['state'] != 'uploaded' or asset['size'] <= 0:
                raise RuntimeError(f'远程产物缺失或不完整：{name}')
            if asset.get('digest') and asset['digest'] != 'sha256:' + digest:
                raise RuntimeError(f'远程产物校验失败：{name}')
        disabled_files = {
            name for platform, names in EXPECTED_FILES.items() if platform not in active for name in names
        }
        active_files = {name for platform in active for name in EXPECTED_FILES[platform]}
        for name, asset in assets.items():
            if name in disabled_files - active_files:
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
    versions = ''.join(
        f'| {platform} | [{build["commit"][:9]}](https://github.com/{repo}/commit/{build["commit"]}) '
        f'| [构建与测试记录]({build["run_url"]}) |\n'
        for platform, build in sorted(builds.items())
    )
    body = (
        '基于 WezTerm 的 scnterm 自动构建版本。\n\n'
        '各平台在自身构建和测试成功后独立更新；失败的平台保留上一版。\n\n'
        '| 平台 | 源码提交 | 验证记录 |\n| --- | --- | --- |\n' + versions + '\n'
        '文件校验值见 `SHA256SUMS`，各平台构建信息见 `build-info.json` 的 `platform_builds`。\n'
        '`latest` 标签及顶层 `commit` 表示最近一次发布更新的提交，不代表所有平台使用相同提交。\n\n'
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
