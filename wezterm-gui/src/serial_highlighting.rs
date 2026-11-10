//! scnterm 扩展：对串口绘制副本匹配并着色，保留终端原始文字及属性。

use config::SerialHighlighting;
use mux::pane::Pane;
use mux::renderable::RenderableDimensions;
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::sync::Arc;
use wezterm_term::{Line, StableRowIndex};

// 超长逻辑行保留设备原色；缓存仅保存文本与规则编号，不持有窗格。
const MAX_LINE_BYTES: usize = 64 * 1024;
const MAX_CACHE_BYTES: usize = 2 * 1024 * 1024;
const NO_MATCH: u8 = u8::MAX;

#[derive(Default)]
pub struct SerialHighlightCache {
    generation: usize,
    config: Option<SerialHighlighting>,
    entries: HashMap<String, Arc<Vec<u8>>>,
    bytes: usize,
}

impl SerialHighlightCache {
    fn matches(
        &mut self,
        text: &str,
        config: &SerialHighlighting,
        generation: usize,
    ) -> Arc<Vec<u8>> {
        if self.generation != generation
            || self
                .config
                .as_ref()
                .map_or(true, |old| !std::ptr::eq(old.rules(), config.rules()))
        {
            self.entries.clear();
            self.bytes = 0;
            self.generation = generation;
            self.config = Some(config.clone());
        }
        if let Some(found) = self.entries.get(text) {
            return Arc::clone(found);
        }
        let mut owners = vec![NO_MATCH; text.len()];
        for (index, rule) in config.rules().iter().enumerate() {
            for regex in &rule.patterns {
                for found in regex.find_iter(text) {
                    // 零长度匹配没有可着色文字；先命中的颜色只占用自身范围。
                    for owner in &mut owners[found.range()] {
                        if *owner == NO_MATCH {
                            *owner = index as u8;
                        }
                    }
                }
            }
        }
        let size = text.len() + owners.len();
        if self.bytes + size > MAX_CACHE_BYTES || self.entries.len() >= 512 {
            self.entries.clear();
            self.bytes = 0;
        }
        let owners = Arc::new(owners);
        self.entries.insert(text.to_owned(), Arc::clone(&owners));
        self.bytes += size;
        owners
    }

    fn highlight(&mut self, lines: &mut [Line], config: &SerialHighlighting, generation: usize) {
        let mut text = String::new();
        for line in lines.iter() {
            for cell in line.visible_cells() {
                if text.len() + cell.str().len() > MAX_LINE_BYTES {
                    return;
                }
                text.push_str(cell.str());
            }
        }
        let owners = self.matches(&text, config, generation);
        if owners.iter().all(|owner| *owner == NO_MATCH) {
            return;
        }
        let mut offset = 0;
        for line in lines {
            let mut changes = Vec::new();
            for cell in line.visible_cells() {
                let end = offset + cell.str().len();
                // 一个组合字符或宽字符占用同一显示单元，采用最靠前的规则。
                if let Some(owner) = owners[offset..end]
                    .iter()
                    .copied()
                    .min()
                    .filter(|v| *v != NO_MATCH)
                {
                    changes.push((cell.cell_index(), cell.width(), owner));
                }
                offset = end;
            }
            if !changes.is_empty() {
                // 副本继承的形状缓存属于原色，必须重新计算。
                line.clear_appdata();
                let cells = line.cells_mut_for_attr_changes_only();
                for (start, width, owner) in changes {
                    let end = (start + width).min(cells.len());
                    for cell in &mut cells[start..end] {
                        cell.attrs_mut()
                            .set_foreground(config.rules()[owner as usize].foreground);
                    }
                }
            }
        }
    }

    pub fn visible_lines(
        &mut self,
        pane: &dyn Pane,
        visible: Range<StableRowIndex>,
        config: &SerialHighlighting,
        generation: usize,
    ) -> Option<(StableRowIndex, Vec<Line>)> {
        if config.rules().is_empty() || !pane.is_serial() {
            return None;
        }
        self.collect_visible(
            visible,
            pane.get_dimensions(),
            config,
            generation,
            |range| pane.get_lines(range),
        )
    }

