use crate::{app::{Action, App, Overlay, Panel}, terminal::Kind, terminal_view};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::borrow::Cow;

const ACCENT: Color = Color::LightMagenta;
const MUTED: Color = Color::Gray;
const QUIET: Color = Color::DarkGray;
const BORDER: Color = Color::Gray;
const SELECT: Color = Color::Rgb(52, 43, 73);

pub fn draw(app: &mut App, frame: &mut Frame<'_>) {
    let area = frame.area();
    app.hits.clear();
    app.terminal_area = Rect::ZERO;
    app.list_area = Rect::ZERO;
    if area.width < 34 || area.height < 9 {
        frame.render_widget(Paragraph::new("Make this terminal larger. F10 quits Tala.").wrap(Wrap { trim: false }), area);
        return;
    }

    let head = row(area, 0);
    line(frame, head, Line::from(vec![
        Span::styled(" tala  ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("/  ", Style::new().fg(QUIET)),
        Span::raw(safe(&app.folder.to_string_lossy())),
    ]));
    actions(frame, &mut app.hits, row(area, 1), &[
        ("F2 Files", Action::Panel(Panel::Files), app.panel == Some(Panel::Files)),
        ("F3 Launcher", Action::Panel(Panel::Launcher), app.panel == Some(Panel::Launcher)),
        ("F4 Git", Action::Panel(Panel::Git), app.panel == Some(Panel::Git)),
        ("F1 Help", Action::Help, false),
        ("F10 Quit", Action::Quit, false),
    ]);

    let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
    let main = match app.panel {
        Some(panel) if area.width >= 77 => {
            let width = if area.width >= 120 { 37 } else { 30 };
            sidebar(app, frame, Rect::new(body.x, body.y, width, body.height), panel);
            Rect::new(body.x + width, body.y, body.width - width, body.height)
        }
        Some(panel) if app.panel_focused => {
            sidebar(app, frame, body, panel);
            Rect::ZERO
        }
        _ => body,
    };
    if !main.is_empty() { terminal(app, frame, main); }
    status(app, frame, row(area, area.height - 2), row(area, area.height - 1));
    if app.overlay.is_some() { overlay(app, frame, area); }
}

fn terminal(app: &mut App, frame: &mut Frame<'_>, area: Rect) {
    let block = Block::new().borders(Borders::TOP).border_style(Style::new().fg(BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let tabs = row(inner, 0);
    let mut strip = tabs;
    let tools = if area.width >= 55 { [("F5 New", Action::New, false), ("F7 Close", Action::Close, false)] } else { [("+", Action::New, false), ("×", Action::Close, false)] };
    let used = actions(frame, &mut app.hits, tabs, &tools);
    strip.x += used;
    strip.width -= used;
    if app.tabs.is_empty() {
        line(frame, strip, Line::styled("  TERMINAL", Style::new().fg(QUIET)));
    } else {
        let slots = usize::from((strip.width / 16).max(1));
        let first = (app.active + 1).saturating_sub(slots);
        for (index, tab) in app.tabs.iter().enumerate().skip(first).take(slots) {
            if strip.width < 4 { break; }
            let width = strip.width.min(16);
            let slot = Rect::new(strip.x, strip.y, width, 1);
            let style = if index == app.active { selected() } else { Style::new().fg(MUTED) };
            let marker = if tab.session.is_some() { " " } else { "·" };
            line(frame, slot, Line::styled(format!(" {index} {marker}{}", tab.profile.label, index = index + 1), style));
            app.hits.push((slot, Action::Tab(index)));
            strip.x += width;
            strip.width -= width;
        }
    }

    let content = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));
    app.terminal_area = content;
    if let Some(tab) = app.tabs.get(app.active) {
        terminal_view::draw(frame, content, tab.parser.screen(), tab.selection, !app.panel_focused && app.overlay.is_none() && tab.session.is_some());
    } else {
        let width = content.width.saturating_sub(4).min(54);
        let x = content.x + (content.width - width) / 2;
        let y = content.y + content.height.saturating_sub(6) / 2;
        let welcome = Rect::new(x, y, width, content.bottom().saturating_sub(y).min(6));
        line(frame, row(welcome, 0), Line::styled("tala", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)));
        line(frame, row(welcome, 1), Line::raw("Find your flow."));
        if welcome.height > 3 {
            actions(frame, &mut app.hits, row(welcome, 3), &[("F5 New terminal", Action::New, true), ("F3 Launcher", Action::Panel(Panel::Launcher), false)]);
        }
    }
}

