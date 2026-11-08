//! scnterm 扩展：串口日志的 UTF-8 和单行终端控制处理。

use super::file::LogFile;
use anyhow::{bail, Result};
use chrono::{DateTime, Local};
use vtparse::{CsiParam, VTActor, VTParser};

const MAX_LINE_CELLS: usize = 64 * 1024;
const MAX_ESCAPE_BYTES: usize = 64 * 1024;

struct Record {
    text: String,
    received: DateTime<Local>,
}

struct Line {
    cells: Vec<String>,
    cursor: usize,
    dirty: bool,
    recorded: bool,
    received: DateTime<Local>,
    now: DateTime<Local>,
    records: Vec<Record>,
    overflow: bool,
}

impl Line {
    fn new() -> Self {
        let now = Local::now();
        Self {
            cells: vec![],
            cursor: 0,
            dirty: false,
            recorded: false,
            received: now,
            now,
            records: vec![],
            overflow: false,
        }
    }

    fn text(&self) -> String {
        self.cells.concat()
    }

    fn snapshot(&mut self, blank: bool) {
        if self.dirty || (blank && !self.recorded) {
            self.records.push(Record {
                text: self.text(),
                received: self.received,
            });
            self.dirty = false;
            self.recorded = true;
        }
    }

    fn changed(&mut self) {
        if !self.dirty {
            self.received = self.now;
        }
        self.dirty = true;
    }

    fn move_cursor(&mut self, cursor: usize) {
        if cursor >= MAX_LINE_CELLS {
            self.overflow = true;
        } else {
            self.cursor = cursor;
        }
    }

    fn clear_wide_at(&mut self, column: usize) {
        if column >= self.cells.len() {
            return;
        }
        if self.cells[column].is_empty() && column > 0 {
            self.cells[column - 1] = " ".into();
        }
        if column + 1 < self.cells.len() && self.cells[column + 1].is_empty() {
            self.cells[column + 1] = " ".into();
        }
    }
}

impl VTActor for Line {
    fn print(&mut self, ch: char) {
        let width = termwiz::cell::unicode_column_width(&ch.to_string(), None).min(2);
        if width == 0 {
            if self.cursor > 0 && self.cursor <= self.cells.len() {
                let mut index = self.cursor - 1;
                if self.cells[index].is_empty() && index > 0 {
                    index -= 1;
                }
                if self.cells[index].len() > 256 {
                    self.overflow = true;
                    return;
                }
                self.changed();
                self.cells[index].push(ch);
            }
            return;
        }
        if self.cursor + width > MAX_LINE_CELLS {
            self.overflow = true;
            return;
        }
        self.changed();
        self.clear_wide_at(self.cursor);
        if width == 2 {
            self.clear_wide_at(self.cursor + 1);
        }
        self.cells
            .resize(self.cells.len().max(self.cursor + width), " ".into());
        self.cells[self.cursor] = ch.to_string();
        if width == 2 {
            self.cells[self.cursor + 1].clear();
        }
        self.cursor += width;
    }

    fn execute_c0_or_c1(&mut self, control: u8) {
        match control {
            b'\r' => {
                self.snapshot(false);
                self.cursor = 0;
            }
            b'\n' | 0x0b | 0x0c => {
                if !self.dirty {
                    self.received = self.now;
                }
                self.snapshot(true);
                self.cells.clear();
                self.cursor = 0;
                self.recorded = false;
            }
            8 => {
                self.cursor = self.cursor.saturating_sub(1);
            }
            b'\t' => {
                let next = (self.cursor / 8 + 1) * 8;
                while self.cursor < next && !self.overflow {
                    self.print(' ');
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &[CsiParam], truncated: bool, byte: u8) {
        if truncated
            || params
                .iter()
                .any(|p| matches!(p, CsiParam::P(b'?' | b'>' | b'!')))
        {
            return;
        }
        let value = params
            .first()
            .and_then(CsiParam::as_integer)
            .unwrap_or(0)
            .max(0) as usize;
        let count = value.max(1);
        match byte {
            b'D' => self.cursor = self.cursor.saturating_sub(count),
            b'C' => self.move_cursor(self.cursor.saturating_add(count)),
            b'G' | b'`' => self.move_cursor(count - 1),
            b'K' => {
                self.changed();
                match value {
                    0 => {
                        self.clear_wide_at(self.cursor);
                        self.cells.truncate(self.cursor);
                    }
                    1 => {
                        self.clear_wide_at(self.cursor);
                        let end = (self.cursor + 1).min(self.cells.len());
                        for cell in &mut self.cells[..end] {
                            *cell = " ".into();
                        }
                    }
                    2 => {
                        self.cells.clear();
                    }
                    _ => {}
                }
            }
            // 全屏重绘只保留输出顺序，不尝试录制屏幕布局。
            b'H' | b'f' | b'J' | b'A' | b'B' | b'E' | b'F' => {
                self.snapshot(false);
                self.cells.clear();
                self.cursor = 0;
                self.recorded = false;
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, byte: u8) {
        if byte == b'c' {
            self.snapshot(false);
            self.cells.clear();
            self.cursor = 0;
        }
    }
    fn osc_dispatch(&mut self, _: &[&[u8]]) {}
    fn apc_dispatch(&mut self, _: Vec<u8>) {}
    fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
    fn dcs_put(&mut self, _: u8) {}
    fn dcs_unhook(&mut self) {}
}

pub(super) struct TextLog {
    parser: VTParser,
    line: Line,
    escape_bytes: usize,
    file: LogFile,
}

impl TextLog {
    pub fn new(file: LogFile) -> Self {
        Self {
            parser: VTParser::new(),
            line: Line::new(),
            escape_bytes: 0,
            file,
        }
    }

    pub fn path(&self) -> &std::path::Path {
        self.file.path()
    }

    pub fn receive(&mut self, bytes: &[u8], received: DateTime<Local>) -> Result<()> {
        self.line.now = received;
        for &byte in bytes {
            self.parser.parse_byte(byte, &mut self.line);
            if self.parser.is_ground() {
                self.escape_bytes = 0;
            } else {
                self.escape_bytes += 1;
            }
            if self.escape_bytes > MAX_ESCAPE_BYTES {
                bail!("终端控制指令过长，日志记录已停止");
            }
            if self.line.overflow {
                bail!("单行日志过长，日志记录已停止");
            }
            // 逐条写出，避免一批高频覆盖刷新积累大量行快照。
            for record in self.line.records.drain(..) {
                self.file.update(&record.text, record.received, true)?;
            }
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.line.dirty {
            self.file
                .update(&self.line.text(), self.line.received, false)?;
        }
        self.file.flush()
    }

    pub fn finish(&mut self) -> Result<()> {
        self.flush()?;
        self.file.finish()
    }
}
