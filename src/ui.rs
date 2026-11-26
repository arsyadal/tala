use crate::{
    app::{Action, App, Overlay, PALETTE_ITEMS, Panel, SplitDirection},
    editor,
    terminal::Kind,
    terminal_view,
    theme::Theme,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::borrow::Cow;

pub fn draw(app: &mut App, frame: &mut Frame<'_>) {
    let theme = app.theme();
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
        Span::styled(" tala  ", Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("/  ", Style::new().fg(theme.quiet)),
        Span::styled(safe(&app.folder.to_string_lossy()), Style::new().fg(theme.fg)),
        Span::styled(format!("  [{}]", app.theme_name.label()), Style::new().fg(theme.quiet)),
    ]));
    actions(frame, &mut app.hits, row(area, 1), theme, &[
        ("F2 Files", Action::Panel(Panel::Files), app.panel == Some(Panel::Files)),
        ("F3 Launcher", Action::Panel(Panel::Launcher), app.panel == Some(Panel::Launcher)),
        ("F4 Git", Action::Panel(Panel::Git), app.panel == Some(Panel::Git)),
        ("Palette", Action::Palette, matches!(app.overlay, Some(Overlay::Palette { .. }))),
        ("F1 Help", Action::Help, false),
        ("F10 Quit", Action::Quit, false),
    ]);

    let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
    let main = match app.panel {
        Some(panel) if area.width >= 77 => {
            let width = if area.width >= 120 { 37 } else { 30 };
            sidebar(app, frame, Rect::new(body.x, body.y, width, body.height), panel, theme);
            Rect::new(body.x + width, body.y, body.width - width, body.height)
        }
        Some(panel) if app.panel_focused => {
            sidebar(app, frame, body, panel, theme);
            Rect::ZERO
        }
        _ => body,
    };
    if !main.is_empty() {
        terminal(app, frame, main, theme);
    }
    status(app, frame, row(area, area.height - 2), row(area, area.height - 1), theme);
    if app.overlay.is_some() {
        overlay(app, frame, area, theme);
    }
}

