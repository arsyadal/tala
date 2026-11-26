use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
const MAX_STATUS_BYTES: usize = 8 * 1024 * 1024;
const MAX_GIT_MESSAGE_BYTES: usize = 1024 * 1024;
const MAX_GIT_ERROR_BYTES: usize = 64 * 1024;
const MAX_REPO_PATH_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChange {
    pub path: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: String,
    pub changes: Vec<GitChange>,
}

pub fn list_directory(path: &Path) -> Result<Vec<DirectoryEntry>, String> {
    let root = std::path::absolute(path)
        .map_err(|error| format!("Unable to resolve directory '{}': {error}", path.display()))?;
    let entries = fs::read_dir(&root)
        .map_err(|error| format!("Unable to read directory '{}': {error}", root.display()))?;
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("Unable to read an entry in '{}': {error}", root.display()))?;
        let file_name = entry.file_name();
        if file_name == OsStr::new(".git") {
            continue;
        }
        let name = file_name.to_string_lossy().into_owned();
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Unable to inspect directory entry '{}': {error}",
                entry.path().display()
            )
        })?;
        result.push(DirectoryEntry {
            path: entry.path(),
            name,
            directory: file_type.is_dir(),
        });
    }

    result.sort_by(|left, right| match (left.directory, right.directory) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left.name.cmp(&right.name),
    });
    Ok(result)
}

pub fn read_text(path: &Path) -> Result<String, String> {
    ensure_not_symlink(path)?;
    let mut file = File::open(path)
        .map_err(|error| format!("Unable to open '{}': {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Unable to inspect '{}': {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a regular file", path.display()));
    }
    if metadata.len() > MAX_TEXT_BYTES as u64 {
        return Err(format!(
            "'{}' is larger than the 1 MiB preview limit",
            path.display()
        ));
    }

    let bytes = read_bounded(&mut file, MAX_TEXT_BYTES)
        .map_err(|error| format!("Unable to read '{}': {error}", path.display()))?;
    decode_text(bytes, path)
}


pub fn git_status(cwd: &Path) -> Result<Option<GitStatus>, String> {
    let Some(root) = repo_root(cwd)? else {
        return Ok(None);
    };
    let output = run_git(
        &root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
        MAX_STATUS_BYTES,
    )?;
    ensure_git_success("status", &output)?;
    let (branch, changes) = parse_status(&output.stdout)?;
    let branch = branch.ok_or_else(|| "Git status did not report the current branch".to_owned())?;
    Ok(Some(GitStatus { branch, changes }))
}

pub fn git_diff(cwd: &Path, path: &str, staged: bool) -> Result<String, String> {
    validate_repo_relative_path(path)?;
    let root = require_repo_root(cwd)?;

    if !staged && is_untracked(&root, path)? {
        let file = checked_repo_file(&root, path)?;
        return read_text(&file);
    }

    let output = if staged {
        run_git(
            &root,
            &[
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--cached",
                "--",
                path,
            ],
            MAX_DIFF_BYTES,
        )?
    } else {
        run_git(
            &root,
            &["diff", "--no-ext-diff", "--no-textconv", "--", path],
            MAX_DIFF_BYTES,
        )?
    };
    ensure_git_success("diff", &output)?;
    decode_git_stdout("diff", output.stdout)
}

pub fn git_stage(cwd: &Path, path: &str, stage: bool) -> Result<(), String> {
    validate_repo_relative_path(path)?;
    let root = require_repo_root(cwd)?;

    if stage {
        let output = run_git(&root, &["add", "--", path], MAX_GIT_MESSAGE_BYTES)?;
        return ensure_git_success("stage", &output);
    }

    let head = run_git(
        &root,
        &["rev-parse", "--verify", "--quiet", "HEAD"],
        MAX_REPO_PATH_BYTES,
    )?;
    let output = if head.status.success() {
        run_git(
            &root,
            &["reset", "--quiet", "HEAD", "--", path],
            MAX_GIT_MESSAGE_BYTES,
        )?
    } else if head.status.code() == Some(1) && head.stderr.is_empty() {
        run_git(
            &root,
            &["rm", "--cached", "--quiet", "--ignore-unmatch", "--", path],
            MAX_GIT_MESSAGE_BYTES,
        )?
    } else {
        return Err(git_failure("inspect HEAD", &head));
    };
    ensure_git_success("unstage", &output)
}

