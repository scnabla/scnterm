# 自动构建与发布

## 触发与发布范围

向 `personal` 分支推送提交后，GitHub Actions 执行 `scnterm 自动构建与发布` 工作流。代码、配置和文档的提交都会触发；仅在本地创建提交不会触发。

| 平台 | CPU 架构 | 执行内容 |
| --- | --- | --- |
| Windows | x86_64 | 编译、测试、打包并发布 |
| Ubuntu 26.04 | x86_64 | 编译、测试、打包并发布 |
| Ubuntu 22.04 | x86_64 | 编译与测试 |
| Ubuntu 24.04 | x86_64 | 编译与测试 |
| macOS | x86_64、aarch64 | 编译、测试并发布通用安装包，测试沿用上游的 x86_64 目标 |

各平台复用 `ci/generate-workflows.py` 中的上游构建步骤。五个平台全部成功后，发布任务收集 Windows、Ubuntu 26.04 与 macOS 的产物。

macOS 任务使用 GitHub 的 `macos-15-intel` 标准构建机器，编译 Intel 和 Apple Silicon 两个目标并生成通用包，自动测试在 Intel 目标上运行。

编译检查验证平台代码能否编译、链接，自动测试验证已有测试覆盖的功能。窗口显示、输入法、字体和快捷键等桌面行为需要对应系统的实机验证。

## 固定下载入口

[scnterm 下载](https://github.com/scnabla/scnterm/releases/tag/latest) 使用标题为 `scnterm`、标签为 `latest` 的同一个 Release，每次发布替换安装包、更新提交编号及说明。

| 文件 | 用途 |
| --- | --- |
| `scnterm-windows-x64.zip` | Windows 便携包 |
| `scnterm-windows-x64-setup.exe` | Windows 安装程序 |
| `scnterm-macos-universal.zip` | macOS Intel 与 Apple Silicon 通用包 |
| `scnterm-ubuntu26.04-amd64.deb` | Ubuntu 26.04 安装包 |
| `scnterm-ubuntu26.04-amd64.tar.xz` | Ubuntu 26.04 文件归档 |
| `scnterm-ubuntu26.04-x64.AppImage` | Linux AppImage |
| `scnterm-ubuntu26.04-x64.AppImage.zsync` | AppImage 更新所需的校验数据 |
| `scnterm-src.tar.gz` | 包含上游脚本所收集子模块的源码包 |
| `SHA256SUMS` | 安装包与构建信息的 SHA-256 校验值 |
| `build-info.json` | 源码提交、发布平台和工作流运行地址 |

下载文件使用 scnterm 名称，包内程序名称、系统安装包标识与安装位置沿用 WezTerm。应用标识与安装路径的调整按独立任务处理。

`latest` 标签跟随成功发布的个人分支提交更新；其他继承自上游的历史标签继续保留。

## 发布条件

- 构建和测试任务只有仓库读取权限，发布任务使用 `GITHUB_TOKEN` 的仓库内容写入权限。
- 构建或测试失败时，发布任务不运行，已有 Release 不变。
- 发布前检查所需平台的产物是否齐全、是否为空，上传后核对文件大小和远程提供的校验值。
- 发布任务串行执行，避免同时修改同一个 Release。
- 发布前比较构建提交与 `personal` 最新提交。已有更新提交时，跳过旧提交的发布。
- 同一个 Release 的同名附件会被替换，已取消发布的平台附件在成功上传新产物后清理。
- 第一次发布先建立草稿，产物校验完成后公开。后续更新沿用已有 Release。

GitHub 附件逐个上传，已有 Release 的附件替换不是一次性完成的。上传失败时可以重新运行该次工作流中的失败任务。

## 修改平台列表

编辑 `ci/generate-scnterm-workflow.py`：

```python
ENABLED_PLATFORMS = ('ubuntu26.04', 'macos', 'windows')
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

发布 macOS 安装包时，当前流程不使用 Apple 签名和公证凭据。

## 手动重跑与上游同步

在 GitHub 仓库的 Actions 页面打开 `scnterm 自动构建与发布`，选择 `Run workflow`，分支选择 `personal`。也可在失败的运行记录中选择重新运行失败任务。

`personal` 分支的 `.github/workflows/` 只保留 `scnterm-release.yml`。上游原始工作流保留在 `main` 分支，平台构建步骤继续从 `ci/generate-workflows.py` 复用。

同步上游后重新运行 `ci/generate-scnterm-workflow.py`，检查构建环境、工具版本与产物文件名的变化。上游新增平台需要同时检查发布脚本中的产物清单。
