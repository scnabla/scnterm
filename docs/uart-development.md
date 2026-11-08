# Windows UART 开发与分步调试

## 第一阶段范围

串口用于查看设备日志和输入命令，输出接入现有终端窗格。连接参数保存在主 Lua 配置的 `config.serial_ports` 中，通过现有命令面板、启动菜单和 `wezterm connect` 入口连接。第一阶段不增加专用配置界面、HEX 收发和下载模式控制。

Windows Release 版本已通过 RK3568 实机收发验证，参数为 COM3、1500000、8N1、无流控。主配置指定 PowerShell 作为默认 Shell 时，通过 `Alt+L` 选择串口可在当前窗口新建串口标签页。输入 `uname -a` 后收到 Linux 4.19.232、aarch64 的系统信息，关闭后重新连接 COM3 可以继续收发。

## 文件分工

| 文件 | 职责 |
| --- | --- |
| `config/src/serial.rs` | Lua 参数定义、参数校验和串口设置转换 |
| `pty/src/serial.rs` | 设备打开、字节收发、连接结束和句柄释放 |
| `wezterm-gui-subcommands/src/serial.rs` | 串口命令行参数 |
| `wezterm-gui/src/serial.rs` | 串口命令与终端窗口的连接 |
| `wezterm-mux-server-impl/src/lib.rs` | 配置重载时更新后续串口连接的参数 |
| `docs/examples/serial.lua` | 可选的独立连接配置示例 |

后续 HEX 功能单独组织字节显示与发送代码，不通过修改普通终端字符显示来实现。

## 连接配置

在主配置 `wezterm.lua` 的 `return config` 前设置 `config.serial_ports`，根据 Windows 设备管理器修改 `port`，并按设备要求设置波特率：

```lua
config.serial_ports = {
  {
    name = 'board-console',
    port = 'COM3',
    baud = 115200,
    data_bits = 8,
    parity = 'None',
    stop_bits = 1,
    flow_control = 'None',
  },
}
```

上述代码沿用主配置中的 `config` 对象。修改通信参数并重载配置后，关闭原串口会话再重新连接，使新参数用于新连接。

正常启动 scnterm 后，通过启动菜单选择串口连接，与 SSH 共用入口。在主配置的 `config.keys` 中配置 `Alt+L`：

```lua
{ key = 'l', mods = 'ALT', action = wezterm.action.ShowLauncher },
```

已有该快捷键时直接保留。串口连接的 `name` 会显示在菜单中；选择后在当前窗口新建串口标签页。

| 配置项 | 可选值 |
| --- | --- |
| `name` | 连接名称，在所有终端连接配置中唯一 |
| `port` | Windows 串口名称，例如 `COM3`；省略时使用 `name` |
| `baud` | 大于零且不超过 `4294967295` 的整数，设备驱动须支持所选速率 |
| `data_bits` | `5`、`6`、`7`、`8` |
| `parity` | `'None'`、`'Odd'`、`'Even'` |
| `stop_bits` | `1`、`2` |
| `flow_control` | `'None'`、`'Software'`、`'Hardware'`；分别对应无流控、XON/XOFF、RTS/CTS |

示例明确设置 115200、8N1、无流控。已有 Lua 配置省略参数时，保留原来的 9600、8N1、XON/XOFF 默认值。

配置中的连接名用于现有连接入口：

```powershell
wezterm connect board-console
```

也可以直接指定端口和参数：

```powershell
wezterm serial COM3 --baud 115200 --data-bits 8 --parity none --stop-bits 1 --flow-control none
```

直接使用 `wezterm serial COM3` 时默认采用 115200、8N1、无流控，不读取同名 Lua 连接的参数。

## 连接状态处理

- 没有日志输出、读取超时或暂时没有可读数据时，继续等待，不据此判定断开。
- 读写出现不可恢复错误时结束连接，保留异常退出的窗格和已有输出。
- 主动关闭串口窗格时通知后台退出，释放串口句柄。
- USB 转串口仍连接电脑、但下游开发板断电或串口线断开时，没有通用的断开判定；第一阶段不通过心跳命令或载波检测信号判断开发板是否在线。
- 重新连接使用已有连接入口，第一阶段不自动重连，也不自动发送复位或下载控制命令。

## 分步调试项目

1. Windows 编译：检查新增类型、模块引用、命令行和 Lua 参数解析；确认原配置可以加载。
2. 配置入口：加载主配置中的 `config.serial_ports`，检查菜单中的连接名称、正确参数和无效参数提示；主配置指定 PowerShell 等默认 Shell 时，通过 `Alt+L` 新建串口标签页仍直接连接设备。
3. 接收日志：连接实际设备，检查中文、换行和连续输出；保持一段时间无输出，确认连接继续等待。
4. 输入命令：检查键盘输入、Enter、退格及粘贴与设备控制台是否匹配。
5. 关闭与重连：关闭窗格后重新打开同一端口，检查端口占用是否解除。
6. 异常情况：检查端口不存在、被其他程序占用、USB 转串口拔出以及重新插入后的手动连接。

每次只调试一项，记录实际结果后再进入下一项。