pub fn git_commit(cwd: &Path, message: &str) -> Result<String, String> {
    let root = require_repo_root(cwd)?;
    let output = run_git(
        &root,
        &["commit", "-m", message],
        MAX_GIT_MESSAGE_BYTES,
    )?;
    ensure_git_success("commit", &output)?;
    successful_git_message("commit", output)
}

pub fn git_sync(cwd: &Path, action: &str) -> Result<String, String> {
    let root = require_repo_root(cwd)?;
    let args: &[&str] = match action {
        "fetch" => &["fetch"],
        "pull" => &["pull", "--ff-only"],
        "push" => &["push"],
        _ => {
            return Err(format!(
                "Unsupported Git sync action '{action}'; expected fetch, pull, or push"
            ))
        }
    };
    let output = run_git(&root, args, MAX_GIT_MESSAGE_BYTES)?;
    ensure_git_success(action, &output)?;
    successful_git_message(action, output)
}

fn ensure_not_symlink(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Unable to inspect '{}': {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("Refusing to open symlink '{}'", path.display()));
    }
    Ok(())
}

fn read_bounded(reader: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("content exceeds the {limit}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn decode_text(bytes: Vec<u8>, path: &Path) -> Result<String, String> {
    if bytes.contains(&0) {
        return Err(format!(
            "'{}' appears to be a binary file (contains NUL bytes)",
            path.display()
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("'{}' is not valid UTF-8 text", path.display()))?;
    if text
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\t' | '\n' | '\r'))
    {
        return Err(format!(
            "'{}' appears to be a binary file (contains control characters)",
            path.display()
        ));
    }
    Ok(text)
}

fn validate_repo_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Git path cannot be empty".to_owned());
    }
    if path.as_bytes().contains(&0) {
        return Err("Git path cannot contain NUL bytes".to_owned());
    }

    let mut has_component = false;
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => has_component = true,
            Component::Prefix(_)
            | Component::RootDir
            | Component::ParentDir
            | Component::CurDir => {
                return Err(format!(
                    "Git path must be a non-escaping repository-relative path: '{path}'"
                ))
            }
        }
    }
    if !has_component {
        return Err("Git path cannot be empty".to_owned());
    }
    Ok(())
}

fn checked_repo_file(root: &Path, path: &str) -> Result<PathBuf, String> {
    let candidate = root.join(path);
    ensure_not_symlink(&candidate)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| format!("Unable to resolve repository '{}': {error}", root.display()))?;
    let canonical_file = fs::canonicalize(&candidate)
        .map_err(|error| format!("Unable to resolve '{}': {error}", candidate.display()))?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(format!(
            "Git path escapes the repository: '{path}'"
        ));
    }
    Ok(canonical_file)
}

fn repo_root(cwd: &Path) -> Result<Option<PathBuf>, String> {
    let metadata = fs::metadata(cwd)
        .map_err(|error| format!("Unable to inspect workspace '{}': {error}", cwd.display()))?;
    if !metadata.is_dir() {
        return Err(format!("Workspace '{}' is not a directory", cwd.display()));
    }

    let output = run_git(
        cwd,
        &["rev-parse", "--show-toplevel"],
        MAX_REPO_PATH_BYTES,
    )?;
    if !output.status.success() {
        let stderr = std::str::from_utf8(&output.stderr)
            .map_err(|_| "Git returned a non-UTF-8 repository discovery error".to_owned())?;
        if stderr.contains("not a git repository") {
            return Ok(None);
        }
        return Err(git_failure("discover repository", &output));
    }

    let mut stdout = output.stdout;
    if stdout.last() == Some(&b'\n') {
        stdout.pop();
        if stdout.last() == Some(&b'\r') {
            stdout.pop();
        }
    }
    if stdout.is_empty() {
        return Err("Git returned an empty repository root".to_owned());
    }
    let root = String::from_utf8(stdout)
        .map_err(|_| "Git repository root is not valid UTF-8".to_owned())?;
    Ok(Some(PathBuf::from(root)))
}

