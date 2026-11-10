//! ScnTerm 常用目录存储：按 SSH 连接隔离，校验文件并原子替换，不依赖界面或设备。

use anyhow::{bail, ensure, Context};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_PATHS: usize = 128;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bookmarks {
    version: u32,
    connections: BTreeMap<String, Vec<String>>,
}

impl Default for Bookmarks {
    fn default() -> Self {
        Self {
            version: 1,
            connections: BTreeMap::new(),
        }
    }
}

pub fn validate_path(path: &str) -> anyhow::Result<()> {
    ensure!(path.starts_with('/'), "目录必须是远端的绝对路径");
    ensure!(path.len() <= 4096, "目录路径超过 4096 字节");
    ensure!(
        !path.chars().any(char::is_control),
        "目录路径不能包含控制字符"
    );
    Ok(())
}

pub fn cd_command(path: &str) -> anyhow::Result<String> {
    validate_path(path)?;
    Ok(format!("cd -- '{}'\r", path.replace('\'', "'\\''")))
}

// 连接名称、主机、端口和用户共同组成键，名称相同但目标改变时不会混用。
pub fn connection_key(name: &str, host: &str, port: &str, user: &str) -> anyhow::Result<String> {
    Ok(serde_json::to_string(&(name, host, port, user))?)
}

pub struct Store {
    path: PathBuf,
    connection: String,
}

impl Store {
    pub fn new(path: PathBuf, connection: String) -> Self {
        Self { path, connection }
    }

    fn read(&self) -> anyhow::Result<Bookmarks> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Bookmarks::default())
            }
            Err(err) => return Err(err).context("无法读取目录列表"),
        };
        let mut data = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut data)?;
        ensure!(
            data.len() as u64 <= MAX_FILE_BYTES,
            "目录列表文件超过 1 MiB"
        );
        let result: Bookmarks =
            serde_json::from_slice(&data).context("目录列表文件格式错误，原文件已保留")?;
        ensure!(
            result.version == 1,
            "目录列表文件版本不受支持，原文件已保留"
        );
        ensure!(
            result.connections.len() <= 128,
            "目录列表最多支持 128 个连接"
        );
        for (key, paths) in &result.connections {
            ensure!(
                key.len() <= 4096 && paths.len() <= MAX_PATHS,
                "目录列表条目超出限制"
            );
            for path in paths {
                validate_path(path)?;
            }
        }
        Ok(result)
    }

    pub fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .read()?
            .connections
            .remove(&self.connection)
            .unwrap_or_default())
    }

    pub fn update(&self, path: &str, add: bool) -> anyhow::Result<Vec<String>> {
        validate_path(path)?;
        ensure!(self.connection.len() <= 4096, "SSH 连接标识过长");
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent).context("无法创建目录列表保存位置")?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // 系统文件锁在进程退出时自动释放；跨窗口/进程读改写不丢失记录。
        let mut lock_path = self.path.with_extension("lock");
        if self
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lock"))
        {
            // 自定义记录文件也可以以 .lock 结尾，文件锁必须使用另一个文件。
            let mut name = self.path.as_os_str().to_os_string();
            name.push(".lock");
            lock_path = name.into();
        }
        let lock = options
            .open(lock_path)
            .context("无法锁定目录列表")?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    bail!("目录列表正在被其他窗口修改，请重试")
                }
                Err(std::fs::TryLockError::Error(err)) => {
                    return Err(err).context("无法锁定目录列表")
                }
            }
        }
        let mut data = self.read()?;
        let entries = data.connections.entry(self.connection.clone()).or_default();
        if add && !entries.iter().any(|entry| entry == path) {
            ensure!(
                entries.len() < MAX_PATHS,
                "每个 SSH 连接最多保存 128 个目录"
            );
            entries.push(path.to_owned());
        } else if !add {
            entries.retain(|entry| entry != path);
        }
        let result = entries.clone();
        if result.is_empty() {
            data.connections.remove(&self.connection);
        }
        ensure!(data.connections.len() <= 128, "目录列表最多支持 128 个连接");
        let bytes = serde_json::to_vec_pretty(&data)?;
        ensure!(
            bytes.len() as u64 <= MAX_FILE_BYTES,
            "目录列表文件超过 1 MiB"
        );
        let mut temp =
            tempfile::NamedTempFile::new_in(parent).context("无法创建目录列表临时文件")?;
        temp.write_all(&bytes).context("无法写入目录列表")?;
        temp.as_file().sync_all().context("无法保存目录列表")?;
        temp.persist(&self.path)
            .map_err(|err| err.error)
            .context("无法替换目录列表，原文件已保留")?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_menu_custom_file_and_lock_name_are_separate() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["路径.lock", "路径.LOCK"] {
            let path = dir.path().join("nested").join(name);
            let store = Store::new(path.clone(), "test".into());
            store.update("/work", true).unwrap();
            store.update("/another", true).unwrap();
            assert_eq!(store.list().unwrap(), vec!["/work", "/another"]);
            assert!(path.exists());
            assert!(path.with_file_name(format!("{name}.lock")).exists());
        }
    }

    #[test]
    fn directory_menu_persists_isolated_connections_and_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        let a = Store::new(
            path.clone(),
            connection_key("test", "server", "22", "one").unwrap(),
        );
        let b = Store::new(
            path.clone(),
            connection_key("test", "server", "22", "two").unwrap(),
        );
        assert!(a.list().unwrap().is_empty());
        a.update("/work/中文 空格", true).unwrap();
        a.update("/work/中文 空格", true).unwrap();
        b.update("/other", true).unwrap();
        assert_eq!(a.list().unwrap(), vec!["/work/中文 空格"]);
        assert_eq!(b.list().unwrap(), vec!["/other"]);
        a.update("/work/中文 空格", false).unwrap();
        assert!(Store::new(path, a.connection).list().unwrap().is_empty());
        assert_eq!(b.list().unwrap(), vec!["/other"]);
    }

    #[test]
    fn directory_menu_preserves_invalid_files_and_limits_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        let store = Store::new(path.clone(), "test".into());
        for original in ["broken", r#"{"version":2,"connections":{}}"#] {
            std::fs::write(&path, original).unwrap();
            assert!(store.update("/work", true).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
        for bad in ["relative", "", "/a\nb", "/a\u{1b}", "/a\0b"] {
            assert!(cd_command(bad).is_err());
        }
        assert!(validate_path(&format!("/{}", "a".repeat(4096))).is_err());
        std::fs::remove_file(&path).unwrap();
        for n in 0..MAX_PATHS {
            store.update(&format!("/{n}"), true).unwrap();
        }
        assert!(store.update("/overflow", true).is_err());
        assert_eq!(store.list().unwrap().len(), MAX_PATHS);
        store.update("/0", true).unwrap();
    }

    #[test]
    fn directory_menu_concurrent_updates_do_not_lose_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut threads = vec![];
        for n in 0..8 {
            let store = Store::new(dir.path().join("bookmarks.json"), "test".into());
            threads.push(std::thread::spawn(move || {
                store.update(&format!("/{n}"), true).unwrap()
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(
            Store::new(dir.path().join("bookmarks.json"), "test".into())
                .list()
                .unwrap()
                .len(),
            8
        );
    }

    #[test]
    fn directory_menu_quotes_shell_metacharacters() {
        let path = "/tmp/a'b $(echo unwanted); 中文";
        assert_eq!(
            cd_command(path).unwrap(),
            "cd -- '/tmp/a'\\''b $(echo unwanted); 中文'\r"
        );
    }
}
