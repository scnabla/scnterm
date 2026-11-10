//! scnterm 扩展：SSH 目录记录文件的配置校验与相对路径解析。

use std::path::{Path, PathBuf};

pub fn validate_file(value: &Option<PathBuf>) -> Result<(), String> {
    if let Some(path) = value {
        let text = path.to_string_lossy();
        if text.trim().is_empty()
            || text.chars().any(char::is_control)
            || text.ends_with('/')
            || (cfg!(windows) && text.ends_with('\\'))
            || path.file_name().is_none()
            || text == "."
            || text.ends_with("/.")
            || (cfg!(windows) && text.ends_with("\\."))
        {
            return Err("directory_bookmarks_file 必须是包含文件名的有效路径".into());
        }
        // Windows 的 C:foo 和 \foo 依赖进程当前盘符，不能按主 Lua 目录稳定解析。
        if !path.is_absolute()
            && path.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::Prefix(_) | std::path::Component::RootDir
                )
            })
        {
            return Err("directory_bookmarks_file 请使用完整绝对路径或普通相对路径".into());
        }
    }
    Ok(())
}

pub fn resolve_file(value: Option<&Path>, config_path: Option<&Path>) -> PathBuf {
    let base = config_path
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate::HOME_DIR.join(".config").join("wezterm"));
    let base = std::path::absolute(&base).unwrap_or(base);
    match value {
        None => base.join("scnterm-directory-bookmarks.json"),
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => base.join(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_bookmarks_default_relative_and_absolute_paths() {
        assert_eq!(
            resolve_file(None, None),
            crate::HOME_DIR.join(".config/wezterm/scnterm-directory-bookmarks.json")
        );
        let config = Path::new("config-test/wezterm.lua");
        assert_eq!(
            resolve_file(None, Some(config)),
            std::env::current_dir()
                .unwrap()
                .join("config-test/scnterm-directory-bookmarks.json")
        );
        let expected = std::env::current_dir()
            .unwrap()
            .join("config-test/数据/paths.json");
        assert_eq!(
            resolve_file(Some(Path::new("数据/paths.json")), Some(config)),
            expected
        );
        assert_eq!(resolve_file(Some(&expected), Some(config)), expected);
        assert!(validate_file(&None).is_ok());
        for name in ["", " ", ".", "..", "paths/", "paths/.", "/", "a\nb", "a\0b"] {
            assert!(validate_file(&Some(name.into())).is_err(), "{name:?}");
        }
        #[cfg(windows)]
        for name in [
            "C:paths.json",
            "\\paths.json",
            "C:\\",
            "paths\\",
            "paths\\.",
        ] {
            assert!(validate_file(&Some(name.into())).is_err(), "{name:?}");
        }
    }
}
