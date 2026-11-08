//! scnterm 扩展：串口日志的菜单、快捷键、状态显示和错误提示。

use crate::commands::{ArgType, CommandDef};
use config::keyassignment::KeyAssignment;
use mux::pane::Pane;
use mux::serial_log::Phase;

pub fn menu_entry(pane: &dyn Pane) -> Option<(String, KeyAssignment)> {
    let log = pane.serial_log()?;
    let state = log.snapshot();
    match state.phase {
        Phase::Starting | Phase::Recording => {
            Some(("停止串口日志".into(), KeyAssignment::StopSerialLogging))
        }
        Phase::Stopped | Phase::Failed if state.can_start => {
            Some(("开始串口日志".into(), KeyAssignment::StartSerialLogging))
        }
        Phase::Stopping => Some(("串口日志正在停止".into(), KeyAssignment::Nop)),
        _ => None,
    }
}

pub fn perform(pane: &dyn Pane, start: bool) -> anyhow::Result<()> {
    if let Some(log) = pane.serial_log() {
        if start {
            log.start(config::configuration().serial_logging.clone())?;
        } else {
            log.stop();
        }
    }
    Ok(())
}

pub fn right_status(
    pane: Option<&dyn Pane>,
    existing: &str,
    config: &config::ConfigHandle,
    focused: bool,
) -> String {
    let log = match pane.and_then(Pane::serial_log) {
        Some(log) => log,
        None => return existing.to_string(),
    };
    let label = match log.snapshot().phase {
        Phase::Stopped => "未记录",
        Phase::Starting => "正在开始",
        Phase::Recording => "记录中",
        Phase::Stopping => "正在停止",
        Phase::Failed => "记录失败",
    };
    let bar_background = if config.use_fancy_tab_bar {
        if focused {
            config.window_frame.active_titlebar_bg
        } else {
            config.window_frame.inactive_titlebar_bg
        }
    } else {
        config
            .resolved_palette
            .tab_bar
            .clone()
            .unwrap_or_default()
            .background()
    };
    let (r, g, b, _) = bar_background.to_srgb_u8();
    let surround = format!("\x1b[0m\x1b[48;2;{r};{g};{b}m");

    // 使用已有半圆字形绘制胶囊两端，亮橘黄 #FFB52E 文字配深紫 #28183D 底色。
    // 内侧各留一个空格，外侧各留两个；外侧跟随标签栏底色，避免圆弧外出现矩形。
    // 追加用户 Lua 状态前恢复默认属性，避免日志配色影响后续文字。
    format!(
        "{surround}  \x1b[38;2;40;24;61m\u{e0b6}\x1b[48;2;40;24;61m\x1b[38;2;255;181;46m 串口日志：{label} {surround}\x1b[38;2;40;24;61m\u{e0b4}  \x1b[0m{existing}"
    )
}

pub fn notify_error(pane: Option<&dyn Pane>) {
    if let Some(log) = pane.and_then(Pane::serial_log) {
        if let Some(reason) = log.take_error_notice() {
            wezterm_toast_notification::persistent_toast_notification(
                "串口日志已停止",
                &format!("{reason}\n串口仍可使用。排除问题后可通过 Alt+L 重新开始记录。"),
            );
        }
    }
}

pub fn command_def(action: &KeyAssignment) -> Option<CommandDef> {
    let (brief, doc) = match action {
        KeyAssignment::StartSerialLogging => {
            ("开始串口日志", "为当前串口窗格新建日志文件并开始记录")
        }
        KeyAssignment::StopSerialLogging => {
            ("停止串口日志", "保存当前串口窗格的待写内容并停止记录")
        }
        _ => return None,
    };
    Some(CommandDef {
        brief: brief.into(),
        doc: doc.into(),
        keys: vec![],
        args: &[ArgType::ActivePane],
        menubar: &[],
        icon: None,
    })
}