fn sidebar(app: &mut App, frame: &mut Frame<'_>, area: Rect, panel: Panel) {
    let active = app.panel_focused && app.overlay.is_none();
    let title = match panel { Panel::Files => " FILES ", Panel::Launcher => " LAUNCHER ", Panel::Git => " SOURCE CONTROL " };
    let block = Block::new().borders(Borders::TOP | Borders::RIGHT)
        .border_style(Style::new().fg(if active { ACCENT } else { BORDER }))
        .title(Line::styled(title, Style::new().fg(if active { ACCENT } else { MUTED }).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let area = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height);
    if area.height < 3 { return; }
    match panel {
        Panel::Files => files(app, frame, area),
        Panel::Launcher => launcher(app, frame, area),
        Panel::Git => source_control(app, frame, area),
    }
}

fn files(app: &mut App, frame: &mut Frame<'_>, area: Rect) {
    line(frame, row(area, 0), Line::styled(safe(&app.browsing.to_string_lossy()), Style::new().fg(MUTED)));
    actions(frame, &mut app.hits, row(area, 1), &[
        ("Up", Action::Up, false), ("Use folder (o)", Action::UseFolder, app.folder != app.browsing),
    ]);
    let list = after(area, 3);
    app.list_area = list;
    if let Some(error) = &app.files_error { message(frame, list, error, Color::LightRed); }
    else if app.entries.is_empty() { message(frame, list, "This folder is empty.", MUTED); }
    for (y, index) in visible(list, app.entries.len(), app.file_index) {
        let entry = &app.entries[index];
        let style = item_style(app.panel_focused, index == app.file_index);
        let slot = row(list, y);
        line(frame, slot, Line::from(vec![
            Span::styled(if entry.directory { "/ " } else { "  " }, style.fg(if entry.directory { ACCENT } else { MUTED })),
            Span::styled(safe(&entry.name), style),
        ]).style(style));
        app.hits.push((slot, Action::Entry(index)));
    }
}

fn launcher(app: &mut App, frame: &mut Frame<'_>, area: Rect) {
    line(frame, row(area, 0), Line::styled("Enter to launch · r to rescan", Style::new().fg(MUTED)));
    let list = after(area, 2);
    app.list_area = list;
    for (y, index) in visible(list, app.profiles.len(), app.profile_index) {
        let profile = &app.profiles[index];
        let style = item_style(app.panel_focused, index == app.profile_index);
        let slot = row(list, y);
        let marker = if profile.kind == Kind::Shell { "> " } else { "* " };
        line(frame, slot, Line::from(vec![
            Span::styled(marker, style.fg(ACCENT)),
            Span::styled(profile.label, style),
            Span::styled(if profile.available { "" } else { "  not found" }, style.fg(MUTED)),
        ]).style(style));
        app.hits.push((slot, Action::Profile(index)));
    }
}

fn source_control(app: &mut App, frame: &mut Frame<'_>, area: Rect) {
    let busy = app.git_work.is_some();
    if let Some(git) = &app.git {
        line(frame, row(area, 0), Line::from(vec![Span::styled("branch ", Style::new().fg(MUTED)), Span::styled(safe(&git.branch), Style::new().fg(ACCENT))]));
        let selected_change = git.changes.get(app.git_index);
        let mut edit_actions = [("", Action::Dismiss, false); 3];
        let mut edits = 0;
        if !busy {
            for (enabled, label, action, primary) in [
                (selected_change.is_some_and(|change| change.unstaged || change.untracked), "Stage", Action::Stage(true), false),
                (selected_change.is_some_and(|change| change.staged), "Unstage", Action::Stage(false), false),
                (git.changes.iter().any(|change| change.staged), "Commit", Action::Commit, true),
            ] {
                if enabled {
                    edit_actions[edits] = (label, action, primary);
                    edits += 1;
                }
            }
            actions(frame, &mut app.hits, row(area, 1), &edit_actions[..edits]);
            actions(frame, &mut app.hits, row(area, 2), &[
                ("Fetch", Action::Sync("fetch"), false), ("Pull", Action::Sync("pull"), false),
                ("Push", Action::Sync("push"), false), ("↻", Action::Refresh, false),
            ]);
        } else { message(frame, row(area, 1), "Running Git…", MUTED); }
        let list = after(area, 4);
        app.list_area = list;
        if let Some(error) = &app.git_error { message(frame, row(area, 3), error, Color::LightRed); }
        if git.changes.is_empty() { message(frame, list, "Working tree clean.", MUTED); }
        for (y, index) in visible(list, git.changes.len(), app.git_index) {
            let change = &git.changes[index];
            let style = item_style(app.panel_focused, index == app.git_index);
            let flags = match (change.staged, change.unstaged, change.untracked) {
                (_, _, true) => "? ", (true, true, false) => "SW", (true, false, false) => "S ", _ => " W",
            };
            let slot = row(list, y);
            line(frame, slot, Line::from(vec![Span::styled(flags, style.fg(if change.staged { Color::LightGreen } else { Color::LightYellow })), Span::styled(" ", style), Span::styled(safe(&change.path), style)]).style(style));
            app.hits.push((slot, Action::GitRow(index)));
        }
    } else {
        actions(frame, &mut app.hits, row(area, 0), &[("Refresh (r)", Action::Refresh, false)]);
        let text = if busy { "Checking workspace…" } else if let Some(error) = &app.git_error { error } else if app.git_known { "This folder is not in a Git repository." } else { "Git status not loaded." };
        message(frame, after(area, 2), text, if app.git_error.is_some() { Color::LightRed } else { MUTED });
    }
}

fn status(app: &mut App, frame: &mut Frame<'_>, info: Rect, keys: Rect) {
    if let Some(notice) = &app.notice {
        line(frame, info, Line::styled(safe(notice.trim()), Style::new().fg(Color::LightYellow)));
    } else if app.panel_focused {
        let hint = match app.panel {
            Some(Panel::Files) => "↑↓ select · Enter open · o use folder · Tab terminal",
            Some(Panel::Launcher) => "↑↓ select · Enter launch · r rescan · Tab terminal",
            Some(Panel::Git) => "S staged / W working / ? untracked · Enter diff · F1 Git keys",
            None => "",
        };
        line(frame, info, Line::styled(hint, Style::new().fg(MUTED)));
    } else if let Some(tab) = app.tabs.get(app.active) {
        let state = match (&tab.session, tab.exit) {
            (Some(_), _) => Cow::Borrowed("running"),
            (None, Some(code)) => Cow::Owned(format!("exited {code}")),
            (None, None) => Cow::Borrowed("ended"),
        };
        line(frame, info, Line::from(vec![Span::styled(tab.profile.label, Style::new().fg(ACCENT)), Span::raw(" · "), Span::styled(state, Style::new().fg(MUTED)), Span::raw(" · "), Span::styled(safe(&tab.cwd.to_string_lossy()), Style::new().fg(MUTED))]));
    } else {
        line(frame, info, Line::styled("New sessions start in the workspace folder.", Style::new().fg(MUTED)));
    }
    actions(frame, &mut app.hits, keys, &[
        ("Terminal", Action::FocusTerminal, !app.panel_focused), ("F8 Copy", Action::Copy, false), ("F9 Paste", Action::Paste, false),
    ]);
}

fn overlay(app: &mut App, frame: &mut Frame<'_>, screen: Rect) {
    let Some(overlay) = &mut app.overlay else { return; };
    let (label, width, height) = match overlay {
        Overlay::Help => (Cow::Borrowed("Help"), 75, 23),
        Overlay::Read { title, .. } => (safe(title), screen.width.saturating_sub(6), screen.height.saturating_sub(4)),
        Overlay::Commit { .. } => (Cow::Borrowed("Commit staged changes"), 64, 8),
    };
    let width = width.min(screen.width.saturating_sub(2));
    let height = height.min(screen.height.saturating_sub(2));
    let area = Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered().border_style(Style::new().fg(ACCENT)).title(Line::styled(label, Style::new().fg(ACCENT)));
    let inside = block.inner(area);
    frame.render_widget(block, area);
    let close = row(inside, inside.height.saturating_sub(1));
    app.hits.push((close, Action::Dismiss));
    line(frame, close, Line::styled(" Esc Close ", Style::new().fg(ACCENT)));
    let body = Rect::new(inside.x + 1, inside.y, inside.width.saturating_sub(2), inside.height.saturating_sub(1));
    match overlay {
        Overlay::Help => message(frame, body, concat!(
            "F2 Files   F3 Launcher   F4 Git\n",
            "F5 New terminal   F6 Next session   F7 Close session\n",
            "F8 Copy selection   F9 Paste   F10 Quit\n",
            "Ctrl+C interrupts the running program.\n\n",
            "Terminal: drag to select; Shift+PgUp/PgDn scrolls.\n",
            "Files: ↑↓ select, Enter open, Backspace up, o use folder.\n",
            "Launcher: Enter launch, r rescan installed CLIs.\n",
            "Git: Enter working diff, d staged diff, r refresh.\n",
            "Git: s stage, u unstage, c commit, f fetch, p pull, P push.\n",
            "Sidebar: Tab focuses the terminal; Esc hides the sidebar.\n",
            "Previews: arrow keys scroll; F8 copies the whole text."
        ), Color::Reset),
        Overlay::Read { body: text, offset, column, .. } => {
            let count = text.lines().count();
            *offset = (*offset).min(count.saturating_sub(usize::from(body.height)).try_into().unwrap_or(u16::MAX));
            for (i, text) in text.lines().skip(usize::from(*offset)).take(usize::from(body.height)).enumerate() {
                let rendered = safe(text);
                let color = match text.as_bytes().first() { Some(b'+') => Color::LightGreen, Some(b'-') => Color::LightRed, Some(b'@') => ACCENT, _ => Color::Reset };
                frame.render_widget(Paragraph::new(rendered).style(Style::new().fg(color)).scroll((0, *column)), row(body, i as u16));
            }
            if count == 0 { message(frame, body, "No text to display.", MUTED); }
            line(frame, close, Line::styled(" Esc Close · F8 Copy · ↑↓←→ Scroll ", Style::new().fg(ACCENT)));
        }
        Overlay::Commit { message: text } => {
            message(frame, row(body, 0), "Commit message", MUTED);
            let scroll = text.chars().count().saturating_sub(usize::from(body.width.saturating_sub(1))).min(usize::from(u16::MAX)) as u16;
            frame.render_widget(Paragraph::new(text.as_str()).style(selected()).scroll((0, scroll)), row(body, 1));
            if body.height > 2 {
                frame.set_cursor_position((body.x + (text.chars().count() as u16).saturating_sub(scroll).min(body.width.saturating_sub(1)), body.y + 1));
                line(frame, row(body, 2), Line::styled("Enter commits · F9 pastes", Style::new().fg(MUTED)));
            }
        }
    }
}

fn row(area: Rect, y: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(y), area.width, u16::from(y < area.height))
}

