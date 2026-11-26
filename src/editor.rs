use crate::theme::Theme;
use ratatui::style::Color;
use std::path::PathBuf;

pub struct EditorState {
    pub path: Option<PathBuf>,
    pub title: String,
    pub extension: String,
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub scroll_x: usize,
    pub editable: bool,
    pub editing: bool,
    pub dirty: bool,
    pub is_diff: bool,
}

impl EditorState {
    pub fn for_file(path: PathBuf, title: String, body: &str) -> Self {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut lines: Vec<String> = body.lines().map(ToOwned::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        } else if body.ends_with('\n') {
            lines.push(String::new());
        }
        Self {
            path: Some(path),
            title,
            extension,
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_y: 0,
            scroll_x: 0,
            editable: true,
            editing: false,
            dirty: false,
            is_diff: false,
        }
    }

    pub fn for_diff(title: String, body: &str) -> Self {
        let lines: Vec<String> = body.lines().map(ToOwned::to_owned).collect();
        Self {
            path: None,
            title,
            extension: String::new(),
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_y: 0,
            scroll_x: 0,
            editable: false,
            editing: false,
            dirty: false,
            is_diff: true,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn toggle_edit(&mut self) -> bool {
        if !self.editable {
            return false;
        }
        self.editing = !self.editing;
        if self.editing && self.lines.is_empty() {
            self.lines.push(String::new());
        }
        self.clamp_cursor();
        true
    }

    pub fn move_up(&mut self, amount: usize) {
        if self.editing {
            self.cursor_row = self.cursor_row.saturating_sub(amount);
            self.clamp_cursor();
        } else {
            self.scroll_y = self.scroll_y.saturating_sub(amount);
        }
    }

    pub fn move_down(&mut self, amount: usize) {
        if self.editing {
            self.cursor_row = self.cursor_row.saturating_add(amount).min(self.lines.len().saturating_sub(1));
            self.clamp_cursor();
        } else {
            self.scroll_y = self.scroll_y.saturating_add(amount).min(self.lines.len().saturating_sub(1));
        }
    }

    pub fn move_left(&mut self) {
        if self.editing {
            if self.cursor_col > 0 {
                self.cursor_col -= 1;
            } else if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.line_len(self.cursor_row);
            }
        } else {
            self.scroll_x = self.scroll_x.saturating_sub(4);
        }
    }

    pub fn move_right(&mut self) {
        if self.editing {
            let len = self.line_len(self.cursor_row);
            if self.cursor_col < len {
                self.cursor_col += 1;
            } else if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
        } else {
            self.scroll_x = self.scroll_x.saturating_add(4);
        }
    }

    pub fn move_home(&mut self) {
        if self.editing {
            self.cursor_col = 0;
        } else {
            self.scroll_y = 0;
            self.scroll_x = 0;
        }
    }

    pub fn move_end(&mut self) {
        if self.editing {
            self.cursor_col = self.line_len(self.cursor_row);
        } else {
            self.scroll_y = self.lines.len().saturating_sub(1);
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        if !self.editing {
            return;
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte(line, self.cursor_col);
        line.insert(byte_idx, ch);
        self.cursor_col += 1;
        self.dirty = true;
    }

    pub fn insert_newline(&mut self) {
        if !self.editing {
            return;
        }
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        let line = &mut self.lines[self.cursor_row];
        let byte_idx = char_to_byte(line, self.cursor_col);
        let tail = line.split_off(byte_idx);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, tail);
        self.dirty = true;
    }

    pub fn insert_text(&mut self, text: &str) {
        if !self.editing {
            return;
        }
        for ch in text.chars() {
            match ch {
                '\r' => {}
                '\n' => self.insert_newline(),
                '\t' => {
                    for _ in 0..4 {
                        self.insert_char(' ');
                    }
                }
                c if !c.is_control() => self.insert_char(c),
                _ => {}
            }
        }
    }

    pub fn backspace(&mut self) {
        if !self.editing || self.lines.is_empty() {
            return;
        }
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let end = char_to_byte(line, self.cursor_col);
            let start = char_to_byte(line, self.cursor_col - 1);
            line.replace_range(start..end, "");
            self.cursor_col -= 1;
            self.dirty = true;
        } else if self.cursor_row > 0 {
            let removed = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.line_len(self.cursor_row);
            self.lines[self.cursor_row].push_str(&removed);
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        if !self.editing || self.lines.is_empty() {
            return;
        }
        let len = self.line_len(self.cursor_row);
        if self.cursor_col < len {
            let line = &mut self.lines[self.cursor_row];
            let start = char_to_byte(line, self.cursor_col);
            let end = char_to_byte(line, self.cursor_col + 1);
            line.replace_range(start..end, "");
            self.dirty = true;
        } else if self.cursor_row + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
            self.dirty = true;
        }
    }

    pub fn sync_viewport(&mut self, visible_rows: usize, visible_cols: usize) {
        if self.editing {
            if self.cursor_row < self.scroll_y {
                self.scroll_y = self.cursor_row;
            } else if visible_rows > 0 && self.cursor_row >= self.scroll_y + visible_rows {
                self.scroll_y = self.cursor_row + 1 - visible_rows;
            }
            if self.cursor_col < self.scroll_x {
                self.scroll_x = self.cursor_col;
            } else if visible_cols > 0 && self.cursor_col >= self.scroll_x + visible_cols {
                self.scroll_x = self.cursor_col + 1 - visible_cols;
            }
        } else {
            let max_scroll = self.lines.len().saturating_sub(visible_rows);
            self.scroll_y = self.scroll_y.min(max_scroll);
        }
    }

    fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map_or(0, |line| line.chars().count())
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.cursor_row = 0;
            self.cursor_col = 0;
            return;
        }
        self.cursor_row = self.cursor_row.min(self.lines.len() - 1);
        self.cursor_col = self.cursor_col.min(self.line_len(self.cursor_row));
    }
}

