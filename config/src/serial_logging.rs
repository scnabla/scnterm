//! scnterm 扩展：串口日志的公共配置与路径解析。

use std::path::{Path, PathBuf};
use wezterm_dynamic::{FromDynamic, ToDynamic};

#[derive(Debug, Clone, FromDynamic, ToDynamic)]
pub struct SerialLogging {
    #[dynamic(default)]
    pub directory: Option<PathBuf>,
    #[dynamic(
        default = "default_max_file_size_mb",
        validate = "validate_max_file_size_mb"
    )]
    pub max_file_size_mb: u64,
    #[dynamic(default = "crate::default_true")]
    pub timestamps: bool,
}

fn default_max_file_size_mb() -> u64 {
    100
}

fn validate_max_file_size_mb(value: &u64) -> Result<(), String> {
    if *value == 0 || value.checked_mul(1_000_000).is_none() {
        Err("串口日志文件大小必须为大于零且不溢出的 MB 数值".into())
    } else {
        Ok(())
    }
}

impl Default for SerialLogging {
    fn default() -> Self {
        Self {
            directory: None,
            max_file_size_mb: default_max_file_size_mb(),
            timestamps: true,
        }
    }
}

impl SerialLogging {
    pub fn resolve_directory(&mut self, config_path: Option<&Path>) {
        let base = config_path
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| crate::HOME_DIR.join(".config").join("wezterm"));
        // 相对 --config-file 路径也在加载时固定，后续启动目录变化不影响日志。
        let base = std::path::absolute(&base).unwrap_or(base);
        self.directory = Some(match &self.directory {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => base.join(path),
            None => base,
        });
    }

    pub fn max_file_bytes(&self) -> anyhow::Result<u64> {
        validate_max_file_size_mb(&self.max_file_size_mb).map_err(anyhow::Error::msg)?;
        Ok(self.max_file_size_mb * 1_000_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_config_relative_directory() {
        let mut options = SerialLogging::default();
        assert!(options.timestamps);
        assert_eq!(options.max_file_bytes().unwrap(), 100_000_000);
        options.resolve_directory(Some(Path::new("configs/wezterm.lua")));
        assert_eq!(
            options.directory.as_ref().unwrap(),
            &std::env::current_dir().unwrap().join("configs")
        );

        let mut options = SerialLogging {
            directory: Some("logs".into()),
            ..Default::default()
        };
        options.resolve_directory(Some(Path::new("configs/wezterm.lua")));
        assert_eq!(
            options.directory.unwrap(),
            std::env::current_dir().unwrap().join("configs/logs")
        );
    }

    #[test]
    fn zero_and_overflowing_sizes_are_rejected() {
        for max_file_size_mb in [0, u64::MAX] {
            let options = SerialLogging {
                max_file_size_mb,
                ..Default::default()
            };
            assert!(options.max_file_bytes().is_err());
        }
    }
}
