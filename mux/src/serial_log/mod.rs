//! scnterm 扩展：每个串口窗格的日志生命周期与有界后台写入。

mod file;
#[cfg(test)]
mod tests;
mod text;

use crate::pane::PaneId;
use anyhow::Result;
use chrono::{DateTime, Local};
use config::SerialLogging;
use parking_lot::{Condvar, Mutex};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use text::TextLog;

const MAX_PENDING_BYTES: usize = 8 * 1024 * 1024;
const MAX_PENDING_MESSAGES: usize = 4096;
const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Starting,
    Recording,
    Stopping,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub path: Option<PathBuf>,
    pub error: Option<String>,
    pub can_start: bool,
}

enum Message {
    Start(SerialLogging),
    Data(Vec<u8>, DateTime<Local>),
    Stop,
    Fail(String),
}

struct State {
    phase: Phase,
    busy: bool,
    closed: bool,
    finished: bool,
    path: Option<PathBuf>,
    error: Option<String>,
    notice: Option<String>,
    pending: VecDeque<Message>,
    pending_bytes: usize,
    closing_activity: Option<crate::activity::Activity>,
}

struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    pane_id: PaneId,
    port: String,
}

pub struct SerialLog {
    shared: Arc<Shared>,
}

impl SerialLog {
    pub fn new(pane_id: PaneId, port: String) -> Arc<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                phase: Phase::Stopped,
                busy: false,
                closed: false,
                finished: false,
                path: None,
                error: None,
                notice: None,
                pending: VecDeque::new(),
                pending_bytes: 0,
                closing_activity: None,
            }),
            changed: Condvar::new(),
            pane_id,
            port,
        });
        let worker_shared = Arc::clone(&shared);
        if let Err(error) = std::thread::Builder::new()
            .name(format!("serial-log-{pane_id}"))
            .spawn(move || worker(worker_shared))
        {
            shared.fail(format!("无法启动日志写入任务：{error}"));
            let mut state = shared.state.lock();
            state.closed = true;
            state.finished = true;
        }
        Arc::new(Self { shared })
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = self.shared.state.lock();
        Snapshot {
            phase: state.phase,
            path: state.path.clone(),
            error: state.error.clone(),
            can_start: !state.closed && !state.busy,
        }
    }

    pub fn take_error_notice(&self) -> Option<String> {
        self.shared.state.lock().notice.take()
    }

    pub fn start(&self, mut options: SerialLogging) -> Result<()> {
        options.max_file_bytes()?;
        if options.directory.is_none() {
            options.resolve_directory(None);
        }
        {
            let mut state = self.shared.state.lock();
            anyhow::ensure!(!state.closed, "串口连接已关闭，无法开始记录");
            // 菜单打开后状态可能变化，重复开始不创建第二个写入任务。
            if state.busy {
                return Ok(());
            }
            state.busy = true;
            state.phase = Phase::Starting;
            state.error = None;
            state.path = None;
            state.pending.push_back(Message::Start(options));
        }
        self.shared.changed.notify_one();
        self.shared.notify();
        Ok(())
    }

    pub fn stop(&self) {
        {
            let mut state = self.shared.state.lock();
            if !matches!(state.phase, Phase::Starting | Phase::Recording) {
                return;
            }
            state.phase = Phase::Stopping;
            state.pending.push_back(Message::Stop);
        }
        self.shared.changed.notify_one();
        self.shared.notify();
    }

    pub fn receive(&self, bytes: &[u8], received: DateTime<Local>) {
        let mut overflow = false;
        {
            let mut state = self.shared.state.lock();
            if state.closed || !matches!(state.phase, Phase::Starting | Phase::Recording) {
                return;
            }
            if state.pending_bytes.saturating_add(bytes.len()) > MAX_PENDING_BYTES
                || state.pending.len() >= MAX_PENDING_MESSAGES
            {
                let reason = "待保存数据超过缓冲限制，日志记录已停止；串口仍可使用".to_string();
                state.phase = Phase::Failed;
                state.error = Some(reason.clone());
                state.notice = Some(reason.clone());
                state.pending.push_back(Message::Fail(reason));
                overflow = true;
            } else {
                state.pending_bytes += bytes.len();
                state
                    .pending
                    .push_back(Message::Data(bytes.to_vec(), received));
            }
        }
        self.shared.changed.notify_one();
        if overflow {
            self.shared.notify();
        }
    }

    /// 只发出关闭请求；磁盘写入和排空在后台完成。
    pub fn close(&self) {
        {
            let mut state = self.shared.state.lock();
            if state.closed {
                return;
            }
            state.closed = true;
            if matches!(state.phase, Phase::Starting | Phase::Recording) {
                state.phase = Phase::Stopping;
            }
            // 最后一个窗格关闭时，沿用已有 Activity 机制等待日志排空。
            if state.busy && crate::Mux::try_get().is_some() {
                state.closing_activity = Some(crate::activity::Activity::new());
            }
        }
        self.shared.changed.notify_one();
    }

    pub fn wait_closed(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.state.lock();
        while !state.finished {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            self.shared.changed.wait_for(&mut state, remaining);
        }
        true
    }
}

