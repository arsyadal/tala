use std::{borrow::Cow, io::Write as _};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    Frame,
};

/// A terminal position in `(row, column)` order, relative to the screen.
pub type Point = (u16, u16);

#[derive(Default)]
pub struct Replies(Vec<u8>);

impl vt100::Callbacks for Replies {
    fn unhandled_csi(&mut self, screen: &mut vt100::Screen, first: Option<u8>, second: Option<u8>, params: &[&[u16]], command: char) {
        if second.is_some() { return; }
        match (first, params, command) {
            (None, [[5]], 'n') => self.0.extend_from_slice(b"\x1b[0n"),
            (None, [[6]], 'n') | (Some(b'?'), [[6]], 'n') => {
                let (row, col) = screen.cursor_position();
                let prefix = if first.is_some() { "?" } else { "" };
                let _ = write!(&mut self.0, "\x1b[{prefix}{};{}R", row + 1, col + 1);
            }
            (None, [] | [[0]], 'c') => self.0.extend_from_slice(b"\x1b[?1;2c"),
            _ => {}
        }
    }
}

impl Replies {
    pub fn flush(&mut self, send: impl FnOnce(&[u8])) {
        if !self.0.is_empty() {
            send(&self.0);
            self.0.clear();
        }
    }
}
const ESC: u8 = 0x1b;
const ASCII_BYTES: [[u8; 1]; 128] = ascii_bytes();

const fn ascii_bytes() -> [[u8; 1]; 128] {
    let mut bytes = [[0; 1]; 128];
    let mut index = 0;
    while index < bytes.len() {
        bytes[index][0] = index as u8;
        index += 1;
    }
    bytes
}

/// Draws the visible VT screen directly into Ratatui's frame buffer.
///
/// The selection endpoints are inclusive and may be supplied in either order.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    screen: &vt100::Screen,
    selection: Option<(Point, Point)>,
    focused: bool,
) {
    render_buffer(frame.buffer_mut(), area, screen, selection);

    if focused && !screen.hide_cursor() && screen.scrollback() == 0 {
        let (row, col) = screen.cursor_position();
        let (screen_rows, screen_cols) = screen.size();
        let frame_area = frame.area();
        if row < screen_rows.min(area.height) && col < screen_cols.min(area.width) {
            let x = area.x.saturating_add(col);
            let y = area.y.saturating_add(row);
            if x >= frame_area.x
                && x < frame_area.x.saturating_add(frame_area.width)
                && y >= frame_area.y
                && y < frame_area.y.saturating_add(frame_area.height)
            {
                frame.set_cursor_position((x, y));
            }
        }
    }
}

fn render_buffer(
    buffer: &mut Buffer,
    area: Rect,
    screen: &vt100::Screen,
    selection: Option<(Point, Point)>,
) {
    let buffer_area = buffer.area;
    let left = area.x.max(buffer_area.x);
    let top = area.y.max(buffer_area.y);
    let right = area
        .x
        .saturating_add(area.width)
        .min(buffer_area.x.saturating_add(buffer_area.width));
    let bottom = area
        .y
        .saturating_add(area.height)
        .min(buffer_area.y.saturating_add(buffer_area.height));
    let (screen_rows, screen_cols) = screen.size();

    for y in top..bottom {
        let row = y - area.y;
        for x in left..right {
            let col = x - area.x;
            let target = &mut buffer[(x, y)];
            target.reset();

            if row >= screen_rows || col >= screen_cols {
                continue;
            }
            let Some(source) = screen.cell(row, col) else {
                continue;
            };

            // A wide glyph must not be emitted if its continuation is clipped; doing so at the
            // right edge would make the host terminal wrap it onto the next line.
            let wide_is_clipped = source.is_wide()
                && (col.saturating_add(1) >= area.width
                    || x.saturating_add(1) >= right
                    || col.saturating_add(1) >= screen_cols);
            if !source.is_wide_continuation() && !wide_is_clipped && source.has_contents() {
                target.set_symbol(source.contents());
            }

            target.fg = color(source.fgcolor());
            target.bg = color(source.bgcolor());

            let mut modifier = Modifier::empty();
            modifier.set(Modifier::BOLD, source.bold());
            modifier.set(Modifier::DIM, source.dim());
            modifier.set(Modifier::ITALIC, source.italic());
            modifier.set(Modifier::UNDERLINED, source.underline());
            modifier.set(Modifier::REVERSED, source.inverse());

            if glyph_is_selected(screen, selection, row, col, source) {
                // Reversing rather than replacing colors retains indexed/RGB colors. Toggling
                // also keeps selection visible over cells that were already inverse-video.
                modifier.toggle(Modifier::REVERSED);
            }
            target.modifier = modifier;
        }
    }
}

