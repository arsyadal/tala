use crate::{
    clipboard::Clipboard,
    config::{Config, WorkspaceState},
    editor::EditorState,
    terminal::{self, Event as PtyEvent, Kind, Profile, Session},
    terminal_view::{self, Point, Replies},
    theme::{Theme, ThemeName},
    workspace::{self, DirectoryEntry, GitBranch, GitStash, GitStatus, GitWorktree},
};
use crossterm::event::{Event, KeyCode as Key, KeyEvent, KeyEventKind, KeyModifiers as Mods, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

const MAX_PANES_PER_TAB: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel { Files, Launcher, Git }

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection { Vertical, Horizontal }

#[derive(Clone, Copy)]
pub enum Action {
    Help,
    Palette,
    QuickOpen,
    Workspaces,
    ToggleSidebar,
    Panel(Panel),
    FocusTerminal,
    Tab(usize),
    Pane(usize),
    NextPane,
    New,
    Split(SplitDirection),
    Close,
    SearchTerminal,
    Copy,
    Paste,
    CycleTheme,
    Quit,
    Up,
    UseFolder,
    NewFile,
    NewFolder,
    RenameEntry,
    DeleteEntry,
    Entry(usize),
    Profile(usize),
    GitRow(usize),
    Refresh,
    Stage(bool),
    Commit,
    Sync(&'static str),
    GitBranches,
    GitStashes,
    GitStashPush,
    GitWorktrees,
    Dismiss,
}

pub struct Pane {
    pub profile: Profile,
    pub cwd: PathBuf,
    pub parser: vt100::Parser<Replies>,
    pub session: Option<Session>,
    pub exit: Option<u32>,
    pub selection: Option<(Point, Point)>,
    pub area: Rect,
}

pub struct Tab {
    pub panes: Vec<Pane>,
    pub active_pane: usize,
    pub split: SplitDirection,
}

impl Tab {
    pub fn active(&self) -> Option<&Pane> {
        self.panes.get(self.active_pane)
    }

    pub fn active_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(self.active_pane)
    }

    pub fn label(&self) -> String {
        let primary = self.active().map_or("Shell", |pane| pane.profile.label.as_str());
        if self.panes.len() > 1 {
            format!("{primary} [{}]", self.panes.len())
        } else {
            primary.to_owned()
        }
    }

    pub fn any_running(&self) -> bool {
        self.panes.iter().any(|pane| pane.session.is_some())
    }
}

#[derive(Clone)]
pub enum InputKind {
    Commit,
    NewFile,
    NewFolder,
    Rename(PathBuf),
    NewBranch,
    StashMessage,
}

#[derive(Clone)]
pub enum ConfirmKind {
    Delete(PathBuf),
    DiscardEditor,
}

pub struct PaletteItem {
    pub label: &'static str,
    pub shortcut: &'static str,
    pub action: Action,
}

pub const PALETTE_ITEMS: &[PaletteItem] = &[
    PaletteItem { label: "New terminal tab", shortcut: "F5 / Ctrl+Shift+T", action: Action::New },
    PaletteItem { label: "Split terminal vertically", shortcut: "Ctrl+Shift+D", action: Action::Split(SplitDirection::Vertical) },
    PaletteItem { label: "Split terminal horizontally", shortcut: "Ctrl+Shift+E", action: Action::Split(SplitDirection::Horizontal) },
    PaletteItem { label: "Focus next split pane", shortcut: "Alt+Right", action: Action::NextPane },
    PaletteItem { label: "Close active pane or tab", shortcut: "F7", action: Action::Close },
    PaletteItem { label: "Search terminal output", shortcut: "Ctrl+Shift+F", action: Action::SearchTerminal },
    PaletteItem { label: "Quick open file", shortcut: "Ctrl+P", action: Action::QuickOpen },
    PaletteItem { label: "Switch workspace / recent folders", shortcut: "Ctrl+Shift+O", action: Action::Workspaces },
    PaletteItem { label: "Toggle sidebar", shortcut: "Ctrl+B", action: Action::ToggleSidebar },
    PaletteItem { label: "Open Files panel", shortcut: "F2", action: Action::Panel(Panel::Files) },
    PaletteItem { label: "Open Launcher panel", shortcut: "F3 / Ctrl+Shift+A", action: Action::Panel(Panel::Launcher) },
    PaletteItem { label: "Open Git panel", shortcut: "F4 / Ctrl+Shift+G", action: Action::Panel(Panel::Git) },
    PaletteItem { label: "Git: Branches (switch or create)", shortcut: "b in Git", action: Action::GitBranches },
    PaletteItem { label: "Git: Stash push", shortcut: "S in Git", action: Action::GitStashPush },
    PaletteItem { label: "Git: Stash list (pop)", shortcut: "t in Git", action: Action::GitStashes },
    PaletteItem { label: "Git: Worktrees", shortcut: "w in Git", action: Action::GitWorktrees },
    PaletteItem { label: "Git: Fetch", shortcut: "f in Git", action: Action::Sync("fetch") },
    PaletteItem { label: "Git: Pull (--ff-only)", shortcut: "p in Git", action: Action::Sync("pull") },
    PaletteItem { label: "Git: Push", shortcut: "P in Git", action: Action::Sync("push") },
    PaletteItem { label: "Switch color theme", shortcut: "Theme", action: Action::CycleTheme },
    PaletteItem { label: "Show keyboard shortcuts", shortcut: "F1", action: Action::Help },
    PaletteItem { label: "Quit Tala", shortcut: "F10", action: Action::Quit },
];

pub enum Overlay {
    Help,
    Editor(EditorState),
    Input { title: String, hint: String, value: String, kind: InputKind },
    Confirm { title: String, message: String, kind: ConfirmKind },
    Palette { query: String, selected: usize },
    QuickOpen { query: String, files: Vec<PathBuf>, selected: usize },
    Workspaces { input: String, folders: Vec<PathBuf>, selected: usize },
    TerminalSearch { query: String, matches: Vec<(usize, String)>, selected: usize },
    Branches { branches: Vec<GitBranch>, selected: usize },
    Stashes { stashes: Vec<GitStash>, selected: usize },
    Worktrees { worktrees: Vec<GitWorktree>, selected: usize },
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
    pub theme_name: ThemeName,
    selecting: bool,
    clipboard: Clipboard,
    config: Config,
    state: WorkspaceState,
}

pub enum GitReply {
    Status(Option<GitStatus>, Option<String>),
    Diff(String, String),
    Branches(Vec<GitBranch>),
    Stashes(Vec<GitStash>),
    Worktrees(Vec<GitWorktree>),
}

enum GitJob {
    Refresh,
    Diff(String, bool),
    Stage(String, bool),
    Commit(String),
    Sync(&'static str),
    Branches,
    SwitchBranch(String, bool),
    Stashes,
    StashPush(Option<String>),
    StashPop(Option<String>),
    Worktrees,
}

impl App {
    pub fn new(folder: PathBuf, config: Config) -> Result<Self, String> {
        let profiles = terminal::profiles(&config.agents);
        if let Some(id) = &config.default_shell {
            if !profiles.iter().any(|profile| profile.kind == Kind::Shell && profile.id == *id) {
                return Err(format!("unknown default shell {id:?}; see tala --list-profiles"));
            }
        }
        let mut state = WorkspaceState::load();
        state.record_folder(&folder);
        state.save();
        let theme_name = config.theme;
        let mut app = Self {
            browsing: folder.clone(),
            folder,
            entries: Vec::new(),
            files_error: None,
            profiles,
            git: None,
            git_known: false,
            git_error: None,
            tabs: Vec::new(),
            active: 0,
            panel: Some(Panel::Files),
            panel_focused: false,
            file_index: 0,
            profile_index: 0,
            git_index: 0,
            overlay: None,
            notice: None,
            hits: Vec::new(),
            terminal_area: Rect::ZERO,
            list_area: Rect::ZERO,
            dirty: true,
            quit: false,
            git_work: None,
            theme_name,
            selecting: false,
            clipboard: Clipboard::default(),
            config,
            state,
        };
        app.read_folder();
        app.git_job(GitJob::Refresh);
        Ok(app)
    }

    pub fn theme(&self) -> Theme {
        self.theme_name.palette()
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        self.tabs.get(self.active).and_then(Tab::active)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.tabs.get_mut(self.active).and_then(Tab::active_mut)
    }

    fn read_folder(&mut self) {
        match workspace::list_directory(&self.browsing) {
            Ok(entries) => {
                self.entries = entries;
                self.file_index = self.file_index.min(self.entries.len().saturating_sub(1));
                self.files_error = None;
            }
            Err(error) => {
                self.entries.clear();
                self.file_index = 0;
                self.files_error = Some(error);
            }
        }
    }

    fn set_workspace(&mut self, next: PathBuf) {
        if !next.is_dir() {
            self.notice = Some(format!("'{}' is not a directory.", next.display()));
            return;
        }
        let changed = self.folder != next;
        self.folder = next.clone();
        self.browsing = next;
        self.state.record_folder(&self.folder);
        self.state.save();
        self.file_index = 0;
        self.read_folder();
        if changed {
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

    fn choose_folder(&mut self) {
        let target = self.browsing.clone();
        self.set_workspace(target);
    }

    fn open_file_path(&mut self, path: &Path) {
        let title = path
            .strip_prefix(&self.folder)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        match workspace::read_text(path) {
            Ok(body) => {
                self.overlay = Some(Overlay::Editor(EditorState::for_file(path.to_path_buf(), title, &body)));
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn select_entry(&mut self, index: usize) {
        if let Some(entry) = self.entries.get(index).cloned() {
            self.file_index = index;
            if entry.directory {
                self.browsing = entry.path;
                self.file_index = 0;
                self.read_folder();
            } else {
                self.open_file_path(&entry.path);
            }
        }
    }

    fn preferred_shell(&self) -> Option<Profile> {
        let preferred = self.config.default_shell.as_deref();
        let login = std::env::var_os("SHELL").map(PathBuf::from);
        let login = login.as_deref().and_then(|path| path.file_stem()).and_then(|stem| stem.to_str());
        self.profiles
            .iter()
            .find(|item| item.kind == Kind::Shell && Some(item.id.as_str()) == preferred)
            .or_else(|| self.profiles.iter().find(|item| item.available && item.kind == Kind::Shell && Some(item.id.as_str()) == login))
            .or_else(|| self.profiles.iter().find(|item| item.available && item.kind == Kind::Shell))
            .cloned()
    }

    fn new_shell(&mut self) {
        if let Some(profile) = self.preferred_shell() {
            self.launch(profile);
        } else {
            self.notice = Some("No supported shell found. See the launcher (F3).".to_owned());
        }
    }

    fn spawn_pane(&self, profile: Profile, area: Rect) -> Result<Pane, String> {
        if !profile.available {
            return Err(format!(
                "{} was not found. Install `{}` on PATH, then refresh the launcher (r).",
                profile.label, profile.command
            ));
        }
        let (cols, rows) = if area.is_empty() { (80, 24) } else { (area.width.max(2), area.height.max(2)) };
        let session = Session::start(&profile, &self.folder, cols, rows)?;
        Ok(Pane {
            profile,
            cwd: self.folder.clone(),
            parser: vt100::Parser::new_with_callbacks(rows, cols, self.config.scrollback, Replies::default()),
            session: Some(session),
            exit: None,
            selection: None,
            area,
        })
    }

    fn launch(&mut self, profile: Profile) {
        match self.spawn_pane(profile, self.terminal_area) {
            Ok(pane) => {
                self.active = self.tabs.len();
                self.tabs.push(Tab {
                    panes: vec![pane],
                    active_pane: 0,
                    split: SplitDirection::Vertical,
                });
                self.focus_terminal();
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn split_terminal(&mut self, direction: SplitDirection) {
        let Some(profile) = self.preferred_shell() else {
            self.notice = Some("No supported shell found. See the launcher (F3).".to_owned());
            return;
        };
        if self.tabs.is_empty() {
            self.launch(profile);
            return;
        }
        let pane_count = self.tabs.get(self.active).map_or(0, |tab| tab.panes.len());
        if pane_count >= MAX_PANES_PER_TAB {
            self.notice = Some(format!("A tab supports up to {MAX_PANES_PER_TAB} split panes."));
            return;
        }
        match self.spawn_pane(profile, self.terminal_area) {
            Ok(pane) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.split = direction;
                    tab.panes.push(pane);
                    tab.active_pane = tab.panes.len() - 1;
                }
                self.focus_terminal();
            }
            Err(error) => self.notice = Some(error),
        }
    }

    fn focus_terminal(&mut self) {
        self.panel_focused = false;
    }

    pub fn fit_terminal(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else { return false; };
        let mut resized = false;
        let mut resize_error = None;
        for pane in &mut tab.panes {
            let area = pane.area;
            if area.is_empty() {
                continue;
            }
            let screen = pane.parser.screen_mut();
            if screen.size() == (area.height, area.width) {
                continue;
            }
            screen.set_size(area.height, area.width);
            pane.selection = None;
            if let Some(session) = &mut pane.session {
                if let Err(error) = session.resize(area.width, area.height) {
                    resize_error = Some(error);
                }
            }
            resized = true;
        }
        if let Some(error) = resize_error {
            self.notice = Some(error);
        }
        resized
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for (tab_index, tab) in self.tabs.iter_mut().enumerate() {
            for pane in &mut tab.panes {
                let Some(session) = &mut pane.session else { continue; };
                for _ in 0..32 {
                    match session.events().try_recv() {
                        Ok(PtyEvent::Data(data)) => {
                            pane.parser.process(&data);
                            pane.parser.callbacks_mut().flush(|reply| { let _ = session.write(reply); });
                            if tab_index == self.active { changed = true; }
                        }
                        Ok(PtyEvent::InputError(error)) if tab_index == self.active => {
                            self.notice = Some(error);
                            changed = true;
                        }
                        Ok(PtyEvent::InputError(_)) => {}
                        Ok(PtyEvent::Exit(code)) => {
                            pane.exit = code;
                            pane.session = None;
                            changed = true;
                            break;
                        }
                        Err(TryRecvError::Disconnected) => {
                            pane.session = None;
                            changed = true;
                            break;
                        }
                        Err(TryRecvError::Empty) => break,
                    }
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
                    Ok(GitReply::Diff(title, body)) => {
                        self.overlay = Some(Overlay::Editor(EditorState::for_diff(title, &body)));
                    }
                    Ok(GitReply::Branches(branches)) => {
                        let selected = branches.iter().position(|b| b.current).unwrap_or(0);
                        self.overlay = Some(Overlay::Branches { branches, selected });
                    }
                    Ok(GitReply::Stashes(stashes)) => {
                        self.overlay = Some(Overlay::Stashes { stashes, selected: 0 });
                    }
                    Ok(GitReply::Worktrees(worktrees)) => {
                        self.overlay = Some(Overlay::Worktrees { worktrees, selected: 0 });
                    }
                    Err(error) => {
                        self.git_error = Some(error.clone());
                        self.notice = Some(error);
                    }
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
        if let Some(pane) = self.active_pane_mut() {
            if let Some(session) = pane.session.as_mut() {
                pane.parser.screen_mut().set_scrollback(0);
                pane.selection = None;
                if let Err(error) = session.write(bytes) {
                    self.notice = Some(error);
                }
            }
        }
    }

    fn paste(&mut self, text: &str) {
        if text.len() > 1024 * 1024 {
            self.notice = Some("Paste is limited to 1 MiB.".to_owned());
            return;
        }
        match &mut self.overlay {
            Some(Overlay::Input { value, .. }) => {
                value.extend(text.chars().filter(|ch| !ch.is_control()));
                return;
            }
            Some(Overlay::Palette { query, selected })
            | Some(Overlay::QuickOpen { query, selected, .. }) => {
                query.extend(text.chars().filter(|ch| !ch.is_control()));
                *selected = 0;
                return;
            }
            Some(Overlay::Workspaces { input, selected, .. }) => {
                input.extend(text.chars().filter(|ch| !ch.is_control()));
                *selected = 0;
                return;
            }
            Some(Overlay::Editor(editor)) if editor.editing => {
                editor.insert_text(text);
                return;
            }
            Some(_) => return,
            None => {}
        }
        if let Some(pane) = self.active_pane() {
            if pane.parser.screen().bracketed_paste() {
                let mut data = Vec::with_capacity(text.len() + 12);
                data.extend_from_slice(b"\x1b[200~");
                data.extend(text.bytes().filter(|&byte| byte != 0x1b));
                data.extend_from_slice(b"\x1b[201~");
                self.send(&data);
            } else if text.contains('\x1b') {
                let sanitized = text.replace('\x1b', "");
                self.send(sanitized.as_bytes());
            } else {
                self.send(text.as_bytes());
            }
        }
    }

    fn copy(&mut self) {
        let text = match &self.overlay {
            Some(Overlay::Editor(editor)) => editor.text(),
            Some(Overlay::TerminalSearch { matches, selected, .. }) => {
                let Some((_, line)) = matches.get(*selected) else { return; };
                line.clone()
            }
            _ => {
                let Some(pane) = self.active_pane() else { return; };
                let Some((a, b)) = pane.selection else {
                    self.notice = Some("Drag over terminal text to select it first.".to_owned());
                    return;
                };
                let ((r1, c1), (r2, c2)) = (a.min(b), a.max(b));
                pane.parser.screen().contents_between(r1, c1, r2, c2.saturating_add(1))
            }
        };
        self.notice = Some(self.clipboard.copy(text).err().unwrap_or_else(|| "Copied to clipboard.".to_owned()));
    }

    fn scroll_terminal(&mut self, delta: isize) {
        if let Some(pane) = self.active_pane_mut() {
            let screen = pane.parser.screen_mut();
            screen.set_scrollback(screen.scrollback().saturating_add_signed(delta));
            pane.selection = None;
        }
    }

    fn open_terminal_search(&mut self) {
        let Some(pane) = self.active_pane_mut() else {
            self.notice = Some("Open a terminal first to search output.".to_owned());
            return;
        };
        let matches = search_pane_lines(pane, "");
        self.overlay = Some(Overlay::TerminalSearch {
            query: String::new(),
            matches,
            selected: 0,
        });
    }

    fn refresh_terminal_search(&mut self) {
        let query = match &self.overlay {
            Some(Overlay::TerminalSearch { query, .. }) => query.clone(),
            _ => return,
        };
        let matches = self
            .active_pane_mut()
            .map(|pane| search_pane_lines(pane, &query))
            .unwrap_or_default();
        if let Some(Overlay::TerminalSearch { matches: current, selected, .. }) = &mut self.overlay {
            *current = matches;
            *selected = 0;
        }
    }

    pub fn filtered_palette_indices(query: &str) -> Vec<usize> {
        let needle = query.trim().to_ascii_lowercase();
        PALETTE_ITEMS
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                needle.is_empty()
                    || item.label.to_ascii_lowercase().contains(&needle)
                    || item.shortcut.to_ascii_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn handle(&mut self, event: Event) {
        self.dirty = true;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => self.key(key),
            Event::Mouse(mouse) => self.mouse(mouse),
            Event::Paste(text) => {
                let accepts_paste = !self.panel_focused && self.overlay.is_none()
                    || matches!(
                        self.overlay,
                        Some(Overlay::Input { .. } | Overlay::Palette { .. } | Overlay::QuickOpen { .. } | Overlay::Workspaces { .. })
                    )
                    || matches!(&self.overlay, Some(Overlay::Editor(ed)) if ed.editing);
                if accepts_paste {
                    self.paste(&text);
                }
            }
            _ => {}
        }
    }

    fn key(&mut self, key: KeyEvent) {
        let modifiers = key.modifiers;
        if modifiers.contains(Mods::CONTROL | Mods::SHIFT) {
            match key.code {
                Key::Char('C' | 'c') => return self.act(Action::Copy),
                Key::Char('V' | 'v') => return self.act(Action::Paste),
                Key::Char('P' | 'p') => return self.act(Action::Palette),
                Key::Char('T' | 't') if self.overlay.is_none() => return self.act(Action::New),
                Key::Char('D' | 'd') if self.overlay.is_none() => return self.act(Action::Split(SplitDirection::Vertical)),
                Key::Char('E' | 'e') if self.overlay.is_none() => return self.act(Action::Split(SplitDirection::Horizontal)),
                Key::Char('F' | 'f') if self.overlay.is_none() => return self.act(Action::SearchTerminal),
                Key::Char('O' | 'o') if self.overlay.is_none() => return self.act(Action::Workspaces),
                Key::Char('G' | 'g') if self.overlay.is_none() => return self.act(Action::Panel(Panel::Git)),
                Key::Char('A' | 'a') if self.overlay.is_none() => return self.act(Action::Panel(Panel::Launcher)),
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
        if self.overlay.is_some() {
            self.overlay_key(key);
            return;
        }
        if modifiers == Mods::CONTROL {
            match key.code {
                Key::Char('p' | 'P') => return self.act(Action::QuickOpen),
                Key::Char('b' | 'B') => return self.act(Action::ToggleSidebar),
                _ => {}
            }
        }
        if modifiers == Mods::ALT && matches!(key.code, Key::Left | Key::Right) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if tab.panes.len() > 1 {
                    let len = tab.panes.len();
                    tab.active_pane = if key.code == Key::Right {
                        (tab.active_pane + 1) % len
                    } else {
                        (tab.active_pane + len - 1) % len
                    };
                    self.focus_terminal();
                    return;
                }
            }
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
        } else if let Some(pane) = self.active_pane() {
            if let Some(bytes) = terminal_view::key_bytes(key, pane.parser.screen().application_cursor()) {
                self.send(&bytes);
            }
        }
    }

    fn overlay_key(&mut self, key: KeyEvent) {
        let modifiers = key.modifiers;
        match self.overlay.as_mut() {
            Some(Overlay::Help) => {
                if key.code == Key::Esc {
                    self.overlay = None;
                }
            }
            Some(Overlay::Editor(editor)) => {
                if modifiers == Mods::CONTROL && matches!(key.code, Key::Char('s' | 'S')) {
                    if let Some(path) = editor.path.clone() {
                        let text = editor.text();
                        match workspace::write_text(&path, &text) {
                            Ok(()) => {
                                editor.dirty = false;
                                self.notice = Some(format!("Saved {}", editor.title));
                                self.read_folder();
                                self.git_job(GitJob::Refresh);
                            }
                            Err(error) => self.notice = Some(error),
                        }
                    }
                    return;
                }
                match key.code {
                    Key::Esc => {
                        if editor.dirty {
                            let title = editor.title.clone();
                            self.overlay = Some(Overlay::Confirm {
                                title: "Unsaved changes".to_owned(),
                                message: format!("Discard unsaved changes to '{title}'? (Enter/y to discard, Esc to cancel)"),
                                kind: ConfirmKind::DiscardEditor,
                            });
                        } else {
                            self.overlay = None;
                        }
                    }
                    Key::Char('e') if !editor.editing && !modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                        if editor.toggle_edit() {
                            self.notice = Some("Edit mode active. Press Ctrl+S to save.".to_owned());
                        }
                    }
                    Key::Up => editor.move_up(1),
                    Key::Down => editor.move_down(1),
                    Key::PageUp => editor.move_up(12),
                    Key::PageDown => editor.move_down(12),
                    Key::Left => editor.move_left(),
                    Key::Right => editor.move_right(),
                    Key::Home => editor.move_home(),
                    Key::End => editor.move_end(),
                    Key::Enter if editor.editing => editor.insert_newline(),
                    Key::Backspace if editor.editing => editor.backspace(),
                    Key::Delete if editor.editing => editor.delete(),
                    Key::Tab if editor.editing => editor.insert_text("    "),
                    Key::Char(ch) if editor.editing && !modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                        editor.insert_char(ch);
                    }
                    _ => {}
                }
            }
            Some(Overlay::Input { value, kind, .. }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Backspace => { value.pop(); }
                Key::Char(ch) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => value.push(ch),
                Key::Enter => {
                    let submitted = std::mem::take(value);
                    let kind = kind.clone();
                    self.overlay = None;
                    self.submit_input(kind, submitted);
                }
                _ => {}
            },
            Some(Overlay::Confirm { kind, .. }) => match key.code {
                Key::Esc | Key::Char('n' | 'N') => self.overlay = None,
                Key::Enter | Key::Char('y' | 'Y') => {
                    let kind = kind.clone();
                    self.overlay = None;
                    self.execute_confirm(kind);
                }
                _ => {}
            },
            Some(Overlay::Palette { query, selected }) => {
                let filtered = Self::filtered_palette_indices(query);
                match key.code {
                    Key::Esc => self.overlay = None,
                    Key::Up => *selected = selected.saturating_sub(1),
                    Key::Down => *selected = (*selected + 1).min(filtered.len().saturating_sub(1)),
                    Key::Backspace => {
                        query.pop();
                        *selected = 0;
                    }
                    Key::Char(ch) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                        query.push(ch);
                        *selected = 0;
                    }
                    Key::Enter => {
                        if let Some(&item_idx) = filtered.get(*selected) {
                            let action = PALETTE_ITEMS[item_idx].action;
                            self.overlay = None;
                            self.act(action);
                        }
                    }
                    _ => {}
                }
            }
            Some(Overlay::QuickOpen { query, files, selected }) => {
                let needle = query.trim().to_ascii_lowercase();
                let folder = &self.folder;
                let filtered: Vec<PathBuf> = files
                    .iter()
                    .filter(|path| {
                        needle.is_empty()
                            || path
                                .strip_prefix(folder)
                                .unwrap_or(path)
                                .to_string_lossy()
                                .to_ascii_lowercase()
                                .contains(&needle)
                    })
                    .cloned()
                    .collect();
                match key.code {
                    Key::Esc => self.overlay = None,
                    Key::Up => *selected = selected.saturating_sub(1),
                    Key::Down => *selected = (*selected + 1).min(filtered.len().saturating_sub(1)),
                    Key::Backspace => {
                        query.pop();
                        *selected = 0;
                    }
                    Key::Char(ch) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                        query.push(ch);
                        *selected = 0;
                    }
                    Key::Enter => {
                        if let Some(target) = filtered.get(*selected).cloned() {
                            self.overlay = None;
                            self.open_file_path(&target);
                        }
                    }
                    _ => {}
                }
            }
            Some(Overlay::Workspaces { input, folders, selected }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Up => *selected = selected.saturating_sub(1),
                Key::Down => *selected = (*selected + 1).min(folders.len().saturating_sub(1)),
                Key::Backspace => { input.pop(); }
                Key::Char(ch) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => input.push(ch),
                Key::Enter => {
                    let typed = input.trim().to_owned();
                    let chosen = if !typed.is_empty() {
                        std::path::absolute(Path::new(&typed)).ok()
                    } else {
                        folders.get(*selected).cloned()
                    };
                    self.overlay = None;
                    if let Some(path) = chosen {
                        self.set_workspace(path);
                    }
                }
                _ => {}
            },
            Some(Overlay::TerminalSearch { query, matches, selected }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Up => *selected = selected.saturating_sub(1),
                Key::Down => *selected = (*selected + 1).min(matches.len().saturating_sub(1)),
                Key::Backspace => {
                    query.pop();
                    self.refresh_terminal_search();
                }
                Key::Char(ch) if !modifiers.intersects(Mods::CONTROL | Mods::ALT) => {
                    query.push(ch);
                    self.refresh_terminal_search();
                }
                Key::Enter => {
                    let target = matches.get(*selected).map(|(offset, _)| *offset);
                    self.overlay = None;
                    if let (Some(offset), Some(pane)) = (target, self.active_pane_mut()) {
                        pane.parser.screen_mut().set_scrollback(offset);
                        self.notice = Some(format!("Jumped to scrollback offset {offset}."));
                    }
                }
                _ => {}
            },
            Some(Overlay::Branches { branches, selected }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Up => *selected = selected.saturating_sub(1),
                Key::Down => *selected = (*selected + 1).min(branches.len().saturating_sub(1)),
                Key::Char('n') => {
                    self.overlay = Some(Overlay::Input {
                        title: "Create Git branch".to_owned(),
                        hint: "Enter creates and switches to the new branch".to_owned(),
                        value: String::new(),
                        kind: InputKind::NewBranch,
                    });
                }
                Key::Enter => {
                    if let Some(branch) = branches.get(*selected).cloned() {
                        self.overlay = None;
                        if !branch.current {
                            self.git_job(GitJob::SwitchBranch(branch.name, false));
                        }
                    }
                }
                _ => {}
            },
            Some(Overlay::Stashes { stashes, selected }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Up => *selected = selected.saturating_sub(1),
                Key::Down => *selected = (*selected + 1).min(stashes.len().saturating_sub(1)),
                Key::Enter => {
                    if let Some(stash) = stashes.get(*selected).cloned() {
                        self.overlay = None;
                        self.git_job(GitJob::StashPop(Some(stash.reference)));
                    }
                }
                _ => {}
            },
            Some(Overlay::Worktrees { worktrees, selected }) => match key.code {
                Key::Esc => self.overlay = None,
                Key::Up => *selected = selected.saturating_sub(1),
                Key::Down => *selected = (*selected + 1).min(worktrees.len().saturating_sub(1)),
                Key::Enter => {
                    if let Some(worktree) = worktrees.get(*selected).cloned() {
                        self.overlay = None;
                        self.set_workspace(worktree.path);
                    }
                }
                _ => {}
            },
            None => {}
        }
    }

    fn submit_input(&mut self, kind: InputKind, value: String) {
        let trimmed = value.trim();
        match kind {
            InputKind::Commit => {
                if !trimmed.is_empty() {
                    self.git_job(GitJob::Commit(trimmed.to_owned()));
                }
            }
            InputKind::NewFile => match workspace::create_file(&self.browsing, trimmed) {
                Ok(path) => {
                    self.read_folder();
                    self.open_file_path(&path);
                    if let Some(Overlay::Editor(editor)) = &mut self.overlay {
                        editor.toggle_edit();
                    }
                    self.notice = Some(format!("Created file '{trimmed}'."));
                }
                Err(error) => self.notice = Some(error),
            },
            InputKind::NewFolder => match workspace::create_dir(&self.browsing, trimmed) {
                Ok(_) => {
                    self.read_folder();
                    self.notice = Some(format!("Created folder '{trimmed}'."));
                }
                Err(error) => self.notice = Some(error),
            },
            InputKind::Rename(path) => match workspace::rename_entry(&path, trimmed) {
                Ok(_) => {
                    self.read_folder();
                    self.notice = Some(format!("Renamed to '{trimmed}'."));
                }
                Err(error) => self.notice = Some(error),
            },
            InputKind::NewBranch => {
                if !trimmed.is_empty() {
                    self.git_job(GitJob::SwitchBranch(trimmed.to_owned(), true));
                }
            }
            InputKind::StashMessage => {
                let msg = if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) };
                self.git_job(GitJob::StashPush(msg));
            }
        }
    }

    fn execute_confirm(&mut self, kind: ConfirmKind) {
        match kind {
            ConfirmKind::Delete(path) => match workspace::delete_entry(&path) {
                Ok(()) => {
                    self.read_folder();
                    self.git_job(GitJob::Refresh);
                    self.notice = Some(format!("Deleted '{}'.", path.file_name().unwrap_or_default().to_string_lossy()));
                }
                Err(error) => self.notice = Some(error),
            },
            ConfirmKind::DiscardEditor => {
                self.notice = Some("Closed without saving.".to_owned());
            }
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
                (Some(Panel::Files), Key::Char('n')) => self.act(Action::NewFile),
                (Some(Panel::Files), Key::Char('N')) => self.act(Action::NewFolder),
                (Some(Panel::Files), Key::Char('R')) => self.act(Action::RenameEntry),
                (Some(Panel::Files), Key::Char('d') | Key::Delete) => self.act(Action::DeleteEntry),
                (Some(Panel::Files), Key::Char('r')) => self.read_folder(),
                (Some(Panel::Launcher), Key::Enter) => self.act(Action::Profile(self.profile_index)),
                (Some(Panel::Launcher), Key::Char('r')) => self.profiles = terminal::profiles(&self.config.agents),
                (Some(Panel::Git), Key::Enter) => self.git_diff(None),
                (Some(Panel::Git), Key::Char('d')) => self.git_diff(Some(true)),
                (Some(Panel::Git), Key::Char('s')) => self.act(Action::Stage(true)),
                (Some(Panel::Git), Key::Char('u')) => self.act(Action::Stage(false)),
                (Some(Panel::Git), Key::Char('c')) => self.act(Action::Commit),
                (Some(Panel::Git), Key::Char('r')) => self.act(Action::Refresh),
                (Some(Panel::Git), Key::Char('f')) => self.act(Action::Sync("fetch")),
                (Some(Panel::Git), Key::Char('p')) => self.act(Action::Sync("pull")),
                (Some(Panel::Git), Key::Char('P')) => self.act(Action::Sync("push")),
                (Some(Panel::Git), Key::Char('b')) => self.act(Action::GitBranches),
                (Some(Panel::Git), Key::Char('S')) => self.act(Action::GitStashPush),
                (Some(Panel::Git), Key::Char('t')) => self.act(Action::GitStashes),
                (Some(Panel::Git), Key::Char('w')) => self.act(Action::GitWorktrees),
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
                    if let Overlay::Editor(editor) = overlay {
                        if event.kind == MouseEventKind::ScrollUp {
                            editor.move_up(3);
                        } else {
                            editor.move_down(3);
                        }
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if self.hits.iter().any(|&(r, action)| matches!(action, Action::Dismiss) && inside(r)) {
                        self.act(Action::Dismiss);
                    }
                }
                _ => {}
            }
            return;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, action)) = self.hits.iter().rev().find(|(r, _)| inside(*r)).copied() {
                    self.act(action);
                }
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if let Some((pane_idx, pane)) = tab.panes.iter_mut().enumerate().find(|(_, p)| inside(p.area)) {
                        tab.active_pane = pane_idx;
                        let point = (y - pane.area.y, x - pane.area.x);
                        pane.selection = Some((point, point));
                        self.selecting = true;
                        self.focus_terminal();
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) if self.selecting => {
                if let Some(pane) = self.active_pane_mut() {
                    let area = pane.area;
                    if !area.is_empty() {
                        if let Some((_, end)) = pane.selection.as_mut() {
                            *end = (
                                y.saturating_sub(area.y).min(area.height - 1),
                                x.saturating_sub(area.x).min(area.width - 1),
                            );
                        }
                    }
                }
                self.selecting = event.kind != MouseEventKind::Up(MouseButton::Left);
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = event.kind == MouseEventKind::ScrollUp;
                if inside(self.terminal_area) {
                    self.scroll_terminal(if up { 3 } else { -3 });
                } else if inside(self.list_area) {
                    self.panel_focused = true;
                    self.move_index(if up { -3 } else { 3 });
                }
            }
            _ => {}
        }
    }

    fn act(&mut self, action: Action) {
        self.notice = None;
        match action {
            Action::Help => self.overlay = Some(Overlay::Help),
            Action::Palette => {
                self.overlay = Some(Overlay::Palette { query: String::new(), selected: 0 });
            }
            Action::QuickOpen => match workspace::collect_workspace_files(&self.folder, 2_000) {
                Ok(files) => {
                    self.overlay = Some(Overlay::QuickOpen { query: String::new(), files, selected: 0 });
                }
                Err(error) => self.notice = Some(error),
            },
            Action::Workspaces => {
                let folders: Vec<PathBuf> = self
                    .state
                    .recent_folders
                    .iter()
                    .filter(|path| path.is_dir())
                    .cloned()
                    .collect();
                self.overlay = Some(Overlay::Workspaces { input: String::new(), folders, selected: 0 });
            }
            Action::ToggleSidebar => {
                if self.panel.is_some() {
                    self.panel = None;
                    self.focus_terminal();
                } else {
                    self.panel = Some(Panel::Files);
                    self.panel_focused = true;
                }
            }
            Action::Panel(panel) => {
                if self.panel == Some(panel) && self.panel_focused {
                    self.panel = None;
                    self.focus_terminal();
                } else {
                    self.panel = Some(panel);
                    self.panel_focused = true;
                    if panel == Panel::Git { self.git_job(GitJob::Refresh); }
                }
            }
            Action::FocusTerminal => self.focus_terminal(),
            Action::New => self.new_shell(),
            Action::Split(direction) => self.split_terminal(direction),
            Action::Tab(index) => {
                self.selecting = false;
                self.active = index;
                self.focus_terminal();
            }
            Action::Pane(index) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if index < tab.panes.len() {
                        self.selecting = false;
                        tab.active_pane = index;
                        self.focus_terminal();
                    }
                }
            }
            Action::NextPane => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    if tab.panes.len() > 1 {
                        tab.active_pane = (tab.active_pane + 1) % tab.panes.len();
                        self.focus_terminal();
                    }
                }
            }
            Action::Close => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    self.selecting = false;
                    if tab.panes.len() > 1 {
                        tab.panes.remove(tab.active_pane);
                        tab.active_pane = tab.active_pane.min(tab.panes.len().saturating_sub(1));
                    } else {
                        self.tabs.remove(self.active);
                        self.active = self.active.min(self.tabs.len().saturating_sub(1));
                    }
                    self.focus_terminal();
                }
            }
            Action::SearchTerminal => self.open_terminal_search(),
            Action::Copy => self.copy(),
            Action::Paste => {
                let accepts = self.overlay.is_none() && !self.panel_focused
                    || matches!(
                        self.overlay,
                        Some(Overlay::Input { .. } | Overlay::Palette { .. } | Overlay::QuickOpen { .. } | Overlay::Workspaces { .. })
                    )
                    || matches!(&self.overlay, Some(Overlay::Editor(ed)) if ed.editing);
                if accepts {
                    match self.clipboard.paste() {
                        Ok(text) => self.paste(&text),
                        Err(error) => self.notice = Some(error),
                    }
                }
            }
            Action::CycleTheme => {
                self.theme_name = self.theme_name.next();
                self.notice = Some(format!("Theme: {}", self.theme_name.label()));
            }
            Action::Quit => self.quit = true,
            Action::Up => {
                self.panel_focused = true;
                if self.browsing.pop() {
                    self.file_index = 0;
                    self.read_folder();
                }
            }
            Action::UseFolder => self.choose_folder(),
            Action::NewFile => {
                self.overlay = Some(Overlay::Input {
                    title: "New file".to_owned(),
                    hint: "Enter file name in the current folder".to_owned(),
                    value: String::new(),
                    kind: InputKind::NewFile,
                });
            }
            Action::NewFolder => {
                self.overlay = Some(Overlay::Input {
                    title: "New folder".to_owned(),
                    hint: "Enter folder name in the current folder".to_owned(),
                    value: String::new(),
                    kind: InputKind::NewFolder,
                });
            }
            Action::RenameEntry => {
                if let Some(entry) = self.entries.get(self.file_index).cloned() {
                    self.overlay = Some(Overlay::Input {
                        title: format!("Rename '{}'", entry.name),
                        hint: "Enter the new name".to_owned(),
                        value: entry.name,
                        kind: InputKind::Rename(entry.path),
                    });
                }
            }
            Action::DeleteEntry => {
                if let Some(entry) = self.entries.get(self.file_index).cloned() {
                    let kind_label = if entry.directory { "folder" } else { "file" };
                    self.overlay = Some(Overlay::Confirm {
                        title: format!("Delete {kind_label}"),
                        message: format!("Permanently delete '{}' ? (Enter/y to confirm, Esc to cancel)", entry.name),
                        kind: ConfirmKind::Delete(entry.path),
                    });
                }
            }
            Action::Entry(index) => {
                self.panel_focused = true;
                self.select_entry(index);
            }
            Action::Profile(index) => {
                self.profile_index = index;
                if let Some(profile) = self.profiles.get(index).cloned() {
                    self.launch(profile);
                }
            }
            Action::GitRow(index) => {
                self.panel_focused = true;
                self.git_index = index;
                self.git_diff(None);
            }
            Action::Refresh => self.git_job(GitJob::Refresh),
            Action::Stage(stage) => {
                if let Some(change) = self.git.as_ref().and_then(|git| git.changes.get(self.git_index)) {
                    if if stage { change.unstaged || change.untracked } else { change.staged } {
                        self.git_job(GitJob::Stage(change.path.clone(), stage));
                    }
                }
            }
            Action::Commit if self.git_work.is_none() && self.git.as_ref().is_some_and(|git| git.changes.iter().any(|change| change.staged)) => {
                self.overlay = Some(Overlay::Input {
                    title: "Commit staged changes".to_owned(),
                    hint: "Enter commits · F9 pastes".to_owned(),
                    value: String::new(),
                    kind: InputKind::Commit,
                });
            }
            Action::Sync(action) if self.git.is_some() => self.git_job(GitJob::Sync(action)),
            Action::GitBranches if self.git.is_some() => self.git_job(GitJob::Branches),
            Action::GitStashes if self.git.is_some() => self.git_job(GitJob::Stashes),
            Action::GitStashPush if self.git.is_some() => {
                self.overlay = Some(Overlay::Input {
                    title: "Stash changes".to_owned(),
                    hint: "Optional stash message · Enter to stash".to_owned(),
                    value: String::new(),
                    kind: InputKind::StashMessage,
                });
            }
            Action::GitWorktrees if self.git.is_some() => self.git_job(GitJob::Worktrees),
            Action::Commit | Action::Sync(_) | Action::GitBranches | Action::GitStashes | Action::GitStashPush | Action::GitWorktrees => {}
            Action::Dismiss => self.overlay = None,
        }
    }
}