fn terminal(app: &mut App, frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let block = Block::new().borders(Borders::TOP).border_style(Style::new().fg(theme.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let tabs = row(inner, 0);
    let mut strip = tabs;
    let tools: &[(&str, Action, bool)] = if area.width >= 72 {
        &[
            ("F5 New", Action::New, false),
            ("Split |", Action::Split(SplitDirection::Vertical), false),
            ("Split -", Action::Split(SplitDirection::Horizontal), false),
            ("F7 Close", Action::Close, false),
        ]
    } else if area.width >= 50 {
        &[
            ("F5 New", Action::New, false),
            ("Split", Action::Split(SplitDirection::Vertical), false),
            ("F7 Close", Action::Close, false),
        ]
    } else {
        &[("+", Action::New, false), ("x", Action::Close, false)]
    };
    let used = actions(frame, &mut app.hits, tabs, theme, tools);
    strip.x += used;
    strip.width = strip.width.saturating_sub(used);
    if app.tabs.is_empty() {
        line(frame, strip, Line::styled("  TERMINAL", Style::new().fg(theme.quiet)));
    } else {
        let slots = usize::from((strip.width / 16).max(1));
        let first = (app.active + 1).saturating_sub(slots);
        for (index, tab) in app.tabs.iter().enumerate().skip(first).take(slots) {
            if strip.width < 4 { break; }
            let width = strip.width.min(18);
            let slot = Rect::new(strip.x, strip.y, width, 1);
            let style = if index == app.active { selected(theme) } else { Style::new().fg(theme.muted) };
            let marker = if tab.any_running() { " " } else { "·" };
            line(frame, slot, Line::styled(format!(" {} {marker}{}", index + 1, tab.label()), style));
            app.hits.push((slot, Action::Tab(index)));
            strip.x += width;
            strip.width = strip.width.saturating_sub(width);
        }
    }

    let content = Rect::new(inner.x, inner.y + 1, inner.width, inner.height.saturating_sub(1));
    app.terminal_area = content;
    let focused_terminal = !app.panel_focused && app.overlay.is_none();
    if let Some(tab) = app.tabs.get_mut(app.active) {
        let count = tab.panes.len();
        if count == 1 {
            let pane = &mut tab.panes[0];
            pane.area = content;
            terminal_view::draw(
                frame,
                content,
                pane.parser.screen(),
                pane.selection,
                focused_terminal && pane.session.is_some(),
            );
        } else if count > 1 && !content.is_empty() {
            let active_pane = tab.active_pane;
            let rects = split_rects(content, count, tab.split);
            for (idx, (pane, pane_rect)) in tab.panes.iter_mut().zip(rects).enumerate() {
                let is_active = idx == active_pane;
                let border_color = if is_active && focused_terminal { theme.accent } else { theme.border };
                let state = if pane.session.is_some() { "" } else { " · exited" };
                let title = format!(" {} {}{state} ", idx + 1, pane.profile.label);
                let pane_block = Block::bordered()
                    .border_style(Style::new().fg(border_color))
                    .title(Line::styled(
                        title,
                        Style::new().fg(if is_active { theme.accent } else { theme.muted }),
                    ));
                let inner_pane = pane_block.inner(pane_rect);
                frame.render_widget(pane_block, pane_rect);
                app.hits.push((row(pane_rect, 0), Action::Pane(idx)));
                pane.area = inner_pane;
                terminal_view::draw(
                    frame,
                    inner_pane,
                    pane.parser.screen(),
                    pane.selection,
                    focused_terminal && is_active && pane.session.is_some(),
                );
            }
        }
    } else {
        let width = content.width.saturating_sub(4).min(56);
        let x = content.x + (content.width.saturating_sub(width)) / 2;
        let y = content.y + content.height.saturating_sub(7) / 2;
        let welcome = Rect::new(x, y, width, content.bottom().saturating_sub(y).min(7));
        line(frame, row(welcome, 0), Line::styled("tala", Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)));
        line(frame, row(welcome, 1), Line::styled("Find your flow.", Style::new().fg(theme.fg)));
        if welcome.height > 3 {
            actions(frame, &mut app.hits, row(welcome, 3), theme, &[
                ("F5 New terminal", Action::New, true),
                ("F3 Launcher", Action::Panel(Panel::Launcher), false),
                ("Ctrl+P Files", Action::QuickOpen, false),
            ]);
        }
        if welcome.height > 5 {
            line(frame, row(welcome, 5), Line::styled("Ctrl+Shift+P Command Palette · Ctrl+Shift+O Workspaces", Style::new().fg(theme.muted)));
        }
    }
}

fn split_rects(area: Rect, count: usize, direction: SplitDirection) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(count);
    let total = match direction {
        SplitDirection::Vertical => area.width,
        SplitDirection::Horizontal => area.height,
    };
    let base = total / (count as u16);
    let rem = total % (count as u16);
    let mut cursor = match direction {
        SplitDirection::Vertical => area.x,
        SplitDirection::Horizontal => area.y,
    };
    for i in 0..count {
        let size = base + u16::from((i as u16) < rem);
        let rect = match direction {
            SplitDirection::Vertical => Rect::new(cursor, area.y, size, area.height),
            SplitDirection::Horizontal => Rect::new(area.x, cursor, area.width, size),
        };
        rects.push(rect);
        cursor = cursor.saturating_add(size);
    }
    rects
}

fn sidebar(app: &mut App, frame: &mut Frame<'_>, area: Rect, panel: Panel, theme: Theme) {
    let active = app.panel_focused && app.overlay.is_none();
    let title = match panel {
        Panel::Files => " FILES ",
        Panel::Launcher => " LAUNCHER ",
        Panel::Git => " SOURCE CONTROL ",
    };
    let block = Block::new()
        .borders(Borders::TOP | Borders::RIGHT)
        .border_style(Style::new().fg(if active { theme.accent } else { theme.border }))
        .title(Line::styled(title, Style::new().fg(if active { theme.accent } else { theme.muted }).add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let area = Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), inner.height);
    if area.height < 3 { return; }
    match panel {
        Panel::Files => files(app, frame, area, theme),
        Panel::Launcher => launcher(app, frame, area, theme),
        Panel::Git => source_control(app, frame, area, theme),
    }
}