fn glyph_is_selected(
    screen: &vt100::Screen,
    selection: Option<(Point, Point)>,
    row: u16,
    col: u16,
    cell: &vt100::Cell,
) -> bool {
    let selected = |point| point_is_selected(selection, point);
    if selected((row, col)) {
        return true;
    }

    // A double-width glyph is indivisible on the host terminal. Selecting either of its cells
    // therefore highlights both halves, avoiding a half-selected glyph that Ratatui cannot emit.
    if cell.is_wide() {
        selected((row, col.saturating_add(1)))
    } else if cell.is_wide_continuation() && col > 0 {
        selected((row, col - 1))
            && screen.cell(row, col - 1).is_some_and(vt100::Cell::is_wide)
    } else {
        false
    }
}

fn point_is_selected(selection: Option<(Point, Point)>, point: Point) -> bool {
    let Some((first, second)) = selection else {
        return false;
    };
    let (start, end) = if first <= second {
        (first, second)
    } else {
        (second, first)
    };
    point >= start && point <= end
}

const fn color(color: vt100::Color) -> Color {
    match color {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(index) => Color::Indexed(index),
        vt100::Color::Rgb(red, green, blue) => Color::Rgb(red, green, blue),
    }
}

/// Encodes a Crossterm key event as bytes understood by an xterm-compatible application.
///
/// Fixed sequences and ASCII text borrow static storage. UTF-8 text and modified xterm
/// sequences own their small output buffers.
pub fn key_bytes(
    key: KeyEvent,
    application_cursor: bool,
) -> Option<Cow<'static, [u8]>> {
    if key.kind == KeyEventKind::Release {
        return None;
    }

    let modifiers = key.modifiers;
    if modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::HYPER | KeyModifiers::META) {
        return None;
    }

    match key.code {
        KeyCode::Char(character) => character_bytes(character, modifiers),
        KeyCode::Null => prefixed_control(0, modifiers),
        KeyCode::Esc => prefixed_control(ESC, modifiers),
        KeyCode::Enter => prefixed_control(b'\r', modifiers),
        KeyCode::Backspace => {
            let byte = if modifiers.contains(KeyModifiers::CONTROL) {
                0x08
            } else {
                0x7f
            };
            prefixed_control(byte, modifiers)
        }
        KeyCode::Tab => prefixed_control(b'\t', modifiers),
        KeyCode::BackTab => backtab_bytes(modifiers),
        KeyCode::Up => cursor_key(b'A', modifiers, application_cursor),
        KeyCode::Down => cursor_key(b'B', modifiers, application_cursor),
        KeyCode::Right => cursor_key(b'C', modifiers, application_cursor),
        KeyCode::Left => cursor_key(b'D', modifiers, application_cursor),
        KeyCode::Home => cursor_key(b'H', modifiers, application_cursor),
        KeyCode::End => cursor_key(b'F', modifiers, application_cursor),
        KeyCode::Insert => tilde_key(b"2", modifiers),
        KeyCode::Delete => tilde_key(b"3", modifiers),
        KeyCode::PageUp => tilde_key(b"5", modifiers),
        KeyCode::PageDown => tilde_key(b"6", modifiers),
        KeyCode::F(number @ 1..=4) => function_key_1_to_4(number, modifiers),
        KeyCode::F(number @ 5..=12) => function_key_5_to_12(number, modifiers),
        _ => None,
    }
}