    fn collect_visible(
        &mut self,
        visible: Range<StableRowIndex>,
        dims: RenderableDimensions,
        config: &SerialHighlighting,
        generation: usize,
        mut get_lines: impl FnMut(Range<StableRowIndex>) -> (StableRowIndex, Vec<Line>),
    ) -> Option<(StableRowIndex, Vec<Line>)> {
        let (first, lines) = get_lines(visible);
        if lines.is_empty() {
            return Some((first, lines));
        }
        let count = lines.len();
        let mut snapshot = VecDeque::from(lines);
        let mut start = first;
        let mut prefix_complete = start == dims.scrollback_top;
        let mut context_bytes = 0;
        // 只补齐跨越可见区域边界的逻辑行，不扫描全部滚动历史。
        while start > dims.scrollback_top && context_bytes <= MAX_LINE_BYTES {
            let requested = (start - 32).max(dims.scrollback_top);
            let (actual, before) = get_lines(requested..start);
            if actual != requested || before.len() != (start - requested) as usize {
                break;
            }
            let boundary = before
                .iter()
                .rposition(|line| !line.last_cell_was_wrapped());
            let offset = boundary.map_or(0, |n| n + 1);
            for line in before.into_iter().skip(offset).rev() {
                context_bytes += line.as_str().len();
                snapshot.push_front(line);
                start -= 1;
            }
            if boundary.is_some() || start == dims.scrollback_top {
                prefix_complete = true;
                break;
            }
        }
        let mut suffix_complete = true;
        context_bytes = 0;
        while snapshot.back().map_or(false, Line::last_cell_was_wrapped) {
            let end = start + snapshot.len() as StableRowIndex;
            let screen_end = dims.physical_top + dims.viewport_rows as StableRowIndex;
            if end >= screen_end {
                break;
            }
            if context_bytes > MAX_LINE_BYTES {
                suffix_complete = false;
                break;
            }
            let requested_end = (end + 32).min(screen_end);
            let (actual, after) = get_lines(end..requested_end);
            if actual != end || after.is_empty() {
                suffix_complete = false;
                break;
            }
            for line in after {
                context_bytes += line.as_str().len();
                let wrapped = line.last_cell_was_wrapped();
                snapshot.push_back(line);
                if !wrapped {
                    break;
                }
            }
        }
        let mut lines: Vec<Line> = snapshot.into_iter().collect();
        self.highlight_snapshot(
            &mut lines,
            prefix_complete,
            suffix_complete,
            config,
            generation,
        );
        let offset = (first - start) as usize;
        Some((first, lines.into_iter().skip(offset).take(count).collect()))
    }

