use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
#[cfg(windows)]
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc::{self, Receiver, Sender, SyncSender, TrySendError}};
use std::thread;
use std::time::Duration;

const OUTPUT_CHUNK_SIZE: usize = 16 * 1024;
const OUTPUT_QUEUE_SIZE: usize = 64;
const INPUT_QUEUE_SIZE: usize = 32;
const SMALL_INPUT: usize = 24;
const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Shell,
    Agent,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub id: &'static str,
    pub label: &'static str,
    pub command: &'static str,
    pub kind: Kind,
    pub available: bool,
}

#[derive(Debug)]
pub enum Event {
    Data(Vec<u8>),
    Exit(Option<u32>),
    InputError(String),
}

type Master = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

enum Input {
    Inline { bytes: [u8; SMALL_INPUT], len: u8 },
    Paste(Box<[u8]>),
}

impl Input {
    fn new(bytes: &[u8]) -> Self {
        if bytes.len() > SMALL_INPUT {
            return Self::Paste(bytes.into());
        }
        let mut data = [0; SMALL_INPUT];
        data[..bytes.len()].copy_from_slice(bytes);
        Self::Inline { bytes: data, len: bytes.len() as u8 }
    }

    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Inline { bytes, len } => &bytes[..usize::from(*len)],
            Self::Paste(bytes) => bytes,
        }
    }
}

pub struct Session {
    master: Master,
    input: SyncSender<Input>,
    events: Receiver<Event>,
    close: Sender<()>,
}

impl Session {
    pub fn start(
        profile: &str,
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<Self, String> {
        if cols == 0 || rows == 0 {
            return Err("terminal dimensions must be greater than zero".to_owned());
        }
        if !cwd.is_dir() {
            return Err(format!(
                "terminal working directory does not exist or is not a directory: {}",
                cwd.display()
            ));
        }

        Self::start_invocation(resolve_profile(profile)?, profile, cwd, cols, rows)
    }

    fn start_invocation(invocation: Invocation, profile: &str, cwd: &Path, cols: u16, rows: u16) -> Result<Self, String> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed to create terminal: {error}"))?;

        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|error| format!("failed to open terminal output: {error}"))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|error| format!("failed to open terminal input: {error}"))?;

        let mut command = invocation.command_builder()?;
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        if profile == "git-bash" {
            command.env("CHERE_INVOKING", "1");
        }
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| format!("failed to start terminal profile '{profile}': {error}"))?;
        drop(pty.slave);

        let (event_tx, events) = mpsc::sync_channel(OUTPUT_QUEUE_SIZE);
        let (input, input_rx) = mpsc::sync_channel(INPUT_QUEUE_SIZE);
        let (reader_done_tx, reader_done_rx) = mpsc::channel();
        let (close_tx, close_rx) = mpsc::channel();

        let master = Arc::new(Mutex::new(Some(pty.master)));
        spawn_output_reader(reader, event_tx.clone(), reader_done_tx);
        spawn_input_writer(writer, input_rx, event_tx.clone());
        spawn_child_reaper(child, close_rx, reader_done_rx, event_tx, Arc::clone(&master));

        Ok(Self { master, input, events, close: close_tx })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.input.try_send(Input::new(bytes)).map_err(|error| match error {
            TrySendError::Full(_) => "terminal is busy reading input".to_owned(),
            TrySendError::Disconnected(_) => "terminal input is closed".to_owned(),
        })
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), String> {
        if cols == 0 || rows == 0 {
            return Err("terminal dimensions must be greater than zero".to_owned());
        }
        let master = self.master.lock().map_err(|_| "terminal resize lock poisoned".to_owned())?;
        let Some(master) = master.as_ref() else { return Ok(()); };
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed to resize terminal: {error}"))
    }

    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.close.send(());
    }
}

pub fn profiles() -> Vec<Profile> {
    let mut profiles = Vec::with_capacity(8);

    #[cfg(windows)]
    {
        let powershell_command = if find_executable("pwsh.exe").is_some() {
            "pwsh"
        } else {
            "powershell"
        };
        profiles.extend([
            Profile {
                id: "powershell",
                label: "PowerShell",
                command: powershell_command,
                kind: Kind::Shell,
                available: resolve_windows_powershell().is_some(),
            },
            Profile {
                id: "cmd",
                label: "Command Prompt",
                command: "cmd",
                kind: Kind::Shell,
                available: resolve_cmd().is_some(),
            },
            Profile {
                id: "git-bash",
                label: "Git Bash",
                command: "bash",
                kind: Kind::Shell,
                available: resolve_git_bash().is_some(),
            },
            Profile {
                id: "wsl",
                label: "WSL",
                command: "wsl",
                kind: Kind::Shell,
                available: find_executable("wsl.exe").is_some(),
            },
        ]);
    }

    #[cfg(not(windows))]
    profiles.extend([
        shell_profile("bash", "Bash"),
        shell_profile("zsh", "Zsh"),
        shell_profile("fish", "Fish"),
    ]);

    profiles.extend([
        agent_profile("codex", "Codex (ChatGPT)", "codex"),
        agent_profile("agy", "Antigravity", "agy"),
        agent_profile("omp", "OMP", "omp"),
        agent_profile("pi", "Pi", "pi"),
    ]);
    profiles
}

