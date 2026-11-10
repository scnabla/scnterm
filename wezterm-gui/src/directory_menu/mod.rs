//! ScnTerm 扩展：SSH 常用目录菜单，在覆盖层线程中读取路径、保存列表及处理按键。

mod storage;

use anyhow::{ensure, Context};
use mux::domain::Domain;
use mux::pane::Pane;
use mux::ssh::RemoteSshDomain;
use mux::termwiztermtab::TermWizTerminal;
use std::sync::Arc;
use std::time::{Duration, Instant};
use storage::{cd_command, connection_key, validate_path, Store};
use termwiz::cell::{AttributeChange, Intensity};
use termwiz::color::ColorAttribute;
use termwiz::input::{InputEvent, KeyCode, KeyEvent, Modifiers};
use termwiz::surface::{Change, CursorVisibility, Position};
use termwiz::terminal::Terminal;

const REPLY_VAR: &str = "SCNTERM_DIRECTORY_REPLY";

fn readable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn render(
    term: &mut TermWizTerminal,
    title: &str,
    paths: &[String],
    selected: usize,
    notice: &str,
) -> anyhow::Result<()> {
    let size = term.get_screen_size()?;
    let width = size.cols.saturating_sub(2);
    let count = size.rows.saturating_sub(6).max(1);
    let first = selected.saturating_sub(count - 1);
    let orange = ColorAttribute::TrueColorWithDefaultFallback((255, 181, 46).into());
    let mut changes = vec![
        Change::ClearScreen(ColorAttribute::Default),
        Change::CursorPosition {
            x: Position::Absolute(0),
            y: Position::Absolute(0),
        },
        Change::CursorVisibility(CursorVisibility::Hidden),
    ];
    for text in [title, "↑↓ 选择  Enter 进入/添加  - 删除记录  Esc 关闭", ""] {
        changes.push(Change::Text(format!(
            "{}\r\n",
            termwiz_funcs::truncate_right(&readable(text), width)
        )));
    }
    for index in first..=(paths.len().min(first + count - 1)) {
        let chosen = index == selected;
        changes.push(
            AttributeChange::Foreground(if chosen {
                orange
            } else {
                ColorAttribute::Default
            })
            .into(),
        );
        changes.push(
            AttributeChange::Intensity(if chosen {
                Intensity::Bold
            } else {
                Intensity::Normal
            })
            .into(),
        );
        let label = if index == 0 {
            "添加当前目录"
        } else {
            &paths[index - 1]
        };
        let line = format!("{} {}", if chosen { "▶" } else { " " }, label);
        changes.push(Change::Text(format!(
            "{}\r\n",
            termwiz_funcs::truncate_right(&line, width)
        )));
    }
    changes.push(AttributeChange::Foreground(ColorAttribute::Default).into());
    changes.push(AttributeChange::Intensity(Intensity::Normal).into());
    changes.push(Change::Text(format!(
        "\r\n{}\r\n",
        termwiz_funcs::truncate_right(&readable(notice), width)
    )));
    term.render(&changes)?;
    term.flush()?;
    Ok(())
}

fn wait_close(term: &mut TermWizTerminal, message: &str) -> anyhow::Result<()> {
    loop {
        let width = term.get_screen_size()?.cols.saturating_sub(2);
        term.render(&[
            Change::ClearScreen(ColorAttribute::Default),
            Change::CursorPosition {
                x: Position::Absolute(0),
                y: Position::Absolute(0),
            },
            Change::CursorVisibility(CursorVisibility::Hidden),
            Change::Text(format!(
                "常用目录\r\n\r\n{}\r\n\r\nEnter / Esc 关闭",
                termwiz_funcs::truncate_right(&readable(message), width)
            )),
        ])?;
        term.flush()?;
        match term.poll_input(None)? {
            None
            | Some(InputEvent::Key(KeyEvent {
                key: KeyCode::Escape | KeyCode::Enter,
                ..
            })) => return Ok(()),
            _ => {}
        }
    }
}

fn check_pane(pane: &dyn Pane) -> anyhow::Result<()> {
    ensure!(!pane.is_dead(), "SSH 连接已关闭");
    ensure!(
        !pane.is_alt_screen_active(),
        "请先退出当前全屏程序，回到命令提示符"
    );
    Ok(())
}

fn query_command(token: &str) -> String {
    // 使用同一个远端 Shell 的 PWD，不能用另一个 SSH 会话的工作目录代替。
    // printf 的固定格式与双引号保留路径中的空格和符号；base64 避免控制序列截断。
    format!(
        " printf '\\033]1337;SetUserVar={REPLY_VAR}=%s\\007' \"$(printf '%s:%s' '{token}' \"$PWD\" | base64 | tr -d '\\r\\n')\"\r"
    )
}

fn matching_reply(value: &str, token: &str) -> anyhow::Result<Option<String>> {
    match value.strip_prefix(&format!("{token}:")) {
        Some(path) => {
            validate_path(path)?;
            Ok(Some(path.to_owned()))
        }
        None => Ok(None),
    }
}