fn char_to_byte(line: &str, col: usize) -> usize {
    line.char_indices().nth(col).map_or(line.len(), |(idx, _)| idx)
}

pub fn highlight_line(line: &str, ext: &str, is_diff: bool, theme: &Theme) -> Vec<(String, Color)> {
    if is_diff {
        let color = match line.as_bytes().first() {
            Some(b'+') => theme.success,
            Some(b'-') => theme.danger,
            Some(b'@') => theme.accent,
            _ => theme.fg,
        };
        return vec![(line.to_owned(), color)];
    }

    let trimmed = line.trim_start();
    if matches!(ext, "md" | "markdown") {
        if trimmed.starts_with('#') {
            return vec![(line.to_owned(), theme.accent)];
        }
        if trimmed.starts_with("```") {
            return vec![(line.to_owned(), theme.comment)];
        }
        return vec![(line.to_owned(), theme.fg)];
    }

    let (comment_prefix, keywords): (Option<&str>, &[&str]) = match ext {
        "rs" => (
            Some("//"),
            &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "for", "while",
                "loop", "if", "else", "match", "return", "use", "mod", "const", "static", "async",
                "await", "where", "type", "true", "false", "self", "Self", "crate", "super",
            ],
        ),
        "js" | "jsx" | "ts" | "tsx" => (
            Some("//"),
            &[
                "function", "const", "let", "var", "return", "if", "else", "for", "while", "class",
                "export", "import", "from", "async", "await", "type", "interface", "true", "false",
                "null", "undefined", "new", "try", "catch",
            ],
        ),
        "py" => (
            Some("#"),
            &[
                "def", "class", "return", "if", "elif", "else", "for", "while", "import", "from",
                "as", "with", "try", "except", "finally", "pass", "True", "False", "None", "and",
                "or", "not", "in", "is", "async", "await", "yield",
            ],
        ),
        "toml" | "sh" | "bash" | "zsh" | "fish" | "yml" | "yaml" => (
            Some("#"),
            &[
                "true", "false", "if", "then", "else", "fi", "for", "do", "done", "case", "esac",
                "in", "function", "export", "local", "return", "exit",
            ],
        ),
        "json" => (None, &["true", "false", "null"]),
        _ => return vec![(line.to_owned(), theme.fg)],
    };

    if ext == "toml" && trimmed.starts_with('[') && trimmed.ends_with(']') {
        return vec![(line.to_owned(), theme.accent)];
    }

    let mut spans = Vec::new();
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if let Some(prefix) = comment_prefix {
            if line[byte_pos..].starts_with(prefix) {
                spans.push((line[byte_pos..].to_owned(), theme.comment));
                break;
            }
        }
        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = byte_pos;
            i += 1;
            let mut escaped = false;
            while i < chars.len() {
                let c = chars[i].1;
                i += 1;
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == quote {
                    break;
                }
            }
            let end = chars.get(i).map_or(line.len(), |&(pos, _)| pos);
            spans.push((line[start..end].to_owned(), theme.string));
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = byte_pos;
            i += 1;
            while i < chars.len() && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let end = chars.get(i).map_or(line.len(), |&(pos, _)| pos);
            let word = &line[start..end];
            let color = if keywords.contains(&word) { theme.keyword } else { theme.fg };
            spans.push((word.to_owned(), color));
            continue;
        }
        if ch.is_ascii_digit() {
            let start = byte_pos;
            i += 1;
            while i < chars.len() && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '_' || chars[i].1 == '.') {
                i += 1;
            }
            let end = chars.get(i).map_or(line.len(), |&(pos, _)| pos);
            spans.push((line[start..end].to_owned(), theme.number));
            continue;
        }
        let start = byte_pos;
        i += 1;
        while i < chars.len() {
            let (next_pos, next_ch) = chars[i];
            if comment_prefix.is_some_and(|prefix| line[next_pos..].starts_with(prefix))
                || matches!(next_ch, '"' | '\'')
                || next_ch.is_ascii_alphanumeric()
                || next_ch == '_'
            {
                break;
            }
            i += 1;
        }
        let end = chars.get(i).map_or(line.len(), |&(pos, _)| pos);
        spans.push((line[start..end].to_owned(), theme.fg));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeName;

    #[test]
    fn editor_inserts_splits_joins_and_preserves_unicode() {
        let mut editor = EditorState::for_file(PathBuf::from("demo.rs"), "demo.rs".into(), "héllo");
        assert!(editor.toggle_edit());
        editor.move_end();
        editor.insert_char('!');
        editor.insert_newline();
        editor.insert_text("world");
        assert_eq!(editor.text(), "héllo!\nworld");
        editor.move_home();
        editor.backspace();
        assert_eq!(editor.text(), "héllo!world");
        assert!(editor.dirty);
    }

    #[test]
    fn highlights_rust_keywords_strings_and_comments() {
        let theme = ThemeName::Dark.palette();
        let spans = highlight_line("let msg = \"hi\"; // note", "rs", false, &theme);
        assert!(spans.iter().any(|(text, color)| text == "let" && *color == theme.keyword));
        assert!(spans.iter().any(|(text, color)| text == "\"hi\"" && *color == theme.string));
        assert!(spans.iter().any(|(text, color)| text == "// note" && *color == theme.comment));
    }
}
