# 自动构建与发布

## 触发与发布范围

向 `personal` 分支推送提交后，GitHub Actions 执行 `scnterm 自动构建与发布` 工作流。代码、配置和文档的提交都会触发；仅在本地创建提交不会触发。

| 平台 | CPU 架构 | 执行内容 |
| --- | --- | --- |
| Windows | x86_64 | 编译、测试、打包并发布 |
| Ubuntu 26.04 | x86_64 | 编译、测试、打包并发布 |
| Ubuntu 22.04 | x86_64 | 编译与测试 |
| Ubuntu 24.04 | x86_64 | 编译与测试 |

各平台复用 `ci/generate-workflows.py` 中的上游构建步骤。Windows 与 Ubuntu 26.04 各自在自身编译、测试和打包成功后更新对应附件，不等待其他平台。Ubuntu 22.04、24.04 仅做兼容性检查，其失败或取消不阻挡 Windows 和 Ubuntu 26.04 发布。

macOS 不参与当前自动编译、测试和发布，也不作为 Release 更新的前置条件。

编译检查验证平台代码能否编译、链接，自动测试验证已有测试覆盖的功能。窗口显示、输入法、字体和快捷键等桌面行为需要对应系统的实机验证。

## 固定下载入口

[scnterm 下载](https://github.com/scnabla/scnterm/releases/tag/latest) 使用标题为 `scnterm`、标签为 `latest` 的同一个 Release，每次平台发布只替换该平台的安装包，并更新对应版本信息及说明。其他启用平台的安装包继续保留。

| 文件 | 用途 |
| --- | --- |
| `scnterm-windows-x64.zip` | Windows 便携包 |
| `scnterm-windows-x64-setup.exe` | Windows 安装程序 |
| `scnterm-ubuntu26.04-amd64.deb` | Ubuntu 26.04 安装包 |
| `scnterm-ubuntu26.04-amd64.tar.xz` | Ubuntu 26.04 文件归档 |
| `scnterm-ubuntu26.04-x64.AppImage` | Linux AppImage |
| `scnterm-ubuntu26.04-x64.AppImage.zsync` | AppImage 更新所需的校验数据 |
| `scnterm-src.tar.gz` | 随 Ubuntu 26.04 更新的源码包，包含上游脚本收集的子模块 |
| `SHA256SUMS` | 安装包与构建信息的 SHA-256 校验值 |
| `build-info.json` | 各平台的源码提交、工作流运行地址及安装包校验值 |

下载文件和解压后的外层目录使用 scnterm 名称。命令名称、系统安装包标识以及配置路径沿用 WezTerm。应用标识与安装路径的调整按独立任务处理。

`latest` 标签和 `build-info.json` 顶层的 `commit`、`run_url` 表示最近一次平台发布更新。不同平台可以对应不同源码提交，应通过 `platform_builds.<平台名>.commit` 核对具体安装包的版本。Release 说明也按平台列出提交和构建记录。其他继承自上游的历史标签继续保留。

## 发布条件

- 构建和测试任务只有仓库读取权限，发布任务使用 `GITHUB_TOKEN` 的仓库内容写入权限。
- 某个平台构建、测试或打包失败、被取消时，只跳过该平台的发布，其上一版安装包保留；其他成功平台独立更新。源码推送成功不表示各个平台下载包都已更新。
- 发布任务只下载自身平台的产物，检查是否齐全、是否为空；上传后核对文件大小和远程校验值。
- 更新构建信息和 `SHA256SUMS` 时合并其他启用平台的原有版本与校验值，不把未更新的平台标成新提交。原有信息缺失或不匹配时停止发布。
- 各平台发布任务共用串行执行组，避免同时修改同一个 Release 的版本信息和校验文件；构建任务仍并行运行。
- 发布前比较构建提交与 `personal` 最新提交。已有更新提交时，跳过旧提交的发布。
- 同一个 Release 的同名附件会被替换，已取消发布的平台附件在成功上传新产物后清理。
- 第一次发布先建立草稿，产物校验完成后公开。后续更新沿用已有 Release。

GitHub 附件逐个上传，已有 Release 的附件替换不是一次性完成的。上传失败时可以重新运行该次工作流中的失败任务。

## 修改平台列表

编辑 `ci/generate-scnterm-workflow.py`：

```python
ENABLED_PLATFORMS = ('ubuntu26.04', 'windows')
CHECK_ONLY_PLATFORMS = ('ubuntu22.04', 'ubuntu24.04')
```

`ENABLED_PLATFORMS` 指定发布安装包的平台；`CHECK_ONLY_PLATFORMS` 指定只编译与测试的平台。同一平台不能同时出现在两个列表中。

现有生成器可选择 `ubuntu22.04`、`ubuntu24.04`、`ubuntu26.04`、`debian12`、`centos9`、`fedora41`、`macos`、`windows`。例如，需要发布 Ubuntu 24.04 安装包时，将 `ubuntu24.04` 从仅检查列表移到发布列表。

修改后重新生成工作流并检查发布脚本：

```bash
PYTHONDONTWRITEBYTECODE=1 python3 ci/generate-scnterm-workflow.py
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s ci/tests -p 'test_scnterm_*.py' -v
git diff --check
```

将生成器和 `.github/workflows/scnterm-release.yml` 一起提交并推送到 `personal`。日常功能修改不需要重新生成工作流。

若重新启用 macOS，生成器使用 `macos-15-intel` 构建 Intel 和 Apple Silicon 通用包，测试运行于 Intel 目标，不使用 Apple 签名和公证凭据。

## 手动重跑与上游同步

在 GitHub 仓库的 Actions 页面打开 `scnterm 自动构建与发布`，选择 `Run workflow`，分支选择 `personal`。也可在失败的运行记录中选择重新运行失败任务。

`personal` 分支的 `.github/workflows/` 只保留 `scnterm-release.yml`。上游原始工作流保留在 `main` 分支，平台构建步骤继续从 `ci/generate-workflows.py` 复用。

同步上游后重新运行 `ci/generate-scnterm-workflow.py`，检查构建环境、工具版本与产物文件名的变化。上游新增平台需要同时检查发布脚本中的产物清单。