    fn highlight_snapshot(
        &mut self,
        lines: &mut [Line],
        prefix_complete: bool,
        suffix_complete: bool,
        config: &SerialHighlighting,
        generation: usize,
    ) {
        let mut begin = 0;
        for index in 0..lines.len() {
            if !lines[index].last_cell_was_wrapped() || index + 1 == lines.len() {
                if (begin != 0 || prefix_complete) && (index + 1 != lines.len() || suffix_complete)
                {
                    self.highlight(&mut lines[begin..=index], config, generation);
                }
                begin = index + 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::{SerialHighlightRule, SerialHighlightingConfig};
    use std::convert::TryFrom;
    use termwiz::cell::CellAttributes;
    use wezterm_term::{Terminal, TerminalConfiguration, TerminalSize};

    fn rules(patterns: &[(&str, &str)]) -> SerialHighlighting {
        SerialHighlighting::try_from(SerialHighlightingConfig {
            rules: patterns
                .iter()
                .map(|(pattern, color)| SerialHighlightRule {
                    foreground: color.to_string(),
                    keywords: vec![],
                    patterns: vec![pattern.to_string()],
                    case_sensitive: None,
                })
                .collect(),
            ..Default::default()
        })
        .unwrap()
    }

    fn line(text: &str, wrap: bool) -> Line {
        let mut line = Line::from_text(text, &CellAttributes::default(), 1, None);
        line.set_last_cell_was_wrapped(wrap, 1);
        line
    }

    fn color(line: &Line, col: usize) -> config::ColorAttribute {
        line.get_cell(col).unwrap().attrs().foreground()
    }

    #[test]
    fn serial_highlighting_priority_unicode_and_original_content() {
        let config = rules(&[("错误|ERROR", "#FF6B81"), ("ERROR42|e\u{301}", "#FFB52E")]);
        let original = line("错误 ERROR42 e\u{301}", false);
        let mut output = vec![original.clone()];
        let mut cache = SerialHighlightCache::default();
        cache.highlight_snapshot(&mut output, true, true, &config, 1);
        assert_eq!(output[0].as_str(), original.as_str());
        for col in [0, 2, 5, 9] {
            assert_eq!(color(&output[0], col), config.rules()[0].foreground);
        }
        assert_eq!(color(&output[0], 10), config.rules()[1].foreground);
        assert_eq!(color(&original, 0), config::ColorAttribute::Default);
        // 重叠的后续规则仍可给未占用的“42”着色。
        assert_eq!(color(&output[0], 11), config.rules()[1].foreground);
    }

    #[test]
    fn serial_highlighting_wraps_but_not_newlines_and_reloads() {
        let mut cache = SerialHighlightCache::default();
        let config = rules(&[("ERROR", "#FF6B81")]);
        let originals = vec![
            line("ER", true),
            line("ROR", false),
            line("ER", false),
            line("ROR", false),
        ];
        let mut output = originals.clone();
        cache.highlight_snapshot(&mut output, true, true, &config, 1);
        assert_eq!(color(&output[0], 0), config.rules()[0].foreground);
        assert_eq!(color(&output[1], 0), config.rules()[0].foreground);
        assert_eq!(color(&output[2], 0), config::ColorAttribute::Default);
        assert_eq!(color(&output[3], 0), config::ColorAttribute::Default);
        let changed = rules(&[("ROR", "#5EDBC6")]);
        let mut output = originals.clone();
        cache.highlight_snapshot(&mut output, true, true, &changed, 2);
        assert_eq!(color(&output[0], 0), config::ColorAttribute::Default);
        assert_eq!(color(&output[1], 0), changed.rules()[0].foreground);
        let mut output = originals.clone();
        cache.highlight_snapshot(&mut output, true, true, &SerialHighlighting::default(), 3);
        assert_eq!(output, originals);
    }

    #[test]
    fn serial_highlighting_visible_boundary_anchors_and_long_wrap() {
        let mut cache = SerialHighlightCache::default();
        let config = rules(&[(r"^ERROR.*END$", "#FFB52E")]);
        let mut lines = vec![line("ERROR", true)];
        lines.extend((0..80).map(|_| line("abcdefghijabcdefghij", true)));
        lines.push(line("END", false));
        lines.push(line("plain", false));
        let dims = RenderableDimensions {
            viewport_rows: lines.len(),
            ..Default::default()
        };
        let (first, output) = cache
            .collect_visible(40..42, dims, &config, 1, |range| {
                (
                    range.start,
                    lines[range.start as usize..range.end as usize].to_vec(),
                )
            })
            .unwrap();
        assert_eq!(first, 40);
        assert_eq!(output.len(), 2);
        assert_eq!(color(&output[0], 0), config.rules()[0].foreground);
        assert_eq!(color(&output[1], 0), config.rules()[0].foreground);
        assert_eq!(color(&lines[40], 0), config::ColorAttribute::Default);
        // 不完整的上下文不能被当作 ^ 或 $，避免假命中。
        let mut output = vec![line("ERROR END", false)];
        cache.highlight_snapshot(&mut output, false, true, &config, 1);
        assert_eq!(color(&output[0], 0), config::ColorAttribute::Default);
    }

    #[derive(Debug)]
    struct TestConfig;
    impl TerminalConfiguration for TestConfig {
        fn generation(&self) -> usize {
            0
        }
        fn color_palette(&self) -> wezterm_term::color::ColorPalette {
            Default::default()
        }
    }

    #[test]
    fn serial_highlighting_parser_chunks_ansi_and_carriage_return() {
        let mut terminal = Terminal::new(
            TerminalSize {
                rows: 4,
                cols: 20,
                ..Default::default()
            },
            Arc::new(TestConfig),
            "test",
            "test",
            Box::new(std::io::sink()),
        );
        let config = rules(&[("错误|ERROR", "#FFB52E")]);
        let mut cache = SerialHighlightCache::default();
        let text = "\x1b[34mERROR 错误".as_bytes();
        // 每个字节单独进入真实解析器，覆盖 ANSI 和 UTF-8 拆包。
        for byte in text {
            terminal.advance_bytes(&[*byte]);
        }
        let (_, original) = mux::renderable::terminal_get_lines(&mut terminal, 0..1);
        let mut output = original.clone();
        cache.highlight_snapshot(&mut output, true, true, &config, 1);
        assert_eq!(color(&output[0], 0), config.rules()[0].foreground);
        assert_eq!(color(&output[0], 6), config.rules()[0].foreground);
        assert_eq!(
            color(&original[0], 0),
            config::ColorAttribute::PaletteIndex(4)
        );
        terminal.advance_bytes(b"\r\x1b[2KOK");
        let (_, mut output) = mux::renderable::terminal_get_lines(&mut terminal, 0..1);
        cache.highlight_snapshot(&mut output, true, true, &config, 1);
        assert_eq!(
            color(&output[0], 0),
            config::ColorAttribute::PaletteIndex(4)
        );
        assert!(output[0].as_str().starts_with("OK"));
    }

    #[test]
    fn serial_highlighting_reflow_and_same_generation_override() {
        let mut terminal = Terminal::new(
            TerminalSize {
                rows: 10,
                cols: 5,
                ..Default::default()
            },
            Arc::new(TestConfig),
            "test",
            "test",
            Box::new(std::io::sink()),
        );
        let mut cache = SerialHighlightCache::default();
        let config = rules(&[("你好ERROR", "#FFB52E")]);
        terminal.advance_bytes("你好ERROR".as_bytes());
        for cols in [5, 7, 3, 20] {
            terminal.resize(TerminalSize {
                rows: 10,
                cols,
                ..Default::default()
            });
            let dims = mux::renderable::terminal_get_dimensions(&mut terminal);
            let (_, mut output) = mux::renderable::terminal_get_lines(
                &mut terminal,
                dims.physical_top..dims.physical_top + 10,
            );
            cache.highlight_snapshot(&mut output, true, true, &config, 1);
            let letters: Vec<_> = output
                .iter()
                .flat_map(|line| line.visible_cells())
                .filter(|cell| cell.str() != " ")
                .collect();
            assert_eq!(
                letters.iter().map(|cell| cell.str()).collect::<String>(),
                "你好ERROR",
                "cols={}",
                cols
            );
            for cell in letters {
                assert_eq!(
                    cell.attrs().foreground(),
                    config.rules()[0].foreground,
                    "cols={}, cell={:?}",
                    cols,
                    cell.str()
                );
            }
        }
        let changed = rules(&[("OTHER", "#FF6B81")]);
        assert_eq!(cache.matches("你好ERROR", &config, 1)[0], 0);
        assert_eq!(cache.matches("你好ERROR", &changed, 1)[0], NO_MATCH);
    }

    #[test]
    fn serial_highlighting_cache_and_input_limits() {
        let config = rules(&[("a|^|$", "#FFB52E")]);
        let mut cache = SerialHighlightCache::default();
        let first = cache.matches("a", &config, 1);
        assert!(Arc::ptr_eq(&first, &cache.matches("a", &config, 1)));
        for n in 0..600 {
            cache.matches(&format!("{}{}", "b".repeat(5000), n), &config, 1);
            assert!(cache.bytes <= MAX_CACHE_BYTES);
            assert!(cache.entries.len() <= 512);
        }
        let original = line(&"a".repeat(MAX_LINE_BYTES + 1), false);
        let mut output = vec![original.clone()];
        cache.highlight_snapshot(&mut output, true, true, &config, 1);
        assert_eq!(output[0], original);
    }
}
