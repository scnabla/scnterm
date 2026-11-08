//! scnterm 扩展：日志文本、文件连续性和后台异常处理测试。

use super::*;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);
struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "scnterm-log-test-{}-{}-{}",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn options(&self) -> SerialLogging {
        SerialLogging {
            directory: Some(self.0.clone()),
            timestamps: false,
            ..Default::default()
        }
    }
    fn files(&self) -> Vec<PathBuf> {
        let mut paths: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "log"))
            .collect();
        paths.sort();
        paths
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text_log(dir: &TestDir, limit: u64) -> TextLog {
    TextLog::new(file::LogFile::create(&dir.0, "COM3", limit, false).unwrap())
}

fn await_phase(log: &SerialLog, phase: Phase) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while log.snapshot().phase != phase {
        assert!(
            Instant::now() < deadline,
            "等待 {phase:?} 超时：{:?}",
            log.snapshot()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn split_utf8_ansi_and_progress_preserve_readable_output() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 1_000_000);
    let bytes = "\x1b]0;隐藏的标题\x07\x1b[31m启动完成\x1b[0m\r\n下载 10%\r下载 20%\r下载 100%\r\n"
        .as_bytes();
    // 每次一个字节，覆盖中文、控制指令和 CRLF 跨次读取。
    for byte in bytes {
        log.receive(&[*byte], Local::now()).unwrap();
    }
    log.finish().unwrap();
    assert_eq!(
        fs::read_to_string(log.path()).unwrap(),
        "启动完成\n下载 10%\n下载 20%\n下载 100%\n"
    );
}

#[test]
fn partial_prompt_flush_then_append_has_no_duplicate_prefix() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 1_000_000);
    log.receive(b"root# ", Local::now()).unwrap();
    log.flush().unwrap();
    assert_eq!(fs::read_to_string(log.path()).unwrap(), "root# ");
    log.receive(b"uname -a\r\n", Local::now()).unwrap();
    log.finish().unwrap();
    assert_eq!(fs::read_to_string(log.path()).unwrap(), "root# uname -a\n");
}

#[test]
fn erase_backspace_and_wide_characters() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 1_000_000);
    log.receive(
        "abc\x08 \x08X\n中文\x1b[2D好\n长进度条\r\x1b[2K完成\n".as_bytes(),
        Local::now(),
    )
    .unwrap();
    log.finish().unwrap();
    assert_eq!(
        fs::read_to_string(log.path()).unwrap(),
        "abX\n中好\n长进度条\n完成\n"
    );
}

#[test]
fn rotation_keeps_stem_and_does_not_split_utf8_or_lines() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 4);
    let first = log.path().to_path_buf();
    log.receive("一\n二\n三\n".as_bytes(), Local::now())
        .unwrap();
    log.finish().unwrap();
    let stem = first.file_stem().unwrap().to_str().unwrap();
    assert_eq!(fs::read_to_string(&first).unwrap(), "一\n");
    assert_eq!(
        fs::read_to_string(dir.0.join(format!("{stem}-1.log"))).unwrap(),
        "二\n"
    );
    assert_eq!(
        fs::read_to_string(dir.0.join(format!("{stem}-2.log"))).unwrap(),
        "三\n"
    );
}

#[test]
fn existing_part_is_never_overwritten() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 2);
    let stem = log
        .path()
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let collision = dir.0.join(format!("{stem}-1.log"));
    fs::write(&collision, "保留原文件").unwrap();
    log.receive(b"a\n", Local::now()).unwrap();
    assert!(log.receive(b"b\n", Local::now()).is_err());
    assert_eq!(fs::read_to_string(collision).unwrap(), "保留原文件");
}

#[test]
fn worker_start_stop_restart_and_close_drain_data() {
    let dir = TestDir::new();
    let log = SerialLog::new(0, "COM3".into());
    log.start(dir.options()).unwrap();
    log.receive(b"first\n", Local::now());
    log.stop();
    await_phase(&log, Phase::Stopped);
    let first = log.snapshot().path.unwrap();
    assert_eq!(fs::read_to_string(&first).unwrap(), "first\n");
    log.receive(b"not recorded\n", Local::now());
    log.start(dir.options()).unwrap();
    log.receive("第二次\n".as_bytes(), Local::now());
    log.close();
    assert!(log.wait_closed(Duration::from_secs(5)));
    let second = log.snapshot().path.unwrap();
    assert_ne!(first, second);
    assert_eq!(fs::read_to_string(second).unwrap(), "第二次\n");
    assert_eq!(dir.files().len(), 2);
    assert!(log.start(dir.options()).is_err());
}

#[test]
fn create_failure_can_recover_with_new_recording() {
    let dir = TestDir::new();
    let blocker = dir.0.join("not-a-directory");
    fs::write(&blocker, b"keep").unwrap();
    let log = SerialLog::new(1, "COM3".into());
    let mut invalid = dir.options();
    invalid.directory = Some(blocker);
    log.start(invalid).unwrap();
    await_phase(&log, Phase::Failed);
    assert!(log.take_error_notice().is_some());
    assert!(log.take_error_notice().is_none());
    assert!(log.snapshot().can_start);
    log.start(dir.options()).unwrap();
    log.receive(b"recovered\n", Local::now());
    log.close();
    assert!(log.wait_closed(Duration::from_secs(5)));
    assert_eq!(
        fs::read_to_string(log.snapshot().path.unwrap()).unwrap(),
        "recovered\n"
    );
}

#[test]
fn queue_overflow_stops_recording_without_blocking_receiver() {
    let dir = TestDir::new();
    let log = SerialLog::new(2, "COM3".into());
    log.start(dir.options()).unwrap();
    log.receive(&vec![b'x'; MAX_PENDING_BYTES + 1], Local::now());
    assert_eq!(log.snapshot().phase, Phase::Failed);
    assert!(log.snapshot().error.unwrap().contains("缓冲"));
    log.close();
    assert!(log.wait_closed(Duration::from_secs(5)));
}

#[test]
fn unfinished_control_string_and_long_line_are_bounded() {
    let dir = TestDir::new();
    let mut log = text_log(&dir, 1_000_000);
    let mut bytes = b"\x1b]0;".to_vec();
    bytes.extend(vec![b'x'; 70 * 1024]);
    assert!(log.receive(&bytes, Local::now()).is_err());
    let mut log = text_log(&dir, 1_000_000);
    assert!(log.receive(&vec![b'x'; 70 * 1024], Local::now()).is_err());
}

#[test]
fn timestamps_are_receive_time_not_flush_time() {
    use chrono::{TimeZone, Timelike};

    let dir = TestDir::new();
    let mut log = TextLog::new(file::LogFile::create(&dir.0, "COM3", 1_000_000, true).unwrap());
    let received = Local
        .with_ymd_and_hms(2026, 11, 8, 22, 36, 11)
        .single()
        .unwrap()
        .with_nanosecond(895_000_000)
        .unwrap();
    log.receive(b"timestamp\n", received).unwrap();
    log.finish().unwrap();
    let text = fs::read_to_string(log.path()).unwrap();
    assert_eq!(text, "[20261108-22:36:11.895] timestamp\n");
}