impl Drop for SerialLog {
    fn drop(&mut self) {
        self.close();
    }
}

impl Shared {
    fn notify(&self) {
        if crate::Mux::try_get().is_some() {
            crate::Mux::notify_from_any_thread(crate::MuxNotification::PaneOutput(self.pane_id));
        }
    }

    fn fail(&self, reason: String) {
        {
            let mut state = self.state.lock();
            state.phase = Phase::Failed;
            state.busy = false;
            state.error = Some(reason.clone());
            state.notice = Some(reason.clone());
            state.pending.clear();
            state.pending_bytes = 0;
        }
        log::error!("串口日志已停止，窗格 {}：{}", self.pane_id, reason);
        self.notify();
    }

    fn stopped(&self) {
        {
            let mut state = self.state.lock();
            state.busy = false;
            if state.phase != Phase::Failed {
                state.phase = Phase::Stopped;
            }
        }
        self.notify();
    }
}

fn worker(shared: Arc<Shared>) {
    let mut log: Option<TextLog> = None;
    let mut last_flush = Instant::now();
    loop {
        let message = {
            let mut state = shared.state.lock();
            if state.pending.is_empty() && !state.closed {
                shared.changed.wait_for(&mut state, FLUSH_INTERVAL);
            }
            let message = state.pending.pop_front();
            if let Some(Message::Data(bytes, _)) = &message {
                state.pending_bytes -= bytes.len();
            }
            if message.is_none() && state.closed {
                break;
            }
            message
        };
        let result: Result<()> = (|| {
            match message {
                Some(Message::Start(options)) => {
                    let directory = options
                        .directory
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("未设置日志目录"))?;
                    let file = file::LogFile::create(
                        directory,
                        &shared.port,
                        options.max_file_bytes()?,
                        options.timestamps,
                    )?;
                    let mut state = shared.state.lock();
                    state.path = Some(file.path().to_path_buf());
                    if state.phase == Phase::Starting {
                        state.phase = Phase::Recording;
                    }
                    drop(state);
                    log = Some(TextLog::new(file));
                    shared.notify();
                }
                Some(Message::Data(bytes, received)) => {
                    if let Some(log) = log.as_mut() {
                        log.receive(&bytes, received)?;
                        shared.state.lock().path = Some(log.path().to_path_buf());
                    }
                }
                Some(Message::Stop) => {
                    if let Some(mut log) = log.take() {
                        log.finish()?;
                    }
                    shared.stopped();
                }
                Some(Message::Fail(reason)) => {
                    if let Some(mut log) = log.take() {
                        log.finish()?;
                    }
                    shared.fail(reason);
                }
                None => {}
            }
            if last_flush.elapsed() >= FLUSH_INTERVAL {
                if let Some(log) = log.as_mut() {
                    log.flush()?;
                }
                last_flush = Instant::now();
            }
            Ok(())
        })();
        if let Err(error) = result {
            if let Some(mut log) = log.take() {
                let _ = log.finish();
            }
            shared.fail(format!("{error:#}"));
        }
    }
    if let Some(mut log) = log {
        if let Err(error) = log.finish() {
            shared.fail(format!("{error:#}"));
        }
    }
    shared.stopped();
    let activity = {
        let mut state = shared.state.lock();
        state.finished = true;
        state.closing_activity.take()
    };
    shared.changed.notify_all();
    drop(activity);
}