#[cfg(not(windows))]
fn shell_profile(id: &'static str, label: &'static str) -> Profile {
    Profile {
        id,
        label,
        command: id,
        kind: Kind::Shell,
        available: find_executable(id).is_some(),
    }
}

fn agent_profile(id: &'static str, label: &'static str, command: &'static str) -> Profile {
    Profile {
        id,
        label,
        command,
        kind: Kind::Agent,
        available: find_executable(command).is_some(),
    }
}

struct Invocation {
    executable: PathBuf,
    arguments: &'static [&'static str],
    #[cfg(windows)]
    cmd_shim: bool,
}

impl Invocation {
    fn direct(executable: PathBuf, arguments: &'static [&'static str]) -> Self {
        Self {
            executable,
            arguments,
            #[cfg(windows)]
            cmd_shim: false,
        }
    }

    fn command_builder(self) -> Result<CommandBuilder, String> {
        #[cfg(windows)]
        if self.cmd_shim {
            let cmd = resolve_cmd().ok_or_else(|| {
                "cannot launch command shim because Command Prompt is unavailable".to_owned()
            })?;
            let mut command = CommandBuilder::new(cmd);
            let mut quoted_path = std::ffi::OsString::from("\"");
            quoted_path.push(self.executable);
            quoted_path.push("\"");
            command.env("TALA_COMMAND_SHIM", quoted_path);
            command.args(["/D", "/V:OFF", "/S", "/C", "%TALA_COMMAND_SHIM%"]);
            return Ok(command);
        }

        let mut command = CommandBuilder::new(self.executable);
        command.args(self.arguments);
        Ok(command)
    }
}

fn resolve_profile(profile: &str) -> Result<Invocation, String> {
    let invocation = match profile {
        #[cfg(windows)]
        "powershell" => resolve_windows_powershell()
            .map(|path| Invocation::direct(path, &["-NoLogo"])),
        #[cfg(windows)]
        "cmd" => resolve_cmd().map(|path| Invocation::direct(path, &[])),
        #[cfg(windows)]
        "git-bash" => resolve_git_bash()
            .map(|path| Invocation::direct(path, &["--login", "-i"])),
        #[cfg(windows)]
        "wsl" => find_executable("wsl.exe").map(|path| Invocation::direct(path, &["--cd", "."])),
        #[cfg(not(windows))]
        "bash" => find_executable("bash").map(|path| Invocation::direct(path, &["-l"])),
        #[cfg(not(windows))]
        "zsh" => find_executable("zsh").map(|path| Invocation::direct(path, &["-l"])),
        #[cfg(not(windows))]
        "fish" => find_executable("fish").map(|path| Invocation::direct(path, &["-l"])),
        "codex" | "agy" | "omp" | "pi" => find_executable(profile).map(agent_invocation),
        _ => return Err(format!("unknown terminal profile '{profile}'")),
    };

    invocation.ok_or_else(|| format!("terminal profile '{profile}' is not available"))
}

fn find_executable(command: &str) -> Option<PathBuf> {
    which::which(command).ok()
}

#[cfg(windows)]
fn resolve_cmd() -> Option<PathBuf> {
    std::env::var_os("ComSpec")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| find_executable("cmd.exe"))
}

#[cfg(windows)]
fn resolve_windows_powershell() -> Option<PathBuf> {
    find_executable("pwsh.exe").or_else(|| find_executable("powershell.exe"))
}

#[cfg(windows)]
fn resolve_git_bash() -> Option<PathBuf> {
    let git = find_executable("git.exe")?;
    for ancestor in git.ancestors().skip(1).take(4) {
        let bin_bash = ancestor.join("bin").join("bash.exe");
        if bin_bash.is_file() {
            return Some(bin_bash);
        }
        let usr_bash = ancestor.join("usr").join("bin").join("bash.exe");
        if usr_bash.is_file() {
            return Some(usr_bash);
        }
    }
    None
}

#[cfg(not(windows))]
fn agent_invocation(path: PathBuf) -> Invocation {
    Invocation::direct(path, &[])
}

#[cfg(windows)]
fn agent_invocation(path: PathBuf) -> Invocation {
    let cmd_shim = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
        });
    Invocation {
        executable: path,
        arguments: &[],
        cmd_shim,
    }
}

fn spawn_input_writer(mut writer: Box<dyn Write + Send>, input: Receiver<Input>, events: SyncSender<Event>) {
    thread::spawn(move || {
        while let Ok(input) = input.recv() {
            if let Err(error) = writer.write_all(input.as_slice()).and_then(|()| writer.flush()) {
                let _ = events.try_send(Event::InputError(format!("failed to write terminal input: {error}")));
                break;
            }
        }
    });
}

fn spawn_output_reader(
    mut reader: Box<dyn Read + Send>,
    events: SyncSender<Event>,
    done: Sender<()>,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; OUTPUT_CHUNK_SIZE];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(length) => {
                    if events.send(Event::Data(buffer[..length].to_vec())).is_err() {
                        break;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = done.send(());
    });
}

