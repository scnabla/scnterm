//! scnterm 扩展：串口文本日志文件创建、追加与按大小分文件。

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub(super) struct LogFile {
    directory: PathBuf,
    stem: String,
    part: u64,
    file: BufWriter<File>,
    path: PathBuf,
    bytes: u64,
    limit: u64,
    timestamps: bool,
    line_open: bool,
    written_text: String,
}

impl LogFile {
    pub fn create(directory: &Path, port: &str, limit: u64, timestamps: bool) -> Result<Self> {
        anyhow::ensure!(limit > 0, "日志分文件大小必须大于零");
        std::fs::create_dir_all(directory)
            .with_context(|| format!("无法创建日志目录 {}", directory.display()))?;
        let port = port.rsplit(['/', '\\']).next().unwrap_or(port);
        let port: String = port
            .chars()
            .take(40)
            .map(|c| {
                if c.is_control() || "<>:\"/\\|?*".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let port = if port.is_empty() { "serial" } else { &port };
        let started = Local::now();
        let stem = format!("{}_{}", port, started.format("%Y%m%d_%H%M%S_%3f"));
        let path = directory.join(format!("{stem}.log"));
        let (stem, path, file) = match Self::open_new(&path) {
            Ok(file) => (stem, path, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // 同毫秒多次开始时提高时间精度，不借用分文件序号，也不覆盖旧文件。
                let stem = format!("{}_{}", port, started.format("%Y%m%d_%H%M%S_%9f"));
                let path = directory.join(format!("{stem}.log"));
                let file = Self::open_new(&path)
                    .with_context(|| format!("无法新建日志 {}", path.display()))?;
                (stem, path, file)
            }
            Err(error) => {
                return Err(error).with_context(|| format!("无法新建日志 {}", path.display()))
            }
        };
        Ok(Self {
            directory: directory.to_path_buf(),
            stem,
            part: 0,
            file: BufWriter::with_capacity(64 * 1024, file),
            path,
            bytes: 0,
            limit,
            timestamps,
            line_open: false,
            written_text: String::new(),
        })
    }

    fn open_new(path: &Path) -> std::io::Result<File> {
        OpenOptions::new().write(true).create_new(true).open(path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn rotate(&mut self) -> Result<()> {
        self.flush()?;
        let part = self.part.checked_add(1).context("日志分文件序号溢出")?;
        let path = self.directory.join(format!("{}-{part}.log", self.stem));
        let file = Self::open_new(&path)
            .with_context(|| format!("无法新建日志分文件 {}", path.display()))?;
        self.file = BufWriter::with_capacity(64 * 1024, file);
        self.path = path;
        self.part = part;
        self.bytes = 0;
        Ok(())
    }

    fn write(&mut self, text: &str) -> Result<()> {
        self.file
            .write_all(text.as_bytes())
            .with_context(|| format!("无法写入日志 {}", self.path.display()))?;
        self.bytes = self.bytes.saturating_add(text.len() as u64);
        Ok(())
    }

    /// 未换行的提示符定期写出；后续追加同一行时只写增量。
    /// 已落盘内容被退格或覆盖时保留旧内容，再记录新的行状态。
    pub fn update(&mut self, text: &str, received: DateTime<Local>, complete: bool) -> Result<()> {
        if self.line_open && !text.starts_with(&self.written_text) {
            self.end_line()?;
        }
        if !self.line_open {
            // 在行之间切换文件，避免把中文或同一条普通日志拆入两个文件。
            if self.bytes >= self.limit {
                self.rotate()?;
            }
            if self.timestamps {
                self.write(&format!(
                    "[{}] ",
                    received.format("%Y%m%d-%H:%M:%S%.3f")
                ))?;
            }
            self.line_open = true;
        }
        self.write(&text[self.written_text.len()..])?;
        self.written_text.clear();
        self.written_text.push_str(text);
        if complete {
            self.end_line()?;
        }
        Ok(())
    }

    fn end_line(&mut self) -> Result<()> {
        if self.line_open {
            self.write("\n")?;
            self.line_open = false;
            self.written_text.clear();
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        self.file
            .flush()
            .with_context(|| format!("无法刷新日志 {}", self.path.display()))
    }

    pub fn finish(&mut self) -> Result<()> {
        self.end_line()?;
        self.flush()
    }
}