fn files(app: &mut App, frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    line(frame, row(area, 0), Line::styled(safe(&app.browsing.to_string_lossy()), Style::new().fg(theme.muted)));
    actions(frame, &mut app.hits, row(area, 1), theme, &[
        ("Up", Action::Up, false),
        ("Use (o)", Action::UseFolder, app.folder != app.browsing),
        ("+File", Action::NewFile, false),
        ("+Dir", Action::NewFolder, false),
    ]);
    let list = after(area, 3);
    app.list_area = list;
    if let Some(error) = &app.files_error {
        message(frame, list, error, theme.danger);
    } else if app.entries.is_empty() {
        message(frame, list, "This folder is empty. Press n for a new file.", theme.muted);
    }
    for (y, index) in visible(list, app.entries.len(), app.file_index) {
        let entry = &app.entries[index];
        let style = item_style(app.panel_focused, index == app.file_index, theme);
        let slot = row(list, y);
        line(frame, slot, Line::from(vec![
            Span::styled(if entry.directory { "/ " } else { "  " }, style.fg(if entry.directory { theme.accent } else { theme.muted })),
            Span::styled(safe(&entry.name), style),
        ]).style(style));
        app.hits.push((slot, Action::Entry(index)));
    }
}

fn launcher(app: &mut App, frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    line(frame, row(area, 0), Line::styled("Enter to launch · r to rescan", Style::new().fg(theme.muted)));
    let list = after(area, 2);
    app.list_area = list;
    for (y, index) in visible(list, app.profiles.len(), app.profile_index) {
        let profile = &app.profiles[index];
        let style = item_style(app.panel_focused, index == app.profile_index, theme);
        let slot = row(list, y);
        let marker = if profile.kind == Kind::Shell { "> " } else { "* " };
        line(frame, slot, Line::from(vec![
            Span::styled(marker, style.fg(theme.accent)),
            Span::styled(profile.label.as_str(), style),
            Span::styled(if profile.available { "" } else { "  not found" }, style.fg(theme.muted)),
        ]).style(style));
        app.hits.push((slot, Action::Profile(index)));
    }
}

fn source_control(app: &mut App, frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let busy = app.git_work.is_some();
    if let Some(git) = &app.git {
        line(frame, row(area, 0), Line::from(vec![
            Span::styled("branch ", Style::new().fg(theme.muted)),
            Span::styled(safe(&git.branch), Style::new().fg(theme.accent)),
        ]));
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
            actions(frame, &mut app.hits, row(area, 1), theme, &edit_actions[..edits]);
            actions(frame, &mut app.hits, row(area, 2), theme, &[
                ("Fetch", Action::Sync("fetch"), false),
                ("Pull", Action::Sync("pull"), false),
                ("Push", Action::Sync("push"), false),
                ("Branch", Action::GitBranches, false),
            ]);
            actions(frame, &mut app.hits, row(area, 3), theme, &[
                ("Stash", Action::GitStashPush, false),
                ("Stashes", Action::GitStashes, false),
                ("Trees", Action::GitWorktrees, false),
            ]);
        } else {
            message(frame, row(area, 1), "Running Git...", theme.muted);
        }
        let list = after(area, 5);
        app.list_area = list;
        if let Some(error) = &app.git_error {
            message(frame, row(area, 4), error, theme.danger);
        }
        if git.changes.is_empty() {
            message(frame, list, "Working tree clean.", theme.muted);
        }
        for (y, index) in visible(list, git.changes.len(), app.git_index) {
            let change = &git.changes[index];
            let style = item_style(app.panel_focused, index == app.git_index, theme);
            let flags = match (change.staged, change.unstaged, change.untracked) {
                (_, _, true) => "? ",
                (true, true, false) => "SW",
                (true, false, false) => "S ",
                _ => " W",
            };
            let slot = row(list, y);
            line(frame, slot, Line::from(vec![
                Span::styled(flags, style.fg(if change.staged { theme.success } else { theme.warn })),
                Span::styled(" ", style),
                Span::styled(safe(&change.path), style),
            ]).style(style));
            app.hits.push((slot, Action::GitRow(index)));
        }
    } else {
        actions(frame, &mut app.hits, row(area, 0), theme, &[("Refresh (r)", Action::Refresh, false)]);
        let text = if busy {
            "Checking workspace..."
        } else if let Some(error) = &app.git_error {
            error
        } else if app.git_known {
            "This folder is not in a Git repository."
        } else {
            "Git status not loaded."
        };
        message(frame, after(area, 2), text, if app.git_error.is_some() { theme.danger } else { theme.muted });
    }
}

