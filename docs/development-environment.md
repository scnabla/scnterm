# Linux 开发环境

## 主机与工具

| 项目 | 配置 |
| --- | --- |
| 检查日期 | 2026-11-07 |
| 系统 | Ubuntu 26.04.1 LTS，x86_64 |
| 内核 | 7.0.0-38-generic |
| 源码与构建目录 | `/home/scnable/work/scnablaTerm` |
| Rust | 1.99.0，`x86_64-unknown-linux-gnu` |
| Cargo | 1.99.0 |
| GCC / G++ | 15.2.0 |
| CMake | 4.2.3 |
| Git | 2.53.0 |
| GNU Make | 4.4.1 |
| pkg-config | 2.5.1 |

官方 Ubuntu 26.04 构建流程使用 stable Rust，没有固定工具链版本。本机已有 1.99.0，构建命令通过 `+1.99.0` 明确选择该版本。

## 源码与分支

| 项目 | 值 |
| --- | --- |
| 个人 Fork | `https://github.com/scnabla/scnterm` |
| `origin` | `https://github.com/scnabla/scnterm.git` |
| `upstream` | `https://github.com/wezterm/wezterm.git` |
| 官方基准提交 | `e68a8ec27727fbbda15e07f0a0737cd5e16481f6` |
| 官方提交日期 | 2026-10-26 |
| 首次构建来源提交 | `ac6cc6138bde0036f4370605397c30bb46b9a54c` |
| 提交邮箱 | `46028051+scnabla@users.noreply.github.com` |

`main` 指向官方基准提交并跟踪 `upstream/main`。`personal` 保存项目文档；首次构建来源提交仅比官方基准新增 `docs/README.md`，应用源码及依赖锁文件保持一致。

主仓库保留完整提交历史。首次获取后，主仓库对象约 312 MiB，子模块 Git 数据约 215 MiB。

| 子模块 | 提交 |
| --- | --- |
| `deps/freetype/freetype2` | `42608f77f20749dd6ddc9e0536788eaad70ea4b5` |
| `deps/freetype/freetype2/subprojects/dlg` | `72dfcc858c040c54a6a0b88fcb7e70ee186d3167` |
| `deps/freetype/libpng` | `f5e92d76973a7a53f517579bc95d61483bf108c0` |
| `deps/freetype/zlib` | `51b7f2abdade71cd9bb0e7a373ef2610ec6f9daf` |
| `deps/harfbuzz/harfbuzz` | `33a3f8de60dcad7535f14f07d6710144548853ac` |

## 系统依赖

依赖按当前源码的 `get-deps` 中 Ubuntu/Debian 清单安装。通过 `pkexec` 弹出系统身份验证窗口，安装到系统目录：

```bash
pkexec /usr/bin/apt-get install -y cmake dpkg-dev fakeroot gcc g++ libegl1-mesa-dev libssl-dev libfontconfig1-dev libwayland-dev libx11-xcb-dev libxcb-ewmh-dev libxcb-icccm4-dev libxcb-image0-dev libxcb-keysyms1-dev libxcb-randr0-dev libxcb-render0-dev libxcb-xkb-dev libxkbcommon-dev libxkbcommon-x11-dev libxcb-util-dev xorg-dev
```

本次安装新增 100 个软件包、升级 1 个关联软件包，下载约 32.3 MB，安装后新增约 114 MB。其余基础工具在安装前已存在。

`pkg-config` 已检出以下开发库：

| 开发库 | 版本 |
| --- | --- |
| OpenSSL | 3.5.5 |
| Fontconfig | 2.17.1 |
| Wayland client | 1.24.0 |
| X11 | 1.8.13 |
| XCB | 1.17.0 |
| xkbcommon | 1.13.1 |
| EGL API | 1.5 |

Rust 工具链保存在 `~/.rustup`，Cargo 依赖缓存保存在 `~/.cargo`，编译产物保存在项目的 `target/`。日志与测试配置保存在项目的 `.cache/scnterm-bootstrap-20261107/`。`target/` 和 `.cache/` 已由官方 `.gitignore` 排除。

## 构建命令

在项目目录执行：

```bash
git submodule update --init --recursive --jobs 4
cargo +1.99.0 fetch --locked
mkdir -p .cache/scnterm-bootstrap-20261107
/usr/bin/time -v cargo +1.99.0 build --release --locked -j 4 -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes > .cache/scnterm-bootstrap-20261107/build-release.log 2>&1
```

`--locked` 使用仓库中现有的 `Cargo.lock`。`-j 4` 将并行构建任务限制为 4 个。

首次 Release 构建成功，耗时 3 分 17 秒，退出码为 0。版本命令输出 `wezterm 20261107-021929-ac6cc613`。构建日志包含上游源码的编译警告。