fn require_repo_root(cwd: &Path) -> Result<PathBuf, String> {
    repo_root(cwd)?.ok_or_else(|| format!("'{}' is not inside a Git repository", cwd.display()))
}

fn is_untracked(root: &Path, path: &str) -> Result<bool, String> {
    let output = run_git(
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=all",
            "--",
            path,
        ],
        MAX_STATUS_BYTES,
    )?;
    ensure_git_success("inspect path", &output)?;
    let (_, changes) = parse_status(&output.stdout)?;
    Ok(changes.into_iter().any(|change| change.untracked))
}

fn parse_status(bytes: &[u8]) -> Result<(Option<String>, Vec<GitChange>), String> {
    let mut records = bytes.split(|byte| *byte == 0);
    let mut branch = None;
    let mut changes = Vec::new();

    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(value) = record.strip_prefix(b"# branch.head ") {
            branch = Some(status_string(value, "branch name")?);
            continue;
        }
        if record.starts_with(b"# ") {
            continue;
        }

        match record.first().copied() {
            Some(b'1') => {
                let (xy, path) = split_status_fields(record, b"1", 9, "ordinary")?;
                changes.push(change_from_xy(path, xy, false)?);
            }
            Some(b'2') => {
                let (xy, path) = split_status_fields(record, b"2", 10, "rename")?;
                changes.push(change_from_xy(path, xy, false)?);
                if records.next().is_none() {
                    return Err("Git status omitted the original rename path".to_owned());
                }
            }
            Some(b'u') => {
                let (xy, path) = split_status_fields(record, b"u", 11, "unmerged")?;
                changes.push(change_from_xy(path, xy, false)?);
            }
            Some(b'?') if record.get(1) == Some(&b' ') => {
                changes.push(GitChange {
                    path: status_string(&record[2..], "untracked path")?,
                    staged: false,
                    unstaged: false,
                    untracked: true,
                });
            }
            Some(b'!') if record.get(1) == Some(&b' ') => {}
            _ => {
                return Err(format!(
                    "Git status returned an unsupported porcelain record '{}': expected v2",
                    String::from_utf8_lossy(record)
                ))
            }
        }
    }

    Ok((branch, changes))
}

fn split_status_fields<'a>(
    record: &'a [u8],
    expected_tag: &[u8],
    count: usize,
    kind: &str,
) -> Result<(&'a [u8], &'a [u8]), String> {
    let mut fields = record.splitn(count, |byte| *byte == b' ');
    let tag = fields.next();
    let xy = fields.next();
    let mut path = None;
    for _ in 2..count {
        path = fields.next();
        if path.is_none() {
            break;
        }
    }
    match (tag, xy, path) {
        (Some(tag), Some(xy), Some(path)) if tag == expected_tag && xy.len() == 2 => {
            Ok((xy, path))
        }
        _ => Err(format!("Git returned a malformed {kind} status record")),
    }
}

fn change_from_xy(path: &[u8], xy: &[u8], untracked: bool) -> Result<GitChange, String> {
    Ok(GitChange {
        path: status_string(path, "changed path")?,
        staged: xy[0] != b'.',
        unstaged: xy[1] != b'.',
        untracked,
    })
}

fn status_string(bytes: &[u8], description: &str) -> Result<String, String> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| format!("Git returned a {description} that is not valid UTF-8"))
}

struct GitOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_git(cwd: &Path, args: &[&str], stdout_limit: usize) -> Result<GitOutput, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(cwd)
        .arg("--no-pager")
        .arg("--literal-pathspecs")
        .arg("-c")
        .arg("color.ui=false")
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_COMMON_DIR")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_windows_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Unable to start Git: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Unable to capture Git standard output".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Unable to capture Git error output".to_owned())?;

    let stdout_reader = thread::spawn(move || capture_bounded(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || capture_bounded(stderr, MAX_GIT_ERROR_BYTES));
    let status = child
        .wait()
        .map_err(|error| format!("Unable to wait for Git: {error}"))?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| "Git standard-output reader stopped unexpectedly".to_owned())?
        .map_err(|error| format!("Unable to read Git standard output: {error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Git error-output reader stopped unexpectedly".to_owned())?
        .map_err(|error| format!("Unable to read Git error output: {error}"))?;

    Ok(GitOutput {
        status,
        stdout,
        stderr,
    })
}