fn after(area: Rect, rows: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(rows.min(area.height)), area.width, area.height.saturating_sub(rows))
}

fn visible(area: Rect, len: usize, index: usize) -> impl Iterator<Item = (u16, usize)> {
    let height = usize::from(area.height);
    let start = (index + 1).saturating_sub(height);
    (start..len).take(height).enumerate().map(|(y, index)| (y as u16, index))
}

fn actions(frame: &mut Frame<'_>, hits: &mut Vec<(Rect, Action)>, area: Rect, entries: &[(&str, Action, bool)]) -> u16 {
    if area.is_empty() { return 0; }
    let mut used = 0;
    for &(label, action, active) in entries {
        let width = label.chars().count() as u16 + 2;
        if used + width > area.width { break; }
        let slot = Rect::new(area.x + used, area.y, width, 1);
        let style = if active { selected().add_modifier(Modifier::BOLD) } else { Style::new().fg(MUTED) };
        line(frame, slot, Line::from(vec![Span::raw(" "), Span::raw(label)]).style(style));
        hits.push((slot, action));
        used += width;
    }
    used
}

fn selected() -> Style { Style::new().fg(Color::White).bg(SELECT) }
fn item_style(focused: bool, active: bool) -> Style { if focused && active { selected() } else { Style::new() } }
fn line(frame: &mut Frame<'_>, area: Rect, line: Line<'_>) { frame.render_widget(Paragraph::new(line), area); }
fn message(frame: &mut Frame<'_>, area: Rect, text: &str, color: Color) {
    frame.render_widget(Paragraph::new(safe_multiline(text)).style(Style::new().fg(color)).wrap(Wrap { trim: false }), area);
}

fn safe(text: &str) -> Cow<'_, str> { sanitized(text, false) }
fn safe_multiline(text: &str) -> Cow<'_, str> { sanitized(text, true) }
fn sanitized(text: &str, multiline: bool) -> Cow<'_, str> {
    let unsafe_char = |c: char| c.is_control() && !(multiline && c == '\n');
    if text.chars().any(unsafe_char) {
        Cow::Owned(text.chars().map(|c| if c == '\t' { ' ' } else if unsafe_char(c) { '�' } else { c }).collect())
    } else { Cow::Borrowed(text) }
}
