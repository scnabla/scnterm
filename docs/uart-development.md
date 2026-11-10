# Windows UART 开发与分步调试

## 第一阶段范围

串口用于查看设备日志和输入命令，输出接入现有终端窗格。连接参数保存在主 Lua 配置的 `config.serial_ports` 中，通过现有命令面板、启动菜单和 `wezterm connect` 入口连接。第一阶段不增加专用配置界面、HEX 收发和下载模式控制。

Windows Release 版本已通过 RK3568 实机收发验证，参数为 COM3、1500000、8N1、无流控。主配置指定 PowerShell 作为默认 Shell 时，通过 `Alt+L` 选择串口可在当前窗口新建串口标签页。输入 `uname -a` 后收到 Linux 4.19.232、aarch64 的系统信息，关闭后重新连接 COM3 可以继续收发。

## 文件分工

| 文件 | 职责 |
| --- | --- |
| `config/src/serial.rs` | Lua 参数定义、参数校验和串口设置转换 |
| `config/src/serial_logging.rs` | 日志公共配置、大小校验和保存目录解析 |
| `pty/src/serial.rs` | 设备打开、字节收发、连接结束和句柄释放 |
| `mux/src/serial_log/` | 日志状态、后台写入、文本处理、分文件及行为测试 |
| `wezterm-gui-subcommands/src/serial.rs` | 串口命令行参数 |
| `wezterm-gui/src/serial.rs` | 串口命令与终端窗口的连接 |
| `wezterm-gui/src/serial_logging.rs` | 日志启停动作、菜单文字、顶部状态及失败提示 |
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

## 串口关键词高亮

在主 Lua 配置中设置 `config.serial_highlighting`。所有串口共用规则，只改变匹配文字的前景色；串口收发、复制出的文本和日志文件保持原有内容。本地终端与 SSH 不应用此配置。

