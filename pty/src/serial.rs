//! This module implements a serial port based tty.
//! This is a bit different from the other implementations in that
//! we cannot explicitly spawn a process into the serial connection,
//! so we can only use a `CommandBuilder::new_default_prog` with the
//! `openpty` method.
//! On most (all?) systems, attempting to open multiple instances of
//! the same serial port will fail.
use crate::{
    Child, ChildKiller, CommandBuilder, ExitStatus, MasterPty, PtyPair, PtySize, PtySystem,
    SlavePty,
};
use anyhow::{ensure, Context};
use serial2::SerialPort;
pub use serial2::{CharSize, FlowControl, Parity, StopBits};
use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Result as IoResult, Write};
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

type Handle = Arc<Connection>;

/// 串口没有子进程，以明确关闭或收发错误结束会话；空闲不结束会话。
struct Connection {
    port: Mutex<Option<SerialPort>>,
    exit_code: Mutex<Option<u32>>,
    exited: Condvar,
}

impl Connection {
    fn finish(&self, code: u32) {
        // 保留首次退出原因，并释放主句柄。读取句柄在读循环退出时释放。
        let mut exit_code = self.exit_code.lock().unwrap();
        if exit_code.is_none() {
            self.port.lock().unwrap().take();
            *exit_code = Some(code);
            self.exited.notify_all();
        }
    }

    fn has_exited(&self) -> bool {
        self.exit_code.lock().unwrap().is_some()
    }

    fn io_result<T>(&self, result: IoResult<T>) -> IoResult<T> {
        if let Err(error) = &result {
            if !matches!(
                error.kind(),
                std::io::ErrorKind::Interrupted
                    | std::io::ErrorKind::WouldBlock
                    | std::io::ErrorKind::TimedOut
            ) {
                self.finish(1);
            }
        }
        result
    }
}

fn closed_port_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotConnected, "串口连接已关闭")
}

pub struct SerialTty {
    port: OsString,
    baud: u32,
    char_size: CharSize,
    parity: Parity,
    stop_bits: StopBits,
    flow_control: FlowControl,
}

impl SerialTty {
    pub fn new<T: AsRef<OsStr> + ?Sized>(port: &T) -> Self {
        Self {
            port: port.as_ref().to_owned(),
            baud: 9600,
            char_size: CharSize::Bits8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            flow_control: FlowControl::XonXoff,
        }
    }

    pub fn set_baud_rate(&mut self, baud: u32) {
        self.baud = baud;
    }

    pub fn set_char_size(&mut self, char_size: CharSize) {
        self.char_size = char_size;
    }

    pub fn set_parity(&mut self, parity: Parity) {
        self.parity = parity;
    }

    pub fn set_stop_bits(&mut self, stop_bits: StopBits) {
        self.stop_bits = stop_bits;
    }

    pub fn set_flow_control(&mut self, flow_control: FlowControl) {
        self.flow_control = flow_control;
    }
}

impl PtySystem for SerialTty {
    fn openpty(&self, _size: PtySize) -> anyhow::Result<PtyPair> {
        let mut port = SerialPort::open(&self.port, self.baud)
            .with_context(|| format!("无法打开串口 {:?}", self.port))?;

        let mut settings = port.get_configuration()?;
        settings.set_raw();
        settings.set_baud_rate(self.baud)?;
        settings.set_char_size(self.char_size);
        settings.set_flow_control(self.flow_control);
        settings.set_parity(self.parity);
        settings.set_stop_bits(self.stop_bits);
        port.set_configuration(&settings)
            .with_context(|| format!("无法设置串口 {:?} 的通信参数", self.port))?;
        log::debug!("serial settings: {:#?}", port.get_configuration());

        // The timeout needs to be rather short because, at least on Windows,
        // a read with a long timeout will block a concurrent write from
        // happening.  In wezterm we tend to have a thread looping on read
        // while writes happen occasionally from the gui thread, and if we
        // make this timeout too long we can block the gui thread.
        port.set_read_timeout(Duration::from_millis(50))?;
        port.set_write_timeout(Duration::from_millis(50))?;

        let connection = Arc::new(Connection {
            port: Mutex::new(Some(port)),
            exit_code: Mutex::new(None),
            exited: Condvar::new(),
        });

        Ok(PtyPair {
            slave: Box::new(Slave {
                connection: Arc::clone(&connection),
            }),
            master: Box::new(SerialMaster {
                connection,
                took_writer: RefCell::new(false),
            }),
        })
    }
}

struct Slave {
    connection: Handle,
}