fn character_bytes(character: char, modifiers: KeyModifiers) -> Option<Cow<'static, [u8]>> {
    let alt = modifiers.contains(KeyModifiers::ALT);
    if modifiers.contains(KeyModifiers::CONTROL) {
        return control_byte(character).map(|byte| byte_with_alt(byte, alt));
    }

    if character.is_ascii() {
        return Some(byte_with_alt(character as u8, alt));
    }

    let mut encoded = [0; 4];
    let text = character.encode_utf8(&mut encoded).as_bytes();
    let mut bytes = Vec::with_capacity(text.len() + usize::from(alt));
    if alt {
        bytes.push(ESC);
    }
    bytes.extend_from_slice(text);
    Some(Cow::Owned(bytes))
}

fn control_byte(character: char) -> Option<u8> {
    match character {
        '@' | ' ' | '2' => Some(0x00),
        'a'..='z' => Some((character as u8) - b'a' + 1),
        'A'..='Z' => Some((character as u8) - b'A' + 1),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '/' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

fn prefixed_control(byte: u8, modifiers: KeyModifiers) -> Option<Cow<'static, [u8]>> {
    // Shift does not change control bytes; Crossterm has already applied it to text keys.
    let unsupported =
        modifiers.difference(KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT);
    if !unsupported.is_empty() {
        return None;
    }
    Some(byte_with_alt(byte, modifiers.contains(KeyModifiers::ALT)))
}

fn byte_with_alt(byte: u8, alt: bool) -> Cow<'static, [u8]> {
    if alt {
        Cow::Owned(vec![ESC, byte])
    } else {
        Cow::Borrowed(&ASCII_BYTES[usize::from(byte)])
    }
}

fn backtab_bytes(mut modifiers: KeyModifiers) -> Option<Cow<'static, [u8]>> {
    // BackTab denotes Shift+Tab even on platforms that omit SHIFT from the modifier field.
    modifiers.insert(KeyModifiers::SHIFT);
    if modifiers == KeyModifiers::SHIFT {
        Some(Cow::Borrowed(b"\x1b[Z"))
    } else {
        modified_csi(b"1", modifiers, b'Z')
    }
}

fn cursor_key(
    final_byte: u8,
    modifiers: KeyModifiers,
    application_cursor: bool,
) -> Option<Cow<'static, [u8]>> {
    if modifiers.is_empty() {
        let sequence: &'static [u8] = match (application_cursor, final_byte) {
            (false, b'A') => b"\x1b[A",
            (false, b'B') => b"\x1b[B",
            (false, b'C') => b"\x1b[C",
            (false, b'D') => b"\x1b[D",
            (false, b'H') => b"\x1b[H",
            (false, b'F') => b"\x1b[F",
            (true, b'A') => b"\x1bOA",
            (true, b'B') => b"\x1bOB",
            (true, b'C') => b"\x1bOC",
            (true, b'D') => b"\x1bOD",
            (true, b'H') => b"\x1bOH",
            (true, b'F') => b"\x1bOF",
            _ => return None,
        };
        Some(Cow::Borrowed(sequence))
    } else {
        modified_csi(b"1", modifiers, final_byte)
    }
}

fn tilde_key(number: &'static [u8], modifiers: KeyModifiers) -> Option<Cow<'static, [u8]>> {
    if modifiers.is_empty() {
        let sequence: &'static [u8] = match number {
            b"2" => b"\x1b[2~",
            b"3" => b"\x1b[3~",
            b"5" => b"\x1b[5~",
            b"6" => b"\x1b[6~",
            _ => return None,
        };
        Some(Cow::Borrowed(sequence))
    } else {
        modified_csi(number, modifiers, b'~')
    }
}

fn function_key_1_to_4(
    number: u8,
    modifiers: KeyModifiers,
) -> Option<Cow<'static, [u8]>> {
    let final_byte = b'P' + number - 1;
    if modifiers.is_empty() {
        let sequence: &'static [u8] = match number {
            1 => b"\x1bOP",
            2 => b"\x1bOQ",
            3 => b"\x1bOR",
            4 => b"\x1bOS",
            _ => return None,
        };
        Some(Cow::Borrowed(sequence))
    } else {
        modified_csi(b"1", modifiers, final_byte)
    }
}