| 产物 | 文件大小 |
| --- | --- |
| `target/release/wezterm` | 约 38 MiB |
| `target/release/wezterm-gui` | 约 87 MiB |
| `target/release/wezterm-mux-server` | 约 34 MiB |
| `target/release/strip-ansi-escapes` | 约 1.7 MiB |

首次构建结束时，项目总占用约 2.9 GiB，其中 `target/` 约 2.1 GiB、`.git/` 约 528 MiB。此测量不含用户目录中的共享 Cargo 缓存和 Rust 工具链，也不含之后测试产生的额外产物。

## 独立启动与基础验证

本机测试配置位于 `.cache/scnterm-bootstrap-20261107/wezterm-test.lua`：

```lua
return {
  check_for_updates = false,
  default_prog = { '/bin/bash', '--noprofile', '--norc' },
  initial_cols = 100,
  initial_rows = 28,
  font_size = 13.0,
  exit_behavior = 'Close',
  window_close_confirmation = 'NeverPrompt',
}
```

在项目目录启动独立窗口：

```bash
./target/release/wezterm --config-file .cache/scnterm-bootstrap-20261107/wezterm-test.lua start --always-new-process --no-auto-connect
```

真实桌面会话为 Wayland。基础验证通过 `wezterm cli` 操作本次创建的 GUI 进程，并读取其窗格状态与终端文本：

| 检查项 | 结果 |
| --- | --- |
| 独立启动 GUI 与 Bash | 成功创建一个终端窗格 |
| 命令输入与中文输出 | 发送 `printf` 命令后读取到英文标记与中文文本 |
| 窗格拆分 | 成功创建左右两个窗格 |
| 活动窗格切换 | 目标窗格的 `is_active` 变为 `true` |
| 窗格尺寸调整 | 左侧宽度由 59 列变为 64 列，右侧由 40 列变为 35 列 |
| 关闭窗格 | 第二个 Shell 退出后恢复为单窗格 |
| 退出程序 | 最后一个 Shell 退出后 GUI 结束，退出码为 0 |

独立配置或 `--always-new-process` 启动的 GUI 不发布供自动查找使用的连接地址。自动化验证通过 `WEZTERM_UNIX_SOCKET` 指定本次进程的 `/run/user/<UID>/wezterm/gui-sock-<PID>`，再调用 CLI。首次使用窗口类名自动查找的尝试失败，其记录保存在 `smoke-discovery-failure.json`。

GUI 测试日志记录了 `writing pdu data buffer: Broken pipe (os error 32)`；上述检查均成功，GUI 正常退出。中文检查覆盖命令输出和终端文本读取，不包含输入法交互或字形外观检查。

## 终端核心测试

```bash
cargo +1.99.0 test --release --locked -j 4 -p wezterm-term -p wezterm-escape-parser -p wezterm-surface --lib > .cache/scnterm-bootstrap-20261107/tests.log 2>&1
```

| 模块 | 通过 | 失败 |
| --- | ---: | ---: |
| `wezterm-escape-parser` | 58 | 0 |
| `wezterm-surface` | 40 | 0 |
| `wezterm-term` | 53 | 0 |
| 合计 | 151 | 0 |

测试完成后，项目总占用约 3.3 GiB，其中 `target/` 约 2.5 GiB。用户目录中的共享 Cargo 缓存约 1.9 GiB，Rust 工具链约 2.1 GiB；这两个目录同时供其他项目使用。

## 本地记录

以下路径均相对于项目目录：

| 文件 | 内容 |
| --- | --- |
| `.cache/scnterm-bootstrap-20261107/environment.json` | 系统、工具版本与构建来源提交 |
| `.cache/scnterm-bootstrap-20261107/submodules.txt` | 递归子模块状态 |
| `.cache/scnterm-bootstrap-20261107/apt-history.log` | 安装完成时的本机 APT 历史副本 |
| `.cache/scnterm-bootstrap-20261107/cargo-fetch.log` | Rust 依赖下载日志 |
| `.cache/scnterm-bootstrap-20261107/build-release.log` | 构建输出与资源使用记录 |
| `.cache/scnterm-bootstrap-20261107/wezterm-test.lua` | 独立启动配置 |
| `.cache/scnterm-bootstrap-20261107/smoke.py` | 本机基础验证脚本 |
| `.cache/scnterm-bootstrap-20261107/smoke-result.json` | 基础验证结果与窗格尺寸 |
| `.cache/scnterm-bootstrap-20261107/terminal-output.txt` | 终端文本输出 |
| `.cache/scnterm-bootstrap-20261107/gui.log` | GUI 运行日志 |
| `.cache/scnterm-bootstrap-20261107/tests.log` | 三个终端核心模块的测试输出 |
