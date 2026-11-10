//! scnterm 扩展：串口关键词配置与正则编译，不参与收发和日志处理。

use crate::{ColorAttribute, RgbaColor};
use regex::{Regex, RegexBuilder};
use std::convert::TryFrom;
use std::sync::Arc;
use wezterm_dynamic::{FromDynamic, ToDynamic};

#[derive(Debug, Clone, Default, FromDynamic, ToDynamic)]
pub struct SerialHighlightingConfig {
    #[dynamic(default)]
    pub case_sensitive: bool,
    #[dynamic(default)]
    pub rules: Vec<SerialHighlightRule>,
}

#[derive(Debug, Clone, FromDynamic, ToDynamic)]
pub struct SerialHighlightRule {
    pub foreground: String,
    #[dynamic(default)]
    pub keywords: Vec<String>,
    #[dynamic(default)]
    pub patterns: Vec<String>,
    #[dynamic(default)]
    pub case_sensitive: Option<bool>,
}

#[derive(Debug)]
pub struct CompiledSerialHighlightRule {
    pub foreground: ColorAttribute,
    pub patterns: Vec<Regex>,
}

#[derive(Debug, Clone, Default, FromDynamic, ToDynamic)]
#[dynamic(
    try_from = "SerialHighlightingConfig",
    into = "SerialHighlightingConfig"
)]
pub struct SerialHighlighting {
    source: SerialHighlightingConfig,
    compiled: Arc<Vec<CompiledSerialHighlightRule>>,
}

impl From<&SerialHighlighting> for SerialHighlightingConfig {
    fn from(value: &SerialHighlighting) -> Self {
        value.source.clone()
    }
}

impl SerialHighlighting {
    pub fn rules(&self) -> &[CompiledSerialHighlightRule] {
        &self.compiled
    }
}

impl TryFrom<SerialHighlightingConfig> for SerialHighlighting {
    type Error = String;

    fn try_from(source: SerialHighlightingConfig) -> Result<Self, String> {
        // 限制配置与编译结果占用，避免配置错误拖慢串口窗口。
        if source.rules.len() > 32 {
            return Err("串口高亮最多支持 32 组规则".into());
        }
        let mut count = 0;
        let mut compiled = Vec::new();
        for (i, rule) in source.rules.iter().enumerate() {
            let label = format!("serial_highlighting.rules[{}]", i + 1);
            let color = RgbaColor::try_from(rule.foreground.clone())
                .map_err(|_| format!("{} 的 foreground 颜色无效：{}", label, rule.foreground))?;
            let mut patterns = Vec::new();
            for (field, values, literal) in [
                ("keywords", &rule.keywords, true),
                ("patterns", &rule.patterns, false),
            ] {
                for (j, value) in values.iter().enumerate() {
                    count += 1;
                    if count > 128 {
                        return Err("串口高亮的关键词与正则合计不能超过 128 条".into());
                    }
                    let location = format!("{}.{}[{}]", label, field, j + 1);
                    if value.is_empty() || value.len() > 4096 {
                        return Err(format!("{} 必须为 1 至 4096 字节的非空文本", location));
                    }
                    let pattern = if literal {
                        regex::escape(value)
                    } else {
                        value.clone()
                    };
                    let regex = RegexBuilder::new(&pattern)
                        .case_insensitive(!rule.case_sensitive.unwrap_or(source.case_sensitive))
                        .size_limit(256 * 1024)
                        .dfa_size_limit(256 * 1024)
                        .build()
                        .map_err(|err| format!("{} 正则无效：{}", location, err))?;
                    patterns.push(regex);
                }
            }
            compiled.push(CompiledSerialHighlightRule {
                foreground: ColorAttribute::TrueColorWithDefaultFallback(color.into()),
                patterns,
            });
        }
        Ok(Self {
            source,
            compiled: Arc::new(compiled),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pattern: &str) -> SerialHighlightingConfig {
        SerialHighlightingConfig {
            rules: vec![SerialHighlightRule {
                foreground: "#FFB52E".into(),
                keywords: vec!["[故障]".into()],
                patterns: vec![pattern.into()],
                case_sensitive: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn serial_highlighting_compiles_literals_regex_and_case_options() {
        let source = config(r"\bfail(?:ed|ure)?\b");
        let compiled = SerialHighlighting::try_from(source.clone()).unwrap();
        assert!(compiled.rules()[0].patterns[0].is_match("[故障]"));
        assert!(!compiled.rules()[0].patterns[0].is_match("故障"));
        for word in ["fail", "FAILED", "Failure"] {
            assert!(compiled.rules()[0].patterns[1].is_match(word));
        }
        let mut source = source;
        source.case_sensitive = true;
        assert!(!SerialHighlighting::try_from(source.clone())
            .unwrap()
            .rules()[0]
            .patterns[1]
            .is_match("FAILED"));
        source.rules[0].case_sensitive = Some(false);
        assert!(
            SerialHighlighting::try_from(source).unwrap().rules()[0].patterns[1].is_match("FAILED")
        );
    }

    #[test]
    fn serial_highlighting_reports_rule_and_pattern_location() {
        let err = SerialHighlighting::try_from(config("[")).unwrap_err();
        assert!(err.contains("rules[1].patterns[1]"), "{}", err);
        let mut source = config("error");
        source.rules[0].foreground = "not-a-color".into();
        assert!(SerialHighlighting::try_from(source)
            .unwrap_err()
            .contains("rules[1] 的 foreground"));
        for pattern in ["".to_string(), "a".repeat(4097)] {
            assert!(SerialHighlighting::try_from(config(&pattern)).is_err());
        }
    }

    #[test]
    fn serial_highlighting_dynamic_roundtrip_and_defaults() {
        let original = SerialHighlighting::try_from(config("error")).unwrap();
        let value = original.to_dynamic();
        let loaded = SerialHighlighting::from_dynamic(&value, Default::default()).unwrap();
        assert!(loaded.rules()[0].patterns[1].is_match("ERROR"));
        assert!(SerialHighlighting::default().rules().is_empty());
    }
}
