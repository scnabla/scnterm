# scnterm

scnterm 是基于 [WezTerm](https://github.com/wezterm/wezterm) 的个人维护分支，由 [scnabla](https://github.com/scnabla) 维护，按日常使用需求开发终端功能，并持续接收上游更新。个人偏好通过配置控制。

项目基于官方提交 `e68a8ec27727fbbda15e07f0a0737cd5e16481f6` 开发，已完成 Linux x86_64 构建和基础运行验证。应用原有名称显示位置使用 `scnterm`；关闭确认框支持左右方向键选择、Enter 确认，默认选中“否”，保留 Y、N、Esc 和鼠标操作。首次构建与测试结果见 [开发环境](docs/development-environment.md)。

Lua 配置可通过 `wezterm.app_name` 读取当前应用显示名称，用于自定义窗口标题。

## 项目文档

- [开发与维护步骤](docs/README.md)：项目约定、功能开发、上游同步和发布流程。
- [Linux 开发环境](docs/development-environment.md)：依赖安装、构建命令、独立启动与测试结果。
- [自动构建与发布](docs/continuous-release.md)：发布平台、兼容性检查和自动更新方式。
- [待修复问题](docs/known-issues.md)：已记录的缺陷、排查方向和验收要求。
- [功能待办](docs/todo.md)：待开发功能、实施顺序和验收要求。
- [个人终端配置](https://github.com/scnabla/personal-config)：通过 Lua 配置实现的个人设置。
- [WezTerm 官方文档](https://wezterm.org/)：已有终端功能、Lua 配置和命令行使用说明。

## 分支与版本

| 分支 | 用途 |
| --- | --- |
| `personal` | 默认分支，保存项目文档和个人改动，作为个人版本的构建来源 |
| `main` | 跟随官方 `main`，保留上游内容 |

功能开发从 `personal` 创建 `feature/<功能名>`，上游更新验证使用 `update/<日期>`，验证通过后合入 `personal`。

向 `personal` 推送提交后，自动编译并测试 Windows、Ubuntu 22.04、Ubuntu 24.04、Ubuntu 26.04 和 macOS。全部通过后，Windows、Ubuntu 26.04 与 macOS 安装包更新到同一个 [Release](https://github.com/scnabla/scnterm/releases/tag/latest)。

`latest` 标签用于 scnterm 的持续发布；其他上游历史标签保留用于定位原始版本。

## Linux 构建

安装依赖后，在项目目录执行：

```bash
git submodule update --init --recursive --jobs 4
cargo +1.99.0 build --release --locked -j 4 -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes
```

构建产物位于 `target/release/`。完整依赖清单、工具版本和独立测试配置见 [Linux 开发环境](docs/development-environment.md)。

## 上游与许可证

WezTerm 由 Wez Furlong 及贡献者开发。scnterm 保留上游提交历史、版权声明及 [MIT 许可证](LICENSE.md)。随程序分发的字体许可证位于 [assets/fonts](assets/fonts)。

- [WezTerm 官方仓库](https://github.com/wezterm/wezterm)
- [WezTerm 官方发布](https://github.com/wezterm/wezterm/releases)