fn status(app: &mut App, frame: &mut Frame<'_>, info: Rect, keys: Rect, theme: Theme) {
    if let Some(notice) = &app.notice {
        line(frame, info, Line::styled(safe(notice.trim()), Style::new().fg(theme.warn)));
    } else if app.panel_focused {
        let hint = match app.panel {
            Some(Panel::Files) => "Enter open · n file · N dir · R rename · d del · o workspace",
            Some(Panel::Launcher) => "Enter launch · r rescan · Tab terminal",
            Some(Panel::Git) => "Enter diff · s/u stage · c commit · b branch · S/t stash · w worktree",
            None => "",
        };
        line(frame, info, Line::styled(hint, Style::new().fg(theme.muted)));
    } else if let Some(pane) = app.active_pane() {
        let state = match (&pane.session, pane.exit) {
            (Some(_), _) => Cow::Borrowed("running"),
            (None, Some(code)) => Cow::Owned(format!("exited {code}")),
            (None, None) => Cow::Borrowed("ended"),
        };
        line(frame, info, Line::from(vec![
            Span::styled(pane.profile.label.as_str(), Style::new().fg(theme.accent)),
            Span::raw(" · "),
            Span::styled(state, Style::new().fg(theme.muted)),
            Span::raw(" · "),
            Span::styled(safe(&pane.cwd.to_string_lossy()), Style::new().fg(theme.muted)),
        ]));
    } else {
        line(frame, info, Line::styled("New sessions start in the workspace folder.", Style::new().fg(theme.muted)));
    }
    actions(frame, &mut app.hits, keys, theme, &[
        ("Terminal", Action::FocusTerminal, !app.panel_focused),
        ("F8 Copy", Action::Copy, false),
        ("F9 Paste", Action::Paste, false),
        ("Find", Action::SearchTerminal, false),
        ("Files", Action::QuickOpen, false),
        ("Workspaces", Action::Workspaces, false),
    ]);
}