fn spawn_child_reaper(
    mut child: Box<dyn Child + Send + Sync>,
    close: Receiver<()>,
    reader_done: Receiver<()>,
    events: SyncSender<Event>,
    master: Master,
) {
    thread::spawn(move || {
        let exit_code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status.exit_code()),
                Ok(None) => {}
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }

            match close.recv_timeout(CHILD_POLL_INTERVAL) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = child.kill();
                    break child.wait().ok().map(|status| status.exit_code());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        };

        // ConPTY can keep its output pipe open until the console itself is closed.
        let closed_master = master.lock().ok().and_then(|mut master| master.take());
        drop(closed_master);
        let _ = reader_done.recv_timeout(Duration::from_secs(1));
        let _ = events.send(Event::Exit(exit_code));
    });
}

#[cfg(test)]
mod tests {
    use super::{profiles, Event, Session};
    use crate::terminal_view::Replies;
    #[cfg(not(windows))]
    use super::Kind;
    use std::time::{Duration, Instant};

    #[test]
    fn interactive_shell_streams_output_and_exit() {
        let profile = test_shell_profile().expect("an interactive shell must be available");
        let cwd = std::env::current_dir().expect("current directory");
        let mut session = Session::start(profile, &cwd, 80, 24).expect("start terminal");
        let mut probe = Probe::default();

        #[cfg(windows)]
        assert!(probe.receive(&mut session).is_none(), "CMD exited before reading input");

        #[cfg(windows)]
        {
            session
                .write(b"set /p TALA_INPUT=\rterminal-input\recho TALA_TRANSCRIPT_%TALA_INPUT%\rexit /b 7\r")
                .expect("write interactive input");
        }
        #[cfg(not(windows))]
        {
            session
                .write(b"read tala_input\n")
                .expect("write read command");
            session
                .write(b"terminal-input\n")
                .expect("write interactive input");
            session
                .write(b"printf 'TALA_TRANSCRIPT_%s\\n' \"$tala_input\"\nexit 7\n")
                .expect("write output and exit commands");
        }

        let exit_code = probe.until_exit(&mut session);
        let transcript = &probe.transcript;

        assert_eq!(exit_code, Some(7));
        assert!(
            transcript
                .windows(b"TALA_TRANSCRIPT_terminal-input".len())
                .any(|window| window == b"TALA_TRANSCRIPT_terminal-input"),
            "terminal transcript did not contain marker: {}",
            String::from_utf8_lossy(&transcript)
        );
    }

    #[cfg(windows)]
    #[test]
    fn launches_windows_agent_shim_from_path_with_spaces_and_metacharacters() {
        let directory = tempfile::Builder::new().prefix("Tala & %USERPROFILE% ! ").tempdir().unwrap();
        let shim = directory.path().join("launch.cmd");
        std::fs::write(&shim, b"@echo TALA_AGENT_SHIM_OK\r\n@exit /b 4\r\n").unwrap();
        let mut session = Session::start_invocation(super::agent_invocation(shim), "test agent", directory.path(), 80, 24).unwrap();
        let mut probe = Probe::default();
        let code = probe.until_exit(&mut session);
        let transcript = String::from_utf8_lossy(&probe.transcript);
        assert_eq!(code, Some(4), "{transcript}");
        assert!(transcript.contains("TALA_AGENT_SHIM_OK"), "{transcript}");
    }

    struct Probe {
        terminal: vt100::Parser<Replies>,
        transcript: Vec<u8>,
        deadline: Instant,
    }

    impl Default for Probe {
        fn default() -> Self {
            Self {
                terminal: vt100::Parser::new_with_callbacks(24, 80, 0, Replies::default()),
                transcript: Vec::new(),
                deadline: Instant::now() + Duration::from_secs(10),
            }
        }
    }

    impl Probe {
        fn receive(&mut self, session: &mut Session) -> Option<Option<u32>> {
            match session.events().recv_timeout(self.deadline.saturating_duration_since(Instant::now())) {
                Ok(Event::Data(bytes)) => {
                    self.terminal.process(&bytes);
                    self.terminal.callbacks_mut().flush(|reply| session.write(reply).expect("respond to terminal query"));
                    self.transcript.extend(bytes);
                    None
                }
                Ok(Event::Exit(code)) => Some(code),
                Ok(Event::InputError(error)) => panic!("{error}"),
                Err(error) => panic!("terminal event: {error}; transcript: {:?}", String::from_utf8_lossy(&self.transcript)),
            }
        }

        fn until_exit(&mut self, session: &mut Session) -> Option<u32> {
            loop {
                if let Some(code) = self.receive(session) { return code; }
            }
        }
    }

    fn test_shell_profile() -> Option<&'static str> {
        #[cfg(windows)]
        {
            return profiles()
                .into_iter()
                .find(|profile| profile.id == "cmd" && profile.available)
                .map(|profile| profile.id);
        }
        #[cfg(not(windows))]
        {
            profiles()
                .into_iter()
                .find(|profile| profile.kind == Kind::Shell && profile.available)
                .map(|profile| profile.id)
        }
    }
}