完整 Lua 示例见 [serial-highlighting.lua](examples/serial-highlighting.lua)。已有主配置时，只复制其中的 `config.serial_highlighting` 段到 `return config` 前，保留原有串口连接及其他设置。Windows 完整个人配置见 [配置仓库](https://github.com/scnabla/personal-config/tree/main/terminal/wezterm/windows)。

```lua
config.serial_highlighting = {
  rules = {
    {
      foreground = '#FF6B81',
      keywords = { '错误', '失败', '[故障]' },
      patterns = { [[\b(?:error|fail(?:ed|ure)?|fatal)\b]] },
    },
    {
      foreground = '#FFB52E',
      keywords = { '警告', '超时' },
      patterns = { [[\bwarn(?:ing)?\b]], [[\btime(?:d)?[ -]?out\b]] },
    },
  },
}
```

| 配置项 | 默认值与作用 |
| --- | --- |
| `rules` | 空列表；每组设置一种颜色及多个关键词或正则 |
| `rules[i].foreground` | 必填颜色，例如 `'#FFB52E'` |
| `rules[i].keywords` | 空列表；按原样匹配，`.`、`[` 等字符不作为正则符号 |
| `rules[i].patterns` | 空列表；使用 Rust `regex` 语法，支持分组、选择、重复、字符类及 Unicode，不支持回溯引用和前后查找 |
| `case_sensitive` | `false`，所有规则默认不区分大小写 |
| `rules[i].case_sensitive` | 省略时继承公共设置，可为该组单独设置 `true` 或 `false` |

`keywords` 与 `patterns` 可以同时设置，也可只设置其中一项。Lua 的 `[[...]]` 字符串保留反斜杠，适合书写正则；表达式含有连续的 `]]` 时，可改用 `[=[...]=]`。正则的整个匹配范围变色，捕获分组不会缩小着色范围。默认按子串匹配，需要英文单词边界时使用 `\b`。

同一段文字命中不同颜色时，列表靠前的规则优先；后续规则仍可给未被前面规则覆盖的部分着色。未命中文字保留设备原色，文字原有底色、粗体和下划线保持不变。搜索和快速选择模式沿用原有显示，普通选区与光标继续使用对应样式。

保存 Lua 后，默认的自动配置重载会更新当前画面和滚动历史的高亮，不重新连接设备。删除 `serial_highlighting` 或清空 `rules` 后恢复设备原色；没有额外的开关或菜单项。正则或颜色错误会指出配置位置，重载失败保留上一次成功加载的配置，已有串口继续收发。

匹配基于终端解析后的文字，支持分批接收、UTF-8 中文和窗口自动折行，不跨设备输出的换行。回车覆盖、退格或清行改变内容后重新匹配。正则中的 `^`、`$` 对应当前解析后的逻辑行边界，行尾空格参与匹配；尚未结束的一行也实时显示匹配结果。

每份配置最多 32 组规则，关键词与正则合计最多 128 条，每条为 1 至 4096 字节的非空文本。正则编译有内存上限，超出时提示对应表达式。超过 64 KiB 的逻辑行，以及无法在限定范围内取得完整上下文的逻辑行，保留设备原色。匹配缓存的文本和规则编号合计不超过 2 MiB，最多保存 512 项。

### 高亮代码与上游接入

| 文件 | 职责 |
| --- | --- |
| `config/src/serial_highlighting.rs` | 主 Lua 配置定义、颜色校验、正则编译及配置测试 |
| `wezterm-gui/src/serial_highlighting.rs` | 读取可见文字及折行上下文、匹配缓存、按优先级给绘制副本着色 |
| `config/src/config.rs`、`config/src/lib.rs` | 注册公共配置，沿用配置加载及错误处理 |
| `mux/src/pane.rs`、`mux/src/localpane.rs` | 提供串口类型判断，独立于日志状态 |
| `wezterm-gui/src/main.rs`、`wezterm-gui/src/termwindow/mod.rs` | 注册显示模块及窗口级缓存 |
| `wezterm-gui/src/termwindow/render/pane.rs` | 将串口绘制副本交给原有渲染流程，保留其他窗格行为 |

## 串口日志配置

日志只保存从设备接收的输出，设备回显的命令随输出保存。文件采用 UTF-8 文本，去除颜色、标题等控制指令，单行进度条的覆盖更新分别保存为多行。关键词高亮不参与日志处理。

在主 Lua 配置中添加公共设置，并在需要自动记录的串口连接中设置 `log_on_connect = true`：

```lua
config.serial_logging = {
  max_file_size_mb = 100,
  timestamps = true,
  -- directory = 'serial-logs',
}

config.serial_ports = {
  {
    name = 'board-console',
    port = 'COM3',
    baud = 115200,
    data_bits = 8,
    parity = 'None',
    stop_bits = 1,
    flow_control = 'None',
    log_on_connect = true,
  },
}
```

已有 `config.serial_ports` 时，在对应连接中增加字段，保留其他连接和串口参数。

| 配置项 | 默认值与作用 |
| --- | --- |
| `serial_logging.directory` | 省略时使用实际加载的主 Lua 文件所在目录；相对路径以该目录为基准。所有串口共用此设置，目录不存在时自动创建 |
| `serial_logging.max_file_size_mb` | `100`，按 100000000 字节计算；必须为正整数且换算后不溢出 |
| `serial_logging.timestamps` | `true`，每行添加电脑本地接收时间，格式为 `[20261108-22:36:11.895]`，保留毫秒，不显示时区；不改动设备自带时间信息 |
| `serial_ports` 中的 `log_on_connect` | `false`；设置为 `true` 后，该串口连接时自动开始记录 |

旧配置省略新增字段时，不会自动开始记录。直接使用 `wezterm serial` 建立的连接也默认不记录，可通过菜单手动开始。

重载配置后，目录、大小和时间戳设置用于下一次开始记录；已有记录继续使用开始时的设置。`log_on_connect` 的修改用于后续新连接。

## 日志操作与文件

在串口窗格中按 `Alt+L`，通过日志菜单项开始或停止记录。操作只影响打开菜单时的串口窗格。本地终端和 SSH 窗格不显示串口日志菜单项。

顶部标签栏右侧显示当前串口窗格的日志状态，包括记录中、未记录和记录失败；开始与停止期间显示对应的处理中状态。切换窗格时状态随之更新，已有 Lua 状态文字继续显示。隐藏标签栏时，可通过 `Alt+L` 查看对应操作。

日志状态使用亮橘黄色文字（`#FFB52E`）和深紫色胶囊底色（`#28183D`），两端为圆弧。文字左右各留一个空格，胶囊外侧各留两个空格，与相邻状态和窗口边缘保持间距。

可在已有的 `config.keys` 中添加自选快捷键，动作名称为 `wezterm.action.StartSerialLogging` 和 `wezterm.action.StopSerialLogging`。未设置默认日志快捷键。

每次开始记录新建文件，文件名包含端口名称和本次开始时间。例如：

```text
COM3_20261108_143001_123.log
COM3_20261108_143001_123-1.log
COM3_20261108_143001_123-2.log
```

达到大小阈值后，在当前行写完时切换文件，单个文件可略大于阈值；同一次记录的开始时间保持不变，分文件序号依次增加。停止后再次开始使用新的开始时间，不追加上一份文件。同毫秒内重开导致文件名冲突时，开始时间提高到纳秒格式；创建文件始终禁止覆盖已有文件。旧日志由用户手动删除。

后台定期刷新未换行的提示符，刷新间隔设为 250 毫秒，后续内容按增量追加。已写入的行随后被退格或覆盖修改时，保留原记录并另写修改后的内容。全屏程序的光标上下移动不作为屏幕回放保存。

后台待写数据限制为 8 MiB，最多积压 4096 批数据。文件创建或写入失败、积压超限时停止记录并提示，串口继续收发；排除问题后通过菜单重新开始。停止期间的内容不补录。单行内容、组合字符和未结束控制指令也设有长度限制，超过限制会停止日志记录并提示。

## 日志与上游代码的接入

日志主要逻辑位于独立模块，上游文件保留以下接入：

| 接入文件 | 接入原因与行为 |
| --- | --- |
| `config/src/config.rs`、`config/src/lib.rs` | 注册公共日志配置，并在加载主配置时确定保存目录 |
| `config/src/serial.rs` | 为每个串口保存自动记录开关 |
| `mux/src/domain.rs` | 创建串口窗格时附加独立日志控制器，保持其他连接的创建流程 |
| `mux/src/pane.rs`、`mux/src/localpane.rs` | 提供日志状态与关闭通知，其他窗格默认不提供串口日志控制器 |
| `mux/src/lib.rs` | 在终端解析之前复制接收数据；接收结束和程序退出时通知日志排空 |
| `config/src/keyassignment.rs`、`wezterm-gui/src/commands.rs` | 注册 Lua 动作并读取独立界面模块提供的动作说明 |
| `wezterm-gui/src/overlay/launcher.rs` | 将当前串口的日志操作加入已有启动菜单 |
| `wezterm-gui/src/termwindow/mod.rs` | 转发操作、合成顶部状态、显示错误，文件写入不在 GUI 线程执行 |

串口驱动不依赖日志模块，日志处理不改变终端解析和实际发送的数据。

## 日志验证记录

2026-11-08，Linux 和 Windows 均完成 Release 构建。Windows 交付版本为 `20261108-serial-log-local`。

| 环境 | 验证内容与结果 |
| --- | --- |
| Linux 自动测试 | 14 项 mux 测试及 2 项日志配置测试通过，覆盖文本解析、分文件、同名文件保护、启停、关闭排空、创建失败恢复及缓冲超限 |
| Windows 自动测试 | 10 项日志测试及 2 项配置测试通过 |
| Linux GUI 与模拟串口 | 连续接收 6000 行，按 1 MB 阈值分文件，逐行检查无遗漏、重复和乱序；中文、进度条、未换行提示符、停止后重开均通过 |
| Linux 异常与多串口 | 日志目录创建失败后串口仍可收发；修复目录后可重新记录；两个串口窗格独立启停；未回显的发送内容不写日志；重连新建文件 |
| Windows COM3 实机 | 使用完整主配置副本，通过 `Alt+L` 连接 RK3568，参数为 1500000、8N1、无流控；`uname -a` 的设备返回内容写入日志，颜色控制码被去除，进度条各次更新分别保留 |
| Windows 菜单与状态 | 自动记录、手动停止、重新开始及新建文件通过；停止期间的输出不补录；顶部状态保留 Lua 状态文字，切回本地标签页后隐藏串口状态；标签页导航保持原有条目 |
| Windows 本地终端 | PowerShell 启动、分屏及命令输入通过 |
| Windows 独立测试桌面 | 从交付目录启动程序，重新连接 COM3、发送命令并核对新日志文件通过；测试输入限定到该桌面的测试窗口 |

### 时间格式与状态样式验证

2026-11-08，Windows 版本 `20261108-serial-status-local` 包含新的日志时间格式和胶囊状态样式。

| 环境 | 验证内容与结果 |
| --- | --- |
| Windows 构建与自动测试 | Release 构建成功，10 项串口日志测试与 2 项日志配置测试通过；接收时间测试核对完整输出 `[20261108-22:36:11.895] timestamp` |
| Windows 配置与独立桌面 | 个人主 Lua 配置加载通过；在独立测试桌面启动新版，PowerShell 命令输出及分屏后的默认终端启动通过 |
| Linux 界面与模拟串口 | 图形标签栏和字符标签栏均显示亮橘黄色文字、深紫色胶囊及两端圆弧；文字两侧和窗口右侧留白正常，已有 Lua 状态保留自己的颜色，切回本地窗格后隐藏日志状态；保存的日志时间格式符合要求 |

### 关键词高亮验证

2026-11-10，Linux Release 构建通过。13 项配置测试、6 项高亮显示测试及 10 项串口日志回归测试通过，其中新增 3 项高亮配置测试和 6 项高亮显示测试。

高亮测试覆盖普通文本与正则、大小写设置、无效颜色和表达式、中文与 ANSI 逐字节接收、组合字符、重叠优先级、自动折行、窗口宽度变化、跨可见区域的长行、回车覆盖、缓存与长度限制，以及规则修改和清空后恢复原色。

独立 Linux 显示环境使用完整个人配置的测试副本连接模拟串口，验证玫红与橘黄色高亮、热重载更换颜色、清空规则恢复设备蓝色、本地终端保持原色，以及错误配置保留上一次有效规则。连续 2000 行输出进入终端并完整保存到日志，复制出的文字和导出的原始颜色序列未被高亮改写。

Windows 版本 `20261110-serial-highlight-local` 的 Release 构建通过；13 项配置测试、6 项高亮显示测试和 10 项串口日志回归测试通过。

Windows 独立测试桌面使用主 Lua 的测试副本启动程序，PowerShell 命令输入与分屏通过。COM3 以 1500000、8N1、无流控连接，设备执行 `printf` 后返回预期的错误词与警告词文本，日志内容一致；关闭测试窗格后释放串口，个人主 Lua 文件哈希保持不变。