fn function_key_5_to_12(
    number: u8,
    modifiers: KeyModifiers,
) -> Option<Cow<'static, [u8]>> {
    let parameter: &'static [u8] = match number {
        5 => b"15",
        6 => b"17",
        7 => b"18",
        8 => b"19",
        9 => b"20",
        10 => b"21",
        11 => b"23",
        12 => b"24",
        _ => return None,
    };
    if modifiers.is_empty() {
        let sequence: &'static [u8] = match number {
            5 => b"\x1b[15~",
            6 => b"\x1b[17~",
            7 => b"\x1b[18~",
            8 => b"\x1b[19~",
            9 => b"\x1b[20~",
            10 => b"\x1b[21~",
            11 => b"\x1b[23~",
            12 => b"\x1b[24~",
            _ => return None,
        };
        Some(Cow::Borrowed(sequence))
    } else {
        modified_csi(parameter, modifiers, b'~')
    }
}

fn modified_csi(
    parameter: &[u8],
    modifiers: KeyModifiers,
    final_byte: u8,
) -> Option<Cow<'static, [u8]>> {
    let modifier = xterm_modifier(modifiers)?;
    let mut sequence = Vec::with_capacity(parameter.len() + 5);
    sequence.extend_from_slice(b"\x1b[");
    sequence.extend_from_slice(parameter);
    sequence.push(b';');
    sequence.push(b'0' + modifier);
    sequence.push(final_byte);
    Some(Cow::Owned(sequence))
}

fn xterm_modifier(modifiers: KeyModifiers) -> Option<u8> {
    let unsupported =
        modifiers.difference(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);
    if !unsupported.is_empty() {
        return None;
    }
    Some(
        1 + u8::from(modifiers.contains(KeyModifiers::SHIFT))
            + 2 * u8::from(modifiers.contains(KeyModifiers::ALT))
            + 4 * u8::from(modifiers.contains(KeyModifiers::CONTROL)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_c_is_interrupt_and_alt_prefixes_it() {
        let control_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(key_bytes(control_c, false).as_deref(), Some(&b"\x03"[..]));

        let alt_control_c = KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::ALT | KeyModifiers::CONTROL,
        );
        assert_eq!(
            key_bytes(alt_control_c, false).as_deref(),
            Some(&b"\x1b\x03"[..])
        );
    }

    #[test]
    fn alt_unicode_is_escape_followed_by_utf8() {
        let key = KeyEvent::new(KeyCode::Char('é'), KeyModifiers::ALT);
        assert_eq!(key_bytes(key, false).as_deref(), Some(&b"\x1b\xc3\xa9"[..]));
    }

    #[test]
    fn application_cursor_and_xterm_modifiers_are_encoded() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(key_bytes(up, true).as_deref(), Some(&b"\x1bOA"[..]));

        let control_up = KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL);
        assert_eq!(
            key_bytes(control_up, true).as_deref(),
            Some(&b"\x1b[1;5A"[..])
        );
    }

    #[test]
    fn renders_combining_and_wide_glyphs_and_highlights_whole_wide_glyph() {
        let mut parser = vt100::Parser::new(2, 6, 0);
        parser.process("e\u{301}界".as_bytes());
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 2));

        render_buffer(
            &mut buffer,
            Rect::new(0, 0, 6, 2),
            parser.screen(),
            Some(((0, 2), (0, 2))),
        );

        assert_eq!(buffer[(0, 0)].symbol(), "e\u{301}");
        assert_eq!(buffer[(1, 0)].symbol(), "界");
        assert_eq!(buffer[(2, 0)].symbol(), " ");
        assert!(buffer[(1, 0)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(2, 0)].modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn renders_vt_colors_and_text_attributes() {
        let mut parser = vt100::Parser::new(1, 2, 0);
        parser.process(b"\x1b[38;2;1;2;3;48;5;42;1;3;4mA");
        let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));

        render_buffer(&mut buffer, Rect::new(0, 0, 2, 1), parser.screen(), None);

        let cell = &buffer[(0, 0)];
        assert_eq!(cell.symbol(), "A");
        assert_eq!(cell.fg, Color::Rgb(1, 2, 3));
        assert_eq!(cell.bg, Color::Indexed(42));
        assert!(cell.modifier.contains(Modifier::BOLD));
        assert!(cell.modifier.contains(Modifier::ITALIC));
        assert!(cell.modifier.contains(Modifier::UNDERLINED));
    }
}