fn overlay(app: &mut App, frame: &mut Frame<'_>, screen: Rect, theme: Theme) {
    let folder = app.folder.clone();
    let Some(overlay) = &mut app.overlay else { return; };
    let (label, width, height) = match overlay {
        Overlay::Help => (Cow::Borrowed("Help"), 78, 25),
        Overlay::Editor(ed) => {
            let mode = if ed.is_diff {
                ""
            } else if ed.editing {
                if ed.dirty { " [EDIT*]" } else { " [EDIT]" }
            } else {
                " [READ-ONLY]"
            };
            (Cow::Owned(format!("{}{mode}", safe(&ed.title))), screen.width.saturating_sub(6), screen.height.saturating_sub(4))
        }
        Overlay::Input { title, .. } => (safe(title), 66, 8),
        Overlay::Confirm { title, .. } => (safe(title), 66, 8),
        Overlay::Palette { .. } => (Cow::Borrowed("Command Palette"), 70, 18),
        Overlay::QuickOpen { .. } => (Cow::Borrowed("Quick Open File"), 72, 18),
        Overlay::Workspaces { .. } => (Cow::Borrowed("Switch Workspace"), 72, 16),
        Overlay::TerminalSearch { .. } => (Cow::Borrowed("Search Terminal Output"), 76, 18),
        Overlay::Branches { .. } => (Cow::Borrowed("Git Branches"), 62, 16),
        Overlay::Stashes { .. } => (Cow::Borrowed("Git Stashes"), 72, 16),
        Overlay::Worktrees { .. } => (Cow::Borrowed("Git Worktrees"), 72, 16),
    };
    let width = width.min(screen.width.saturating_sub(2));
    let height = height.min(screen.height.saturating_sub(2));
    let area = Rect::new(screen.x + (screen.width - width) / 2, screen.y + (screen.height - height) / 2, width, height);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_style(Style::new().fg(theme.accent))
        .title(Line::styled(label, Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)));
    let inside = block.inner(area);
    frame.render_widget(block, area);
    let close = row(inside, inside.height.saturating_sub(1));
    app.hits.push((close, Action::Dismiss));
    line(frame, close, Line::styled(" Esc Close ", Style::new().fg(theme.accent)));
    let body = Rect::new(inside.x + 1, inside.y, inside.width.saturating_sub(2), inside.height.saturating_sub(1));

    match overlay {
        Overlay::Help => message(frame, body, concat!(
            "Global: F1 Help   Ctrl+Shift+P Palette   Ctrl+P Quick Open   Ctrl+Shift+O Workspaces\n",
            "Panels: F2 Files   F3 Launcher (Ctrl+Shift+A)   F4 Git (Ctrl+Shift+G)   Ctrl+B Sidebar\n",
            "Terminal: F5 New (Ctrl+Shift+T)   F6 Next tab   F7 Close pane/tab   F10 Quit\n",
            "Splits: Ctrl+Shift+D Vertical split   Ctrl+Shift+E Horizontal split   Alt+Left/Right Pane\n",
            "Search & Clipboard: Ctrl+Shift+F Search output   F8/Ctrl+Shift+C Copy   F9/Ctrl+Shift+V Paste\n",
            "Ctrl+C sends interrupt to the active terminal process.\n\n",
            "Files: Enter open, Backspace up, o set workspace, n new file, N new dir, R rename, d delete.\n",
            "Editor: e toggle edit mode, Ctrl+S save, arrows scroll/move, F8 copy all.\n",
            "Launcher: Enter launch, r rescan PATH (supports [[agents]] in config.toml).\n",
            "Git: Enter diff, d staged diff, s/u stage/unstage, c commit, f/p/P fetch/pull/push.\n",
            "Git extra: b branches, S stash push, t stash list/pop, w worktrees."
        ), theme.fg),

        Overlay::Editor(ed) => {
            let gutter_width = ed.lines.len().max(1).to_string().len() as u16 + 2;
            let text_width = body.width.saturating_sub(gutter_width);
            ed.sync_viewport(usize::from(body.height), usize::from(text_width));
            if ed.lines.is_empty() {
                message(frame, body, "No text to display.", theme.muted);
            } else {
                for (i, line_idx) in (ed.scroll_y..ed.lines.len()).take(usize::from(body.height)).enumerate() {
                    let row_rect = row(body, i as u16);
                    let gutter = format!("{:>w$} ", line_idx + 1, w = usize::from(gutter_width.saturating_sub(1)));
                    let mut spans = vec![Span::styled(gutter, Style::new().fg(theme.quiet))];
                    let raw = &ed.lines[line_idx];
                    let safe_text = safe(raw);
                    let visible_text: String = safe_text.chars().skip(ed.scroll_x).collect();
                    for (segment, color) in editor::highlight_line(&visible_text, &ed.extension, ed.is_diff, &theme) {
                        spans.push(Span::styled(segment, Style::new().fg(color)));
                    }
                    frame.render_widget(Paragraph::new(Line::from(spans)), row_rect);
                }
                if ed.editing && ed.cursor_row >= ed.scroll_y && ed.cursor_row < ed.scroll_y + usize::from(body.height) {
                    let cy = body.y + (ed.cursor_row - ed.scroll_y) as u16;
                    let cx = body.x + gutter_width + ed.cursor_col.saturating_sub(ed.scroll_x) as u16;
                    if cx < body.right() && cy < body.bottom() {
                        frame.set_cursor_position((cx, cy));
                    }
                }
            }
            let footer = if ed.is_diff {
                " Esc Close · F8 Copy · Arrows Scroll "
            } else if ed.editing {
                " Ctrl+S Save · Esc Close · F8 Copy · F9 Paste "
            } else {
                " e Edit · Esc Close · F8 Copy · Arrows Scroll "
            };
            line(frame, close, Line::styled(footer, Style::new().fg(theme.accent)));
        }

        Overlay::Input { hint, value, .. } => {
            message(frame, row(body, 0), hint, theme.muted);
            let scroll = value.chars().count().saturating_sub(usize::from(body.width.saturating_sub(1))).min(usize::from(u16::MAX)) as u16;
            frame.render_widget(Paragraph::new(value.as_str()).style(selected(theme)).scroll((0, scroll)), row(body, 2));
            frame.set_cursor_position((
                body.x + (value.chars().count() as u16).saturating_sub(scroll).min(body.width.saturating_sub(1)),
                body.y + 2,
            ));
            line(frame, close, Line::styled(" Enter Confirm · Esc Cancel ", Style::new().fg(theme.accent)));
        }

        Overlay::Confirm { message: text, .. } => {
            message(frame, body, text, theme.warn);
            line(frame, close, Line::styled(" Enter / y Confirm · Esc Cancel ", Style::new().fg(theme.accent)));
        }

        Overlay::Palette { query, selected: sel } => {
            render_filter_input(frame, body, "> ", query, theme);
            let filtered = App::filtered_palette_indices(query);
            let list = after(body, 2);
            if filtered.is_empty() {
                message(frame, list, "No matching commands.", theme.muted);
            } else {
                for (y, idx) in visible(list, filtered.len(), *sel) {
                    let item = &PALETTE_ITEMS[filtered[idx]];
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::from(vec![
                        Span::styled(item.label, style),
                        Span::styled(format!("  [{}]", item.shortcut), if idx == *sel { style } else { Style::new().fg(theme.muted) }),
                    ]).style(style));
                }
            }
            line(frame, close, Line::styled(" Enter Run · Arrows Move · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::QuickOpen { query, files, selected: sel } => {
            render_filter_input(frame, body, "File: ", query, theme);
            let needle = query.trim().to_ascii_lowercase();
            let filtered: Vec<&std::path::PathBuf> = files
                .iter()
                .filter(|path| {
                    needle.is_empty()
                        || path.strip_prefix(&folder).unwrap_or(path).to_string_lossy().to_ascii_lowercase().contains(&needle)
                })
                .collect();
            let list = after(body, 2);
            if filtered.is_empty() {
                message(frame, list, "No matching files in workspace.", theme.muted);
            } else {
                for (y, idx) in visible(list, filtered.len(), *sel) {
                    let rel = filtered[idx].strip_prefix(&folder).unwrap_or(filtered[idx]);
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::styled(safe(&rel.to_string_lossy()), style));
                }
            }
            line(frame, close, Line::styled(" Enter Open · Arrows Move · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::Workspaces { input, folders, selected: sel } => {
            render_filter_input(frame, body, "Path: ", input, theme);
            let list = after(body, 2);
            if folders.is_empty() {
                message(frame, list, "Type a folder path above and press Enter.", theme.muted);
            } else {
                for (y, idx) in visible(list, folders.len(), *sel) {
                    let style = if idx == *sel && input.trim().is_empty() { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::styled(safe(&folders[idx].to_string_lossy()), style));
                }
            }
            line(frame, close, Line::styled(" Enter Switch · Type custom path · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::TerminalSearch { query, matches, selected: sel } => {
            render_filter_input(frame, body, "Find: ", query, theme);
            let list = after(body, 2);
            if matches.is_empty() {
                message(frame, list, "No matching terminal lines.", theme.muted);
            } else {
                for (y, idx) in visible(list, matches.len(), *sel) {
                    let (offset, text) = &matches[idx];
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::from(vec![
                        Span::styled(format!("-{offset:<4} "), if idx == *sel { style } else { Style::new().fg(theme.muted) }),
                        Span::styled(safe(text), style),
                    ]).style(style));
                }
            }
            line(frame, close, Line::styled(" Enter Jump · F8 Copy line · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::Branches { branches, selected: sel } => {
            let list = body;
            if branches.is_empty() {
                message(frame, list, "No local branches found.", theme.muted);
            } else {
                for (y, idx) in visible(list, branches.len(), *sel) {
                    let b = &branches[idx];
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    let marker = if b.current { "* " } else { "  " };
                    line(frame, row(list, y), Line::from(vec![
                        Span::styled(marker, if b.current { style.fg(theme.success) } else { style }),
                        Span::styled(safe(&b.name), style),
                    ]).style(style));
                }
            }
            line(frame, close, Line::styled(" Enter Switch · n New branch · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::Stashes { stashes, selected: sel } => {
            let list = body;
            if stashes.is_empty() {
                message(frame, list, "No stashes in this repository.", theme.muted);
            } else {
                for (y, idx) in visible(list, stashes.len(), *sel) {
                    let s = &stashes[idx];
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::from(vec![
                        Span::styled(safe(&s.reference), if idx == *sel { style } else { Style::new().fg(theme.accent) }),
                        Span::styled("  ", style),
                        Span::styled(safe(&s.summary), style),
                    ]).style(style));
                }
            }
            line(frame, close, Line::styled(" Enter Pop selected stash · Esc Close ", Style::new().fg(theme.accent)));
        }

        Overlay::Worktrees { worktrees, selected: sel } => {
            let list = body;
            if worktrees.is_empty() {
                message(frame, list, "No worktrees found.", theme.muted);
            } else {
                for (y, idx) in visible(list, worktrees.len(), *sel) {
                    let wt = &worktrees[idx];
                    let style = if idx == *sel { selected(theme) } else { Style::new().fg(theme.fg) };
                    line(frame, row(list, y), Line::from(vec![
                        Span::styled(format!("[{}] ", safe(&wt.branch)), if idx == *sel { style } else { Style::new().fg(theme.accent) }),
                        Span::styled(safe(&wt.path.to_string_lossy()), style),
                    ]).style(style));
                }
            }
            line(frame, close, Line::styled(" Enter Open worktree as workspace · Esc Close ", Style::new().fg(theme.accent)));
        }
    }
}

fn render_filter_input(frame: &mut Frame<'_>, body: Rect, prefix: &str, value: &str, theme: Theme) {
    let input_row = row(body, 0);
    let prefix_len = prefix.chars().count() as u16;
    let available = body.width.saturating_sub(prefix_len);
    let scroll = value.chars().count().saturating_sub(usize::from(available.saturating_sub(1))).min(usize::from(u16::MAX)) as u16;
    line(frame, input_row, Line::from(vec![
        Span::styled(prefix, Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(value, selected(theme)),
    ]));
    frame.set_cursor_position((
        body.x + prefix_len + (value.chars().count() as u16).saturating_sub(scroll).min(available.saturating_sub(1)),
        body.y,
    ));
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

fn actions(frame: &mut Frame<'_>, hits: &mut Vec<(Rect, Action)>, area: Rect, theme: Theme, entries: &[(&str, Action, bool)]) -> u16 {
    if area.is_empty() { return 0; }
    let mut used = 0;
    for &(label, action, active) in entries {
        let width = label.chars().count() as u16 + 2;
        if used + width > area.width { break; }
        let slot = Rect::new(area.x + used, area.y, width, 1);
        let style = if active { selected(theme).add_modifier(Modifier::BOLD) } else { Style::new().fg(theme.muted) };
        line(frame, slot, Line::from(vec![Span::raw(" "), Span::raw(label)]).style(style));
        hits.push((slot, action));
        used += width;
    }
    used
}

fn selected(theme: Theme) -> Style { Style::new().fg(theme.select_fg).bg(theme.select_bg) }
fn item_style(focused: bool, active: bool, theme: Theme) -> Style {
    if focused && active { selected(theme) } else { Style::new().fg(theme.fg) }
}
fn line(frame: &mut Frame<'_>, area: Rect, line: Line<'_>) { frame.render_widget(Paragraph::new(line), area); }
fn message(frame: &mut Frame<'_>, area: Rect, text: &str, color: Color) {
    frame.render_widget(Paragraph::new(safe_multiline(text)).style(Style::new().fg(color)).wrap(Wrap { trim: false }), area);
}

fn safe(text: &str) -> Cow<'_, str> { sanitized(text, false) }
fn safe_multiline(text: &str) -> Cow<'_, str> { sanitized(text, true) }
fn sanitized(text: &str, multiline: bool) -> Cow<'_, str> {
    let unsafe_char = |c: char| c.is_control() && !(multiline && c == '\n');
    if text.chars().any(unsafe_char) {
        Cow::Owned(text.chars().map(|c| if c == '\t' { ' ' } else if unsafe_char(c) { '\u{FFFD}' } else { c }).collect())
    } else {
        Cow::Borrowed(text)
    }
}
