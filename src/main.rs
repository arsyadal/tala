mod app;
mod clipboard;
mod config;
mod terminal;
mod terminal_view;
mod ui;
mod workspace;

use app::App;
use config::Config;
use crossterm::{
    cursor::Show,
    event::{self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{self as tty, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{env, ffi::OsStr, io::{self, IsTerminal}, path::PathBuf, process::ExitCode, time::Duration};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => { eprintln!("tala: {error}"); ExitCode::FAILURE }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let first = args.next();
    match first.as_deref().and_then(OsStr::to_str) {
        Some("--help" | "-h") => {
            println!("Tala {}\nFind your flow.\n\nUsage: tala [FOLDER]\n       tala --list-profiles\n       tala --config-path\n       tala --version\n\nF1: help   F2: files   F3: launcher   F4: Git   F10: quit", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--version" | "-V") => { println!("tala {}", env!("CARGO_PKG_VERSION")); return Ok(()); }
        Some("--config-path") => {
            let path = Config::path().ok_or("no user configuration directory found")?;
            println!("{}", path.display());
            return Ok(());
        }
        Some("--list-profiles") => {
            for profile in terminal::profiles() {
                println!("{}\t{}\t{}", profile.id, profile.command, if profile.available { "available" } else { "not found" });
            }
            return Ok(());
        }
        Some(arg) if arg.starts_with('-') => return Err(format!("unknown option {arg}; see tala --help").into()),
        _ => {}
    }
    if args.next().is_some() {
        return Err("expected one folder; see tala --help".into());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("an interactive terminal is required; see tala --help".into());
    }
    let folder = first.map(PathBuf::from).map_or_else(env::current_dir, std::path::absolute)?;
    if !folder.is_dir() {
        return Err(format!("{} is not a directory", folder.display()).into());
    }
    let config = Config::load()?;
    let session = TtyGuard::enter(config.mouse)?;
    let mut output = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    output.clear()?;
    let mut app = App::new(folder, config)?;
    while !app.quit {
        let changed = app.poll();
        if changed || app.dirty {
            output.draw(|frame| ui::draw(&mut app, frame))?;
            app.dirty = app.fit_terminal();
        }
        if event::poll(Duration::from_millis(if changed { 0 } else { 16 }))? {
            app.handle(event::read()?);
        }
    }
    drop(session);
    Ok(())
}

struct TtyGuard { mouse: bool }

impl TtyGuard {
    fn enter(mouse: bool) -> io::Result<Self> {
        tty::enable_raw_mode()?;
        let guard = Self { mouse };
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        if mouse { execute!(stdout, EnableMouseCapture)?; }
        Ok(guard)
    }
}

impl Drop for TtyGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        if self.mouse { let _ = execute!(stdout, DisableMouseCapture); }
        let _ = execute!(stdout, DisableBracketedPaste, LeaveAlternateScreen, Show);
        let _ = tty::disable_raw_mode();
    }
}