fn capture_bounded(mut reader: impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(limit.min(16 * 1024));
    reader
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Git output exceeds the {limit}-byte limit"),
        ));
    }
    Ok(bytes)
}

#[cfg(windows)]
fn hide_windows_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_windows_console(_command: &mut Command) {}

fn ensure_git_success(operation: &str, output: &GitOutput) -> Result<(), String> {
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure(operation, output))
    }
}

fn git_failure(operation: &str, output: &GitOutput) -> String {
    if !output.stderr.is_empty() {
        let detail = std::str::from_utf8(&output.stderr)
            .unwrap_or("Git returned non-UTF-8 error output");
        return format!("Git {operation} failed: {}", detail.trim_end());
    }
    if !output.stdout.is_empty() {
        let detail =
            std::str::from_utf8(&output.stdout).unwrap_or("Git returned non-UTF-8 output");
        return format!("Git {operation} failed: {}", detail.trim_end());
    }
    format!("Git {operation} failed: exit status {}", output.status)
}

fn decode_git_stdout(operation: &str, stdout: Vec<u8>) -> Result<String, String> {
    String::from_utf8(stdout)
        .map_err(|_| format!("Git {operation} returned output that is not valid UTF-8"))
}

fn successful_git_message(operation: &str, output: GitOutput) -> Result<String, String> {
    let stdout = decode_git_stdout(operation, output.stdout)?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|_| format!("Git {operation} returned an error message that is not valid UTF-8"))?;
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => Ok(format!(
            "{}\n{}",
            stdout.trim_end(),
            stderr.trim_end()
        )),
        (false, true) => Ok(stdout),
        (true, false) => Ok(stderr),
        (true, true) => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn git_workflow_handles_nested_workspace_and_literal_path() {
        let temp = TempDir::new().expect("create temporary repository");
        setup_git(temp.path(), &["init"]);
        setup_git(temp.path(), &["config", "user.name", "Tala Test"]);
        setup_git(
            temp.path(),
            &["config", "user.email", "tala@example.invalid"],
        );
        setup_git(temp.path(), &["config", "core.autocrlf", "false"]);

        let workspace = temp.path().join("nested").join("workspace");
        fs::create_dir_all(&workspace).expect("create nested workspace");
        let file_name = "notes [literal].txt";
        let file_path = temp.path().join(file_name);
        fs::write(&file_path, "first line\n").expect("write untracked file");

        let status = git_status(&workspace)
            .expect("read status")
            .expect("repository status");
        let change = status
            .changes
            .iter()
            .find(|change| change.path == file_name)
            .expect("repo-root-relative untracked change");
        assert!(change.untracked);
        assert_eq!(
            git_diff(&workspace, file_name, false).unwrap(),
            "first line\n"
        );

        git_stage(&workspace, file_name, true).expect("stage literal path");
        let status = git_status(&workspace).unwrap().unwrap();
        assert!(status
            .changes
            .iter()
            .any(|change| change.path == file_name && change.staged));

        git_stage(&workspace, file_name, false).expect("unstage in unborn branch");
        let status = git_status(&workspace).unwrap().unwrap();
        assert!(status
            .changes
            .iter()
            .any(|change| change.path == file_name && change.untracked));

        git_stage(&workspace, file_name, true).expect("restage literal path");
        let commit = git_commit(&workspace, "Initial commit").expect("commit staged file");
        assert!(commit.contains("Initial commit"));
        assert!(git_status(&workspace).unwrap().unwrap().changes.is_empty());

        fs::write(&file_path, "second line\n").expect("modify tracked text");
        git_stage(&workspace, file_name, true).expect("stage tracked modification");
        git_stage(&workspace, file_name, false).expect("unstage tracked modification");
        let status = git_status(&workspace).unwrap().unwrap();
        assert!(status.changes.iter().any(|change| {
            change.path == file_name && change.unstaged && !change.staged && !change.untracked
        }));
    }

    fn setup_git(cwd: &Path, args: &[&str]) {
        let output = run_git(cwd, args, MAX_GIT_MESSAGE_BYTES).expect("run setup Git command");
        assert!(
            output.status.success(),
            "{}",
            git_failure("test setup", &output)
        );
    }
}
