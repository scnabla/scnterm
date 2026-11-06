# scnterm

scnterm 是基于 [WezTerm](https://github.com/wezterm/wezterm) 的个人维护分支，由 [scnabla](https://github.com/scnabla) 维护，按日常使用需求开发终端功能，并持续接收上游更新。个人偏好通过配置控制。

项目已完成 Linux x86_64 的首次构建和基础运行验证。当前功能源码与官方基准提交 `e68a8ec27727fbbda15e07f0a0737cd5e16481f6` 一致，构建与测试结果见 [开发环境](docs/development-environment.md)。

## 项目文档

- [开发与维护步骤](docs/README.md)：项目约定、功能开发、上游同步和发布流程。
- [Linux 开发环境](docs/development-environment.md)：依赖安装、构建命令、独立启动与测试结果。
- [个人终端配置](https://github.com/scnabla/personal-config)：通过 Lua 配置实现的个人设置。
- [WezTerm 官方文档](https://wezterm.org/)：已有终端功能、Lua 配置和命令行使用说明。

## 分支与版本

| 分支 | 用途 |
| --- | --- |
| `personal` | 默认分支，保存项目文档和个人改动，作为个人版本的构建来源 |
| `main` | 跟随官方 `main`，保留上游内容 |

功能开发从 `personal` 创建 `feature/<功能名>`，上游更新验证使用 `update/<日期>`，验证通过后合入 `personal`。

仓库保留从上游继承的版本标签，包括 `nightly`。这些标签用于定位上游历史提交，不代表 scnterm 已发布对应安装包。

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