fn search_pane_lines(pane: &mut Pane, query: &str) -> Vec<(usize, String)> {
    let query_lower = query.trim().to_ascii_lowercase();
    let screen = pane.parser.screen_mut();
    let original = screen.scrollback();
    let rows = usize::from(screen.size().0).max(1);
    let mut matches: Vec<(usize, String)> = Vec::new();
    let mut offset = 0usize;
    loop {
        screen.set_scrollback(offset);
        let actual = screen.scrollback();
        let text = screen.contents();
        for line in text.lines() {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            if query_lower.is_empty() || trimmed.to_ascii_lowercase().contains(&query_lower) {
                if matches.last().is_none_or(|(_, prev)| prev != trimmed) {
                    matches.push((actual, trimmed.to_owned()));
                }
            }
            if matches.len() >= 200 {
                break;
            }
        }
        if matches.len() >= 200 {
            break;
        }
        let next = actual.saturating_add(rows);
        screen.set_scrollback(next);
        if screen.scrollback() <= actual {
            break;
        }
        offset = next;
    }
    screen.set_scrollback(original);
    matches
}

fn run_git(folder: &Path, job: GitJob) -> Result<GitReply, String> {
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
        GitJob::Branches => return Ok(GitReply::Branches(workspace::git_branches(folder)?)),
        GitJob::SwitchBranch(name, create) => {
            let message = workspace::git_switch_branch(folder, &name, create)?;
            Some(if message.trim().is_empty() { format!("Switched to branch '{name}'.") } else { message })
        }
        GitJob::Stashes => return Ok(GitReply::Stashes(workspace::git_stashes(folder)?)),
        GitJob::StashPush(message) => {
            let output = workspace::git_stash_push(folder, message.as_deref())?;
            Some(if output.trim().is_empty() { "Saved stash.".to_owned() } else { output })
        }
        GitJob::StashPop(reference) => {
            let output = workspace::git_stash_pop(folder, reference.as_deref())?;
            Some(if output.trim().is_empty() { "Applied and dropped stash.".to_owned() } else { output })
        }
        GitJob::Worktrees => return Ok(GitReply::Worktrees(workspace::git_worktrees(folder)?)),
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
        assert!(matches!(app.overlay, Some(Overlay::Editor(ref ed)) if ed.text() == "notes from the selected folder"));
    }

    #[test]
    fn palette_filters_and_cycles_theme() {
        let root = tempfile::tempdir().unwrap();
        let mut app = App::new(root.path().to_owned(), Config::default()).unwrap();
        assert_eq!(app.theme_name, ThemeName::Dark);
        app.handle(Event::Key(KeyEvent::new(Key::Char('P'), Mods::CONTROL | Mods::SHIFT)));
        assert!(matches!(app.overlay, Some(Overlay::Palette { .. })));
        for ch in "theme".chars() {
            app.handle(Event::Key(KeyEvent::new(Key::Char(ch), Mods::NONE)));
        }
        app.handle(Event::Key(KeyEvent::new(Key::Enter, Mods::NONE)));
        assert!(app.overlay.is_none());
        assert_eq!(app.theme_name, ThemeName::Light);
    }

    #[test]
    fn quick_open_finds_and_opens_file_for_editing_and_saving() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("hello.txt");
        std::fs::write(&file, "v1").unwrap();
        let mut app = App::new(root.path().to_owned(), Config::default()).unwrap();
        app.handle(Event::Key(KeyEvent::new(Key::Char('p'), Mods::CONTROL)));
        assert!(matches!(app.overlay, Some(Overlay::QuickOpen { .. })));
        app.handle(Event::Key(KeyEvent::new(Key::Enter, Mods::NONE)));
        assert!(matches!(app.overlay, Some(Overlay::Editor(_))));
        app.handle(Event::Key(KeyEvent::new(Key::Char('e'), Mods::NONE)));
        app.handle(Event::Key(KeyEvent::new(Key::End, Mods::NONE)));
        app.handle(Event::Key(KeyEvent::new(Key::Char('2'), Mods::NONE)));
        app.handle(Event::Key(KeyEvent::new(Key::Char('s'), Mods::CONTROL)));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v12");
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
