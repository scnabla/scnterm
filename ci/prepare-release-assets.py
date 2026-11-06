#!/usr/bin/env python3
"""将选定平台的上游产物整理到 release-assets 目录。"""

from pathlib import Path
import shutil
import sys

from scnterm_release_assets import ASSET_NAMES


def prepare(platform):
    names = ASSET_NAMES[platform]
    for name in names:
        path = Path(name)
        if not path.is_file() or path.is_symlink() or path.stat().st_size == 0:
            raise ValueError(f'产物缺失或为空：{name}')
    output = Path('release-assets')
    output.mkdir(exist_ok=False)
    for source, target in names.items():
        shutil.copy2(source, output / target)


if __name__ == '__main__':
    prepare(sys.argv[1])