fn current_directory(
    term: &mut TermWizTerminal,
    pane: &dyn Pane,
) -> anyhow::Result<Option<String>> {
    check_pane(pane)?;
    let token = format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..));
    pane.writer()
        .write_all(query_command(&token).as_bytes())
        .context("无法向 SSH 会话发送路径查询")?;
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        check_pane(pane)?;
        if let Some(value) = pane.copy_user_vars().get(REPLY_VAR) {
            if let Some(path) = matching_reply(value, &token)? {
                return Ok(Some(path));
            }
        }
        if let Some(InputEvent::Key(KeyEvent {
            key: KeyCode::Escape,
            ..
        })) = term.poll_input(Some(Duration::from_millis(50)))?
        {
            return Ok(None);
        }
    }
    anyhow::bail!("未能读取当前目录，请回到空白命令提示符后重试")
}

pub fn run(
    mut term: TermWizTerminal,
    pane: Arc<dyn Pane>,
    domain: Option<Arc<dyn Domain>>,
    bookmarks_file: std::path::PathBuf,
) -> anyhow::Result<()> {
    term.set_raw_mode()?;
    term.render(&[Change::Title("常用目录".into())])?;
    let setup = (|| -> anyhow::Result<(String, Store)> {
        let domain = domain.context("请在内置 SSH 连接中打开目录菜单")?;
        let ssh = domain
            .downcast_ref::<RemoteSshDomain>()
            .context("请在内置 SSH 连接中打开目录菜单")?;
        let options = ssh.ssh_config().context("无法读取当前 SSH 连接配置")?;
        let host = options.get("hostname").context("SSH 连接缺少主机名")?;
        let user = options.get("user").context("SSH 连接缺少用户名")?;
        let port = options.get("port").map(String::as_str).unwrap_or("22");
        let key = connection_key(domain.domain_name(), host, port, user)?;
        let store = Store::new(bookmarks_file, key);
        Ok((
            format!("常用目录 · {}", readable(domain.domain_name())),
            store,
        ))
    })();
    let (title, store) = match setup {
        Ok(result) => result,
        Err(err) => return wait_close(&mut term, &format!("{err:#}")),
    };
    let mut paths = match store.list() {
        Ok(paths) => paths,
        Err(err) => return wait_close(&mut term, &format!("{err:#}")),
    };
    let mut selected = if paths.is_empty() { 0 } else { 1 };
    let mut notice = "请在空白命令提示符下使用；删除记录不会删除服务器目录".to_owned();
    loop {
        render(&mut term, &title, &paths, selected, &notice)?;
        let event = match term.poll_input(None)? {
            Some(event) => event,
            None => return Ok(()),
        };
        match event {
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                ..
            }) => return Ok(()),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('c' | 'C'),
                modifiers: Modifiers::CTRL,
            }) => return Ok(()),
            InputEvent::Key(KeyEvent {
                key: KeyCode::UpArrow,
                ..
            }) => selected = selected.saturating_sub(1),
            InputEvent::Key(KeyEvent {
                key: KeyCode::DownArrow,
                ..
            }) => selected = (selected + 1).min(paths.len()),
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('-'),
                modifiers: Modifiers::NONE,
            }) => {
                if selected == 0 {
                    notice = "请选择要删除的目录记录".into();
                    continue;
                }
                match store.update(&paths[selected - 1], false) {
                    Ok(updated) => {
                        paths = updated;
                        selected = selected.min(paths.len());
                        notice = "已删除目录记录".into();
                    }
                    Err(err) => notice = format!("{err:#}"),
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) if selected == 0 => {
                render(
                    &mut term,
                    &title,
                    &paths,
                    selected,
                    "正在读取当前目录，Esc 关闭",
                )?;
                match current_directory(&mut term, &*pane) {
                    Ok(None) => return Ok(()),
                    Ok(Some(path)) => match store.update(&path, true) {
                        Ok(updated) => {
                            let existed = paths.contains(&path);
                            paths = updated;
                            selected = paths.iter().position(|p| p == &path).map_or(0, |i| i + 1);
                            notice = if existed {
                                "该目录已在列表中"
                            } else {
                                "已添加当前目录"
                            }
                            .into();
                        }
                        Err(err) => notice = format!("{err:#}"),
                    },
                    Err(err) => notice = format!("{err:#}"),
                }
            }
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                ..
            }) => {
                let result = check_pane(&*pane).and_then(|_| {
                    pane.writer()
                        .write_all(cd_command(&paths[selected - 1])?.as_bytes())
                        .context("无法向 SSH 会话发送切换目录命令")
                });
                match result {
                    Ok(()) => return Ok(()),
                    Err(err) => notice = format!("{err:#}"),
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_menu_rejects_stale_or_invalid_replies() {
        assert_eq!(matching_reply("old:/stale", "new").unwrap(), None);
        assert_eq!(
            matching_reply("new:/中文 a'b", "new").unwrap(),
            Some("/中文 a'b".into())
        );
        assert!(matching_reply("new:/bad\npath", "new").is_err());
        assert!(query_command("0123").contains("\"$PWD\""));
        assert_eq!(readable("server\x1b\nname"), "server  name");
    }
}
