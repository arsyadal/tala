use crate::{clipboard::Clipboard, config::Config, terminal::{self, Event as PtyEvent, Kind, Profile, Session}, terminal_view::{self, Point, Replies}, workspace::{self, DirectoryEntry, GitStatus}};
use crossterm::event::{Event, KeyCode as Key, KeyEvent, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::{path::PathBuf, sync::mpsc::{self, Receiver, TryRecvError}};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel { Files, Launcher, Git }

#[derive(Clone, Copy)]
pub enum Action {
    Help, Panel(Panel), FocusTerminal, Tab(usize), New, Close, Copy, Paste, Quit,
    Up, UseFolder, Entry(usize), Profile(usize), GitRow(usize), Refresh, Stage(bool),
    Commit, Sync(&'static str), Dismiss,
}

pub struct Tab {
    pub profile: Profile,
    pub cwd: PathBuf,
    pub parser: vt100::Parser<Replies>,
    pub session: Option<Session>,
    pub exit: Option<u32>,
    pub selection: Option<(Point, Point)>,
}

pub enum Overlay {
    Help,
    Read { title: String, body: String, offset: u16, column: u16 },
    Commit { message: String },
}

pub struct App {
    pub folder: PathBuf,
    pub browsing: PathBuf,
    pub entries: Vec<DirectoryEntry>,
    pub files_error: Option<String>,
    pub profiles: Vec<Profile>,
    pub git: Option<GitStatus>,
    pub git_known: bool,
    pub git_error: Option<String>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub panel: Option<Panel>,
    pub panel_focused: bool,
    pub file_index: usize,
    pub profile_index: usize,
    pub git_index: usize,
    pub overlay: Option<Overlay>,
    pub notice: Option<String>,
    pub hits: Vec<(Rect, Action)>,
    pub terminal_area: Rect,
    pub list_area: Rect,
    pub dirty: bool,
    pub quit: bool,
    pub git_work: Option<Receiver<Result<GitReply, String>>>,
    selecting: bool,
    clipboard: Clipboard,
    config: Config,
}

pub enum GitReply {
    Status(Option<GitStatus>, Option<String>),
    Diff(String, String),
}

enum GitJob {
    Refresh, Diff(String, bool), Stage(String, bool), Commit(String), Sync(&'static str),
}

impl App {
    pub fn new(folder: PathBuf, config: Config) -> Result<Self, String> {
        let profiles = terminal::profiles();
        if let Some(id) = &config.default_shell {
            if !profiles.iter().any(|profile| profile.kind == Kind::Shell && profile.id == id) {
                return Err(format!("unknown default shell {id:?}; see tala --list-profiles"));
            }
        }
        let mut app = Self {
            browsing: folder.clone(), folder, entries: Vec::new(), files_error: None,
            profiles, git: None, git_known: false, git_error: None, tabs: Vec::new(), active: 0,
            panel: Some(Panel::Files), panel_focused: false, file_index: 0, profile_index: 0,
            git_index: 0, overlay: None, notice: None, hits: Vec::new(), terminal_area: Rect::ZERO,
            list_area: Rect::ZERO, dirty: true, quit: false, git_work: None, selecting: false,
            clipboard: Clipboard::default(), config,
        };
        app.read_folder();
        app.git_job(GitJob::Refresh);
        Ok(app)
    }

    fn read_folder(&mut self) {
        self.file_index = 0;
        match workspace::list_directory(&self.browsing) {
            Ok(entries) => { self.entries = entries; self.files_error = None; }
            Err(error) => { self.entries.clear(); self.files_error = Some(error); }
        }
    }

    fn choose_folder(&mut self) {
        if self.folder != self.browsing {
            self.folder.clone_from(&self.browsing);
            self.git_work = None;
            self.git = None;
            self.git_known = false;
            self.git_error = None;
            self.git_index = 0;
            self.git_job(GitJob::Refresh);
        }
        self.panel_focused = false;
        self.notice = Some("Workspace set. New sessions will start here.".to_owned());
    }

    fn select_entry(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index) {
            self.file_index = index;
            if entry.directory {
                self.browsing.clone_from(&entry.path);
                self.read_folder();
            } else {
                match workspace::read_text(&entry.path) {
                    Ok(body) => self.overlay = Some(Overlay::Read { title: entry.name.clone(), body, offset: 0, column: 0 }),
                    Err(error) => self.notice = Some(error),
                }
            }
        }
    }

    fn new_shell(&mut self) {
        let preferred = self.config.default_shell.as_deref();
        let login = std::env::var_os("SHELL").map(PathBuf::from);
        let login = login.as_deref().and_then(|path| path.file_stem()).and_then(|stem| stem.to_str());
        let profile = self.profiles.iter().find(|item| item.kind == Kind::Shell && Some(item.id) == preferred)
            .or_else(|| self.profiles.iter().find(|item| item.available && item.kind == Kind::Shell && Some(item.id) == login))
            .or_else(|| self.profiles.iter().find(|item| item.available && item.kind == Kind::Shell));
        if let Some(profile) = profile.cloned() { self.launch(profile); }
        else { self.notice = Some("No supported shell found. See the launcher (F3).".to_owned()); }
    }

    fn launch(&mut self, profile: Profile) {
        if !profile.available {
            self.notice = Some(format!("{} was not found. Install `{}` on PATH, then refresh the launcher (r).", profile.label, profile.command));
            return;
        }
        let (cols, rows) = if self.terminal_area.is_empty() { (80, 24) } else { (self.terminal_area.width, self.terminal_area.height) };
        match Session::start(profile.id, &self.folder, cols, rows) {
            Ok(session) => {
                self.active = self.tabs.len();
                self.tabs.push(Tab { profile, cwd: self.folder.clone(), parser: vt100::Parser::new_with_callbacks(rows, cols, self.config.scrollback, Replies::default()), session: Some(session), exit: None, selection: None });
                self.focus_terminal();
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn focus_terminal(&mut self) { self.panel_focused = false; }

    pub fn fit_terminal(&mut self) -> bool {
        let area = self.terminal_area;
        if area.is_empty() { return false; }
        let Some(tab) = self.tabs.get_mut(self.active) else { return false; };
        let screen = tab.parser.screen_mut();
        if screen.size() == (area.height, area.width) { return false; }
        screen.set_size(area.height, area.width);
        tab.selection = None;
        if let Some(session) = &mut tab.session {
            if let Err(error) = session.resize(area.width, area.height) { self.notice = Some(error); }
        }
        true
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            let Some(session) = &mut tab.session else { continue; };
            for _ in 0..32 {
                match session.events().try_recv() {
                    Ok(PtyEvent::Data(data)) => {
                        tab.parser.process(&data);
                        tab.parser.callbacks_mut().flush(|reply| { let _ = session.write(reply); });
                        if index == self.active { changed = true; }
                    }
                    Ok(PtyEvent::InputError(error)) if index == self.active => { self.notice = Some(error); changed = true; }
                    Ok(PtyEvent::InputError(_)) => {}
                    Ok(PtyEvent::Exit(code)) => { tab.exit = code; tab.session = None; changed = true; break; }
                    Err(TryRecvError::Disconnected) => { tab.session = None; changed = true; break; }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        if let Some(work) = &self.git_work {
            let reply = match work.try_recv() {
                Ok(reply) => Some(reply),
                Err(TryRecvError::Disconnected) => Some(Err("Git worker stopped before reporting a result.".to_owned())),
                Err(TryRecvError::Empty) => None,
            };
            if let Some(reply) = reply {
                self.git_work = None;
                changed = true;
                match reply {
                    Ok(GitReply::Status(status, notice)) => {
                        self.git_index = self.git_index.min(status.as_ref().map_or(0, |item| item.changes.len().saturating_sub(1)));
                        self.git = status;
                        self.git_known = true;
                        self.git_error = None;
                        if let Some(notice) = notice { self.notice = Some(notice); }
                    }
                    Ok(GitReply::Diff(title, body)) => self.overlay = Some(Overlay::Read { title, body, offset: 0, column: 0 }),
                    Err(error) => { self.git_error = Some(error.clone()); self.notice = Some(error); }
                }
            }
        }
        changed
    }

    fn git_job(&mut self, job: GitJob) {
        if self.git_work.is_some() { return; }
        let (sender, receiver) = mpsc::channel();
        let folder = self.folder.clone();
        let result = std::thread::Builder::new().name("tala-git".to_owned()).spawn(move || {
            let reply = run_git(&folder, job);
            let _ = sender.send(reply);
        });
        match result {
            Ok(_) => self.git_work = Some(receiver),
            Err(error) => self.notice = Some(format!("Could not start Git worker: {error}")),
        }
    }

    fn git_diff(&mut self, staged: Option<bool>) {
        if let Some(change) = self.git.as_ref().and_then(|item| item.changes.get(self.git_index)) {
            self.git_job(GitJob::Diff(change.path.clone(), staged.unwrap_or(change.staged && !change.unstaged && !change.untracked)));
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if let Some(session) = tab.session.as_mut() {
                tab.parser.screen_mut().set_scrollback(0);
                tab.selection = None;
                if let Err(error) = session.write(bytes) { self.notice = Some(error); }
            }
        }
    }

    fn paste(&mut self, text: &str) {
        if text.len() > 1024 * 1024 {
            self.notice = Some("Paste is limited to 1 MiB.".to_owned());
            return;
        }
        if let Some(Overlay::Commit { message }) = &mut self.overlay {
            message.extend(text.chars().filter(|ch| !ch.is_control()));
        } else if let Some(tab) = self.tabs.get(self.active) {
            if tab.parser.screen().bracketed_paste() {
                let mut data = Vec::with_capacity(text.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend(text.bytes().filter(|&byte| byte != 0x1b));
                data.extend_from_slice(b"\x1b[201~");
                self.send(&data);
            } else {
                if text.contains('\x1b') {
                    let sanitized = text.replace('\x1b', "");
                    self.send(sanitized.as_bytes());
                } else {
                    self.send(text.as_bytes());
                }
            }
        }
    }

    fn copy(&mut self) {
        let text = match &self.overlay {
            Some(Overlay::Read { body, .. }) => body.clone(),
            _ => {
                let Some(tab) = self.tabs.get(self.active) else { return; };
                let Some((a, b)) = tab.selection else { self.notice = Some("Drag over terminal text to select it first.".to_owned()); return; };
                let ((r1, c1), (r2, c2)) = (a.min(b), a.max(b));
                tab.parser.screen().contents_between(r1, c1, r2, c2.saturating_add(1))
            }
        };
        self.notice = Some(self.clipboard.copy(text).err().unwrap_or_else(|| "Copied to clipboard.".to_owned()));
    }

    fn scroll_terminal(&mut self, delta: isize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let screen = tab.parser.screen_mut();
            screen.set_scrollback(screen.scrollback().saturating_add_signed(delta));
            tab.selection = None;
        }
    }

    pub fn handle(&mut self, event: Event) {
        self.dirty = true;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse),
            Event::Paste(text) if self.overlay.is_none() && !self.panel_focused || matches!(self.overlay, Some(Overlay::Commit { .. })) => self.paste(&text),
            _ => {}
        }
    }

    fn key(&mut self, key: KeyEvent) {
        let modifiers = key.modifiers;
        if modifiers.contains(Mods::CONTROL | Mods::SHIFT) {
            match key.code {
                Key::Char('C' | 'c') => return self.act(Action::Copy),
                Key::Char('V' | 'v') => return self.act(Action::Paste),
                _ => {}
            }
        }
        match key.code {
            Key::F(1) => return self.act(Action::Help),
            Key::F(8) => return self.act(Action::Copy),
            Key::F(9) => return self.act(Action::Paste),
            Key::F(10) => return self.act(Action::Quit),
            _ => {}
        }
        if let Some(overlay) = &mut self.overlay {
            match (overlay, key.code) {
                (_, Key::Esc) => self.overlay = None,
                (Overlay::Read { offset, .. }, Key::Down) => *offset = offset.saturating_add(1),
                (Overlay::Read { offset, .. }, Key::Up) => *offset = offset.saturating_sub(1),
                (Overlay::Read { offset, .. }, Key::PageDown) => *offset = offset.saturating_add(12),
                (Overlay::Read { offset, .. }, Key::PageUp) => *offset = offset.saturating_sub(12),
                (Overlay::Read { column, .. }, Key::Left) => *column = column.saturating_sub(4),
                (Overlay::Read { column, .. }, Key::Right) => *column = column.saturating_add(4),
                (Overlay::Read { offset, .. }, Key::Home) => *offset = 0,
                (Overlay::Commit { message }, Key::Char(ch)) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => message.push(ch),
                (Overlay::Commit { message }, Key::Backspace) => { message.pop(); }
                (Overlay::Commit { message }, Key::Enter) if !message.trim().is_empty() => {
                    let message = std::mem::take(message);
                    self.overlay = None;
                    self.git_job(GitJob::Commit(message));
                }
                _ => {}
            }
            return;
        }
        match key.code {
            Key::F(2) => return self.act(Action::Panel(Panel::Files)),
            Key::F(3) => return self.act(Action::Panel(Panel::Launcher)),
            Key::F(4) => return self.act(Action::Panel(Panel::Git)),
            Key::F(5) => return self.act(Action::New),
            Key::F(6) if !self.tabs.is_empty() => return self.act(Action::Tab((self.active + 1) % self.tabs.len())),
            Key::F(7) => return self.act(Action::Close),
            _ => {}
        }
        if self.panel_focused {
            self.panel_key(key.code);
        } else if self.tabs.is_empty() && key.code == Key::Enter {
            self.act(Action::New);
        } else if modifiers == Mods::SHIFT && matches!(key.code, Key::PageUp | Key::PageDown) {
            let step = self.terminal_area.height.saturating_sub(1).max(1) as isize;
            self.scroll_terminal(if key.code == Key::PageUp { step } else { -step });
        } else if let Some(tab) = self.tabs.get(self.active) {
            if let Some(bytes) = terminal_view::key_bytes(key, tab.parser.screen().application_cursor()) { self.send(&bytes); }
        }
    }

    fn panel_key(&mut self, key: Key) {
        match key {
            Key::Esc => { self.panel = None; self.focus_terminal(); }
            Key::Tab => self.focus_terminal(),
            Key::Up => self.move_index(-1),
            Key::Down => self.move_index(1),
            Key::PageUp => self.move_index(-(self.list_area.height.max(1) as isize)),
            Key::PageDown => self.move_index(self.list_area.height.max(1) as isize),
            _ => match (self.panel, key) {
                (Some(Panel::Files), Key::Enter | Key::Right) => self.act(Action::Entry(self.file_index)),
                (Some(Panel::Files), Key::Backspace | Key::Left) => self.act(Action::Up),
                (Some(Panel::Files), Key::Char('o')) => self.act(Action::UseFolder),
                (Some(Panel::Files), Key::Char('r')) => self.read_folder(),
                (Some(Panel::Launcher), Key::Enter) => self.act(Action::Profile(self.profile_index)),
                (Some(Panel::Launcher), Key::Char('r')) => self.profiles = terminal::profiles(),
                (Some(Panel::Git), Key::Enter) => self.git_diff(None),
                (Some(Panel::Git), Key::Char('d')) => self.git_diff(Some(true)),
                (Some(Panel::Git), Key::Char('s')) => self.act(Action::Stage(true)),
                (Some(Panel::Git), Key::Char('u')) => self.act(Action::Stage(false)),
                (Some(Panel::Git), Key::Char('c')) => self.act(Action::Commit),
                (Some(Panel::Git), Key::Char('r')) => self.act(Action::Refresh),
                (Some(Panel::Git), Key::Char('f')) => self.act(Action::Sync("fetch")),
                (Some(Panel::Git), Key::Char('p')) => self.act(Action::Sync("pull")),
                (Some(Panel::Git), Key::Char('P')) => self.act(Action::Sync("push")),
                _ => {}
            },
        }
    }

    fn move_index(&mut self, delta: isize) {
        let (index, len) = match self.panel {
            Some(Panel::Files) => (&mut self.file_index, self.entries.len()),
            Some(Panel::Launcher) => (&mut self.profile_index, self.profiles.len()),
            Some(Panel::Git) => (&mut self.git_index, self.git.as_ref().map_or(0, |git| git.changes.len())),
            None => return,
        };
        *index = index.saturating_add_signed(delta).min(len.saturating_sub(1));
    }

    fn mouse(&mut self, event: MouseEvent) {
        let x = event.column;
        let y = event.row;
        let inside = |r: Rect| x >= r.x && y >= r.y && x < r.right() && y < r.bottom();
        if let Some(overlay) = &mut self.overlay {
            match event.kind {
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    if let Overlay::Read { offset, .. } = overlay {
                        *offset = offset.saturating_add_signed(if event.kind == MouseEventKind::ScrollUp { -3 } else { 3 });
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.hits.iter().any(|&(r, action)| matches!(action, Action::Dismiss) && inside(r)) { self.act(Action::Dismiss); }
                }
                _ => {}
            }
            return;
        }
        let area = self.terminal_area;
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, action)) = self.hits.iter().rev().find(|(r, _)| inside(*r)).copied() { self.act(action); }
                if inside(area) {
                    self.focus_terminal();
                    let point = (y - area.y, x - area.x);
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        self.selecting = true;
                        tab.selection = Some((point, point));
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) if self.selecting => {
                if !area.is_empty() {
                    if let Some((_, end)) = self.tabs.get_mut(self.active).and_then(|tab| tab.selection.as_mut()) {
                        *end = (y.saturating_sub(area.y).min(area.height - 1), x.saturating_sub(area.x).min(area.width - 1));
                    }
                }
                self.selecting = event.kind != MouseEventKind::Up(MouseButton::Left);
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = event.kind == MouseEventKind::ScrollUp;
                if inside(area) { self.scroll_terminal(if up { 3 } else { -3 }); }
                else if inside(self.list_area) { self.panel_focused = true; self.move_index(if up { -3 } else { 3 }); }
            }
            _ => {}
        }
    }

    fn act(&mut self, action: Action) {
        self.notice = None;
        match action {
            Action::Help => self.overlay = Some(Overlay::Help),
            Action::Panel(panel) => {
                if self.panel == Some(panel) && self.panel_focused { self.panel = None; self.focus_terminal(); }
                else {
                    self.panel = Some(panel);
                    self.panel_focused = true;
                    if panel == Panel::Git { self.git_job(GitJob::Refresh); }
                }
            }
            Action::FocusTerminal => self.focus_terminal(),
            Action::New => self.new_shell(),
            Action::Tab(index) => { self.selecting = false; self.active = index; self.focus_terminal(); }
            Action::Close => {
                if self.active < self.tabs.len() {
                    self.selecting = false;
                    self.tabs.remove(self.active);
                    self.active = self.active.min(self.tabs.len().saturating_sub(1));
                    self.focus_terminal();
                }
            }
            Action::Copy => self.copy(),
            Action::Paste => {
                if self.overlay.is_none() && !self.panel_focused || matches!(self.overlay, Some(Overlay::Commit { .. })) {
                    match self.clipboard.paste() { Ok(text) => self.paste(&text), Err(error) => self.notice = Some(error) }
                }
            }
            Action::Quit => self.quit = true,
            Action::Up => {
                self.panel_focused = true;
                if self.browsing.pop() { self.read_folder(); }
            }
            Action::UseFolder => self.choose_folder(),
            Action::Entry(index) => { self.panel_focused = true; self.select_entry(index); }
            Action::Profile(index) => {
                self.profile_index = index;
                if let Some(profile) = self.profiles.get(index).cloned() { self.launch(profile); }
            }
            Action::GitRow(index) => { self.panel_focused = true; self.git_index = index; self.git_diff(None); }
            Action::Refresh => self.git_job(GitJob::Refresh),
            Action::Stage(stage) => {
                if let Some(change) = self.git.as_ref().and_then(|git| git.changes.get(self.git_index)) {
                    if if stage { change.unstaged || change.untracked } else { change.staged } {
                        self.git_job(GitJob::Stage(change.path.clone(), stage));
                    }
                }
            }
            Action::Commit if self.git_work.is_none() && self.git.as_ref().is_some_and(|git| git.changes.iter().any(|change| change.staged)) => {
                self.overlay = Some(Overlay::Commit { message: String::new() });
            }
            Action::Sync(action) if self.git.is_some() => self.git_job(GitJob::Sync(action)),
            Action::Commit | Action::Sync(_) => {}
            Action::Dismiss => self.overlay = None,
        }
    }
}

fn run_git(folder: &std::path::Path, job: GitJob) -> Result<GitReply, String> {
    let notice = match job {
        GitJob::Refresh => None,
        GitJob::Diff(path, staged) => {
            let body = workspace::git_diff(folder, &path, staged)?;
            let title = format!("{path} · {} diff", if staged { "staged" } else { "working tree" });
            return Ok(GitReply::Diff(title, body));
        }
        GitJob::Stage(path, stage) => {
            workspace::git_stage(folder, &path, stage)?;
            Some(format!("{} {path}", if stage { "Staged" } else { "Unstaged" }))
        }
        GitJob::Commit(message) => Some(workspace::git_commit(folder, &message)?),
        GitJob::Sync(action) => {
            let message = workspace::git_sync(folder, action)?;
            Some(if message.trim().is_empty() { format!("Git {action} completed.") } else { message })
        }
    };
    Ok(GitReply::Status(workspace::git_status(folder)?, notice))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_changes_workspace_only_when_folder_is_chosen() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("project with spaces");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("notes.txt"), "notes from the selected folder").unwrap();
        let mut app = App::new(root.path().to_owned(), Config::default()).unwrap();
        let press = |app: &mut App, code| app.handle(Event::Key(KeyEvent::new(code, Mods::NONE)));
        press(&mut app, Key::F(2));
        press(&mut app, Key::Enter);
        assert_eq!(app.folder, root.path());
        press(&mut app, Key::Char('o'));
        assert_eq!(app.folder, folder);
        press(&mut app, Key::F(2));
        press(&mut app, Key::Enter);
        assert!(matches!(app.overlay, Some(Overlay::Read { body, .. }) if body == "notes from the selected folder"));
    }

    #[test]
    fn interactive_program_can_query_terminal_cursor() {
        let mut terminal = vt100::Parser::new_with_callbacks(20, 40, 0, Replies::default());
        terminal.process(b"\x1b[4;12H\x1b[6n");
        let mut response = Vec::new();
        terminal.callbacks_mut().flush(|reply| response.extend_from_slice(reply));
        assert_eq!(response, b"\x1b[4;12R");
    }
}
