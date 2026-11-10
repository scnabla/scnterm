# SSH 常用目录

ScnTerm 的常用目录菜单用于在内置 SSH 会话中保存和切换远端目录。每个连接单独保存列表，菜单只显示当前连接的记录。

## 配置快捷键

在主 Lua 的现有 `config.keys` 表中增加：

```lua
{ key = 'p', mods = 'ALT', action = wezterm.action.ShowDirectoryMenu },
```

如果需要同时兼容未包含目录菜单的旧版本，可以在 `config.keys` 定义后、`return config` 前添加：

```lua
local supported, action = pcall(function()
  return wezterm.action.ShowDirectoryMenu
end)
if supported then
  table.insert(config.keys, { key = 'p', mods = 'ALT', action = action })
end
```

## 菜单操作

通过 `Alt+L` 建立内置 SSH 连接，回到空白命令提示符后按 `Alt+P`。

| 操作 | 行为 |
| --- | --- |
| `↑`、`↓` | 移动当前选择 |
| 选中“添加当前目录”后按回车 | 读取当前 SSH Shell 的目录并保存，重复添加不产生重复记录 |
| 选中已保存的路径后按回车 | 在当前 SSH 窗格执行 `cd`，关闭菜单 |
| `-` | 删除选中的目录记录，菜单继续显示；服务器上的目录和文件不受影响 |
| `Esc` | 关闭菜单 |

选中项使用亮橘黄色文字、加粗及箭头标记。列表超过窗口高度时随选择滚动。删除最后一条记录后仍保留“添加当前目录”。

连接名称、实际主机、端口和用户名共同区分列表。相同连接的新标签页和分屏共享已保存目录；更换连接目标或用户名后使用另一份列表。

## 当前目录与切换命令

添加目录时，菜单向当前 SSH Shell 发送一次读取 `$PWD` 的命令，通过终端控制序列返回路径；不从屏幕提示符猜测目录，也不使用另一个 SSH 会话的目录。远端需要兼容 POSIX 的 Shell（如 Bash、Zsh）以及 `printf`、`base64`、`tr`，无需安装 ScnTerm 或修改远端启动配置。

查询命令会出现在当前会话中，是否进入命令历史由远端 Shell 的历史配置决定。未能在 4 秒内取得本次查询的结果时显示提示，不保存旧结果。

切换目录使用带引号的绝对路径，支持中文、空格、单引号及 Shell 特殊字符。目录已删除、没有访问权限等错误由远端 Shell 显示。

添加和切换都应在空白命令提示符下使用，避免将命令送入正在运行的程序或接在尚未完成的输入后面。处于全屏应用的替代屏幕时拒绝发送命令。本地终端、串口以及在本地 Shell 中手动执行的 `ssh` 命令不使用此菜单。

## 本地保存

目录记录默认保存在主 Lua 所在目录下的 `scnterm-directory-bookmarks.json`，关闭菜单或重启程序后保留。第一次添加目录时创建文件。

主 Lua 可指定其他记录文件：

```lua
config.directory_bookmarks_file = 'data/scnterm-directory-bookmarks.json'
```

相对路径按主 Lua 所在目录计算，不随程序启动位置改变。Windows 绝对路径可写成：

```lua
config.directory_bookmarks_file = 'D:/ScnTerm-data/directories.json'
```

- 省略配置或设为 `nil`：使用主 Lua 同目录下的默认文件。
- 使用 `--config-file` 指定另一份主 Lua：默认文件及相对路径跟随该 Lua 所在目录。
- 没有加载主 Lua：默认使用 `~/.config/wezterm/scnterm-directory-bookmarks.json`。
- 路径必须包含文件名；空路径、控制字符和 Windows 的 `C:filename` 等不完整绝对路径不接受。
- 保存主 Lua 后，下次打开目录菜单读取新位置；已经打开的菜单继续使用打开时的文件。
- 修改位置不会自动搬动、合并或删除旧记录。需要保留已有列表时，先关闭目录菜单，把旧文件复制到新位置；新位置已有文件时先备份并核对内容。

早期版本使用 `%APPDATA%\wezterm\scnterm-directory-bookmarks.json`（Windows）或 `$XDG_DATA_HOME/wezterm/scnterm-directory-bookmarks.json`（Linux，未设置时为 `~/.local/share/wezterm/scnterm-directory-bookmarks.json`）。升级后可把旧文件复制到主 Lua 同目录，也可通过 `directory_bookmarks_file` 继续指定旧位置。

该文件属于使用记录，独立于主 Lua 配置，不包含 SSH 密码或私钥。它包含服务器标识和目录路径，不应作为通用示例提交到配置仓库。

每个连接最多 128 个目录，最多保存 128 个连接；单条路径最多 4096 字节，整个文件最多 1 MiB。文件写入使用跨进程锁和临时文件替换。文件损坏或版本不受支持时保留原文件并提示错误。

## 代码组织

| 文件 | 职责 |
| --- | --- |
| `wezterm-gui/src/directory_menu/mod.rs` | 识别当前内置 SSH 连接、菜单绘制、键盘交互、查询当前目录及发送切换命令 |
| `wezterm-gui/src/directory_menu/storage.rs` | 连接隔离、路径校验、命令引号处理、本地存储及并发控制 |
| `config/src/directory_menu.rs` | 记录文件路径校验、默认位置及相对路径解析 |
| `config/src/config.rs`、`config/src/lib.rs` | 注册配置字段，并在加载主 Lua 时解析路径 |
| `config/src/keyassignment.rs` | 注册 `ShowDirectoryMenu` 动作 |
| `wezterm-gui/src/commands.rs` | 提供命令面板名称和说明 |
| `wezterm-gui/src/main.rs` | 注册独立模块 |
| `wezterm-gui/src/termwindow/mod.rs` | 动作转发，复用现有覆盖层线程及关闭处理 |

文件读写和等待远端结果在覆盖层线程中执行，SSH 原窗格继续接收输出。