impl SlavePty for Slave {
    fn spawn_command(&self, cmd: CommandBuilder) -> anyhow::Result<Box<dyn Child + Send + Sync>> {
        ensure!(
            cmd.is_default_prog(),
            "can only use default prog commands with serial tty implementations"
        );
        Ok(Box::new(SerialChild {
            connection: Arc::clone(&self.connection),
        }))
    }
}

/// 将串口连接状态接入已有的终端会话生命周期。
struct SerialChild {
    connection: Handle,
}

// An anemic impl of Debug to satisfy some indirect trait bounds
impl std::fmt::Debug for SerialChild {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        fmt.debug_struct("SerialChild").finish()
    }
}

impl Child for SerialChild {
    fn try_wait(&mut self) -> IoResult<Option<ExitStatus>> {
        Ok(self
            .connection
            .exit_code
            .lock()
            .unwrap()
            .map(ExitStatus::with_exit_code))
    }

    fn wait(&mut self) -> IoResult<ExitStatus> {
        let mut exit_code = self.connection.exit_code.lock().unwrap();
        while exit_code.is_none() {
            exit_code = self.connection.exited.wait(exit_code).unwrap();
        }
        Ok(ExitStatus::with_exit_code(exit_code.unwrap()))
    }

    fn process_id(&self) -> Option<u32> {
        None
    }

    #[cfg(windows)]
    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        None
    }
}

impl ChildKiller for SerialChild {
    fn kill(&mut self) -> IoResult<()> {
        self.connection.finish(0);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(SerialChildKiller {
            connection: Arc::clone(&self.connection),
        })
    }
}

struct SerialChildKiller {
    connection: Handle,
}

impl std::fmt::Debug for SerialChildKiller {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        fmt.debug_struct("SerialChildKiller").finish()
    }
}

impl ChildKiller for SerialChildKiller {
    fn kill(&mut self) -> IoResult<()> {
        self.connection.finish(0);
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(SerialChildKiller {
            connection: Arc::clone(&self.connection),
        })
    }
}

pub struct SerialMaster {
    connection: Handle,
    took_writer: RefCell<bool>,
}

struct MasterWriter {
    connection: Handle,
}

impl Write for MasterWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, std::io::Error> {
        let result = {
            let port = self.connection.port.lock().unwrap();
            port.as_ref().ok_or_else(closed_port_error)?.write(buf)
        };
        self.connection.io_result(result)
    }

    fn flush(&mut self) -> Result<(), std::io::Error> {
        let result = {
            let port = self.connection.port.lock().unwrap();
            port.as_ref().ok_or_else(closed_port_error)?.flush()
        };
        self.connection.io_result(result)
    }
}

impl Drop for SerialMaster {
    fn drop(&mut self) {
        self.connection.finish(0);
    }
}

impl MasterPty for SerialMaster {
    fn resize(&self, _size: PtySize) -> anyhow::Result<()> {
        // Serial ports have no concept of size
        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        // Serial ports have no concept of size
        Ok(PtySize::default())
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn std::io::Read + Send>> {
        let port = self.connection.port.lock().unwrap();
        let port = port.as_ref().ok_or_else(closed_port_error)?;
        let reader_port = port.try_clone()?;
        Ok(Box::new(Reader {
            port: Some(reader_port),
            connection: Arc::clone(&self.connection),
        }))
    }

    fn take_writer(&self) -> anyhow::Result<Box<dyn std::io::Write + Send>> {
        if *self.took_writer.borrow() {
            anyhow::bail!("cannot take writer more than once");
        }
        *self.took_writer.borrow_mut() = true;
        if self.connection.has_exited() {
            return Err(closed_port_error().into());
        }
        Ok(Box::new(MasterWriter {
            connection: Arc::clone(&self.connection),
        }))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        // N/A: there is no local process
        None
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<crate::unix::RawFd> {
        None
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<PathBuf> {
        None
    }
}

struct Reader {
    port: Option<SerialPort>,
    connection: Handle,
}

impl Drop for Reader {
    fn drop(&mut self) {
        self.port.take();
        self.connection.finish(0);
    }
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        // serial2 同时处理 Windows 异步句柄和 Unix 读取超时。
        // 使用串口接口读取，不能将 Windows 串口句柄当普通文件同步读取。
        loop {
            if self.connection.has_exited() {
                self.port.take();
                return Ok(0);
            }
            let port = match self.port.as_ref() {
                Some(port) => port,
                None => return Ok(0),
            };
            match port.read(buf) {
                Ok(0) => {
                    self.connection.finish(1);
                    self.port.take();
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "EOF on serial port",
                    ));
                }
                Ok(size) => {
                    return Ok(size);
                }
                Err(e) => {
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) {
                        continue;
                    }
                    log::error!("serial read error: {}", e);
                    self.connection.finish(1);
                    self.port.take();
                    return Err(e);
                }
            }
        }
    }
}
