"""上游打包文件与 scnterm 下载文件的对应关系。"""

ASSET_NAMES = {
    'windows': {
        'WezTerm-windows-nightly.zip': 'scnterm-windows-x64.zip',
        'WezTerm-nightly-setup.exe': 'scnterm-windows-x64-setup.exe',
    },
    'macos': {'WezTerm-macos-nightly.zip': 'scnterm-macos-universal.zip'},
    **{
        'ubuntu' + version: {
            f'wezterm-nightly.Ubuntu{version}.deb': f'scnterm-ubuntu{version}-amd64.deb',
            f'wezterm-nightly.Ubuntu{version}.tar.xz': f'scnterm-ubuntu{version}-amd64.tar.xz',
        }
        for version in ('22.04', '24.04', '26.04')
    },
    'debian12': {
        'wezterm-nightly.Debian12.deb': 'scnterm-debian12-amd64.deb',
        'wezterm-nightly.Debian12.tar.xz': 'scnterm-debian12-amd64.tar.xz',
    },
    **{
        platform: {
            f'{package}-nightly-{platform}.rpm': f'{package.replace("wezterm", "scnterm")}-{platform}-x64.rpm'
            for package in ('wezterm', 'wezterm-common', 'wezterm-gui', 'wezterm-mux-server')
        }
        for platform in ('centos9', 'fedora41')
    },
}
ASSET_NAMES['ubuntu26.04'].update({
    'scnterm-ubuntu26.04-x64.AppImage': 'scnterm-ubuntu26.04-x64.AppImage',
    'scnterm-ubuntu26.04-x64.AppImage.zsync': 'scnterm-ubuntu26.04-x64.AppImage.zsync',
    'wezterm-nightly-src.tar.gz': 'scnterm-src.tar.gz',
})
EXPECTED_FILES = {platform: tuple(names.values()) for platform, names in ASSET_NAMES.items()}
