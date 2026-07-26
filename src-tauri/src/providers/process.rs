use std::{
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::Duration,
};
#[cfg(unix)]
use tokio::time::sleep;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufWriter},
    process::{Child, ChildStdin, Command},
    sync::mpsc,
    time::timeout,
};

#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
};

const MAX_LINE: usize = 1024 * 1024;
const STDERR_TAIL: usize = 8 * 1024;
const MAX_VERSION_STREAM: usize = 64 * 1024;

enum ProcessLine {
    Stdout(String),
    Stderr(String),
    Limit(&'static str),
}

pub enum ProcessOutput {
    Stdout(String),
    Exited(ExitStatus),
}

pub struct JsonProcess {
    label: String,
    child: Child,
    stdin: BufWriter<ChildStdin>,
    receiver: mpsc::Receiver<ProcessLine>,
    stderr_tail: String,
    #[cfg(windows)]
    job: WindowsJob,
}

#[cfg(windows)]
pub(crate) struct WindowsJob {
    handle: HANDLE,
    root_pid: u32,
    terminated: bool,
}

#[cfg(windows)]
// The handle is owned exclusively by this wrapper and is valid to move between
// runtime worker threads; every access is through thread-safe kernel APIs.
unsafe impl Send for WindowsJob {}

#[cfg(windows)]
impl WindowsJob {
    pub(crate) fn assign(child: &Child) -> Result<Self, String> {
        let root_pid = child
            .id()
            .ok_or_else(|| "spawned process did not expose a Windows process id".to_string())?;
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(format!(
                "could not create a Windows process job: {}",
                std::io::Error::last_os_error()
            ));
        }
        let job = Self {
            handle,
            root_pid,
            terminated: false,
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.handle,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(format!(
                "could not configure a Windows process job: {}",
                std::io::Error::last_os_error()
            ));
        }
        let process = child
            .raw_handle()
            .ok_or_else(|| "spawned process did not expose a Windows handle".to_string())?
            as HANDLE;
        if unsafe { AssignProcessToJobObject(job.handle, process) } == 0 {
            return Err(format!(
                "could not assign the provider to a Windows process job: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(job)
    }

    pub(crate) fn terminate(&mut self, taskkill_root: bool) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        // Job association happens immediately after spawn, but an npm `.cmd`
        // shim could theoretically create a child first. taskkill follows the
        // root tree and closes that practical race; the Job Object remains the
        // non-async kill-on-drop guarantee for every assigned descendant.
        if taskkill_root {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &self.root_pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        unsafe {
            TerminateJobObject(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        self.terminate(false);
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

impl JsonProcess {
    pub async fn spawn(
        mut command: Command,
        label: impl Into<String>,
        max_stream: usize,
    ) -> Result<Self, String> {
        let label = label.into();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        configure_process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to start {label}: {error}"))?;
        #[cfg(windows)]
        let job = match WindowsJob::assign(&child) {
            Ok(job) => job,
            Err(error) => {
                terminate_child(&mut child).await;
                return Err(format!("Failed to contain {label}: {error}"));
            }
        };
        let Some(stdin) = child.stdin.take() else {
            terminate_child(&mut child).await;
            return Err(format!("{label} stdin was unavailable"));
        };
        let Some(stdout) = child.stdout.take() else {
            terminate_child(&mut child).await;
            return Err(format!("{label} stdout was unavailable"));
        };
        let Some(stderr) = child.stderr.take() else {
            terminate_child(&mut child).await;
            return Err(format!("{label} stderr was unavailable"));
        };
        let (sender, receiver) = mpsc::channel(256);
        tokio::spawn(pump_stream(stdout, sender.clone(), true, max_stream));
        tokio::spawn(pump_stream(stderr, sender, false, max_stream));
        Ok(Self {
            label,
            child,
            stdin: BufWriter::new(stdin),
            receiver,
            stderr_tail: String::new(),
            #[cfg(windows)]
            job,
        })
    }

    pub async fn send_json(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let mut line = serde_json::to_vec(value).map_err(|error| error.to_string())?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .map_err(|error| format!("Failed to write to {}: {error}", self.label))?;
        self.stdin
            .flush()
            .await
            .map_err(|error| format!("Failed to flush {} input: {error}", self.label))
    }

    pub async fn close_stdin(&mut self) -> Result<(), String> {
        self.stdin
            .shutdown()
            .await
            .map_err(|error| format!("Failed to close {} input: {error}", self.label))
    }

    pub async fn next_stdout(&mut self) -> Result<ProcessOutput, String> {
        loop {
            match self.receiver.recv().await {
                Some(ProcessLine::Stdout(line)) => return Ok(ProcessOutput::Stdout(line)),
                Some(ProcessLine::Stderr(line)) => push_tail(&mut self.stderr_tail, &line),
                Some(ProcessLine::Limit(stream)) => {
                    return Err(format!("{} produced too much {stream} output", self.label));
                }
                None => {
                    let status = self.child.wait().await.map_err(|error| error.to_string())?;
                    return Ok(ProcessOutput::Exited(status));
                }
            }
        }
    }

    pub fn stderr_tail(&self) -> &str {
        self.stderr_tail.trim()
    }

    pub async fn shutdown(&mut self) {
        terminate_child(&mut self.child).await;
    }
}

impl Drop for JsonProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        self.job.terminate(self.child.id().is_some());
    }
}

pub fn find_executable(command: &str) -> Option<PathBuf> {
    if command.is_empty() {
        return None;
    }
    if let Ok(path) = which::which(command) {
        return Some(path);
    }
    let home = dirs::home_dir();
    let mut directories = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = home {
        directories.extend([
            home.join(".local/bin"),
            home.join(".local/share/pnpm"),
            home.join(".npm-global/bin"),
            home.join(".kimi-code/bin"),
            home.join(".cargo/bin"),
        ]);
        #[cfg(windows)]
        directories.push(home.join("AppData/Roaming/npm"));
    }
    let mut candidates = Vec::new();
    for directory in directories {
        candidates.push(directory.join(command));
        #[cfg(windows)]
        {
            candidates.push(directory.join(format!("{command}.exe")));
            candidates.push(directory.join(format!("{command}.cmd")));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

pub async fn probe_version(path: &Path) -> Option<String> {
    let command = platform_command(path, &["--version".to_string()]);
    let mut process = JsonProcess::spawn(command, "provider version probe", MAX_VERSION_STREAM)
        .await
        .ok()?;
    if process.close_stdin().await.is_err() {
        process.shutdown().await;
        return None;
    }
    let result = timeout(Duration::from_secs(4), async {
        let mut first_stdout = None;
        loop {
            match process.next_stdout().await? {
                ProcessOutput::Stdout(line) => {
                    if first_stdout.is_none() && !line.trim().is_empty() {
                        first_stdout = Some(line);
                    }
                }
                ProcessOutput::Exited(_) => {
                    return Ok::<_, String>(first_stdout.or_else(|| {
                        process
                            .stderr_tail()
                            .lines()
                            .find(|line| !line.trim().is_empty())
                            .map(str::to_string)
                    }));
                }
            }
        }
    })
    .await;
    let value = match result {
        Ok(Ok(Some(value))) => value,
        Ok(Ok(None)) => return None,
        Ok(Err(_)) | Err(_) => {
            process.shutdown().await;
            return None;
        }
    };
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

pub fn platform_command(path: &Path, args: &[String]) -> Command {
    // On Windows, pass `.cmd`/`.bat` paths directly to Rust's process layer.
    // Rust applies its dedicated batch-file escaping and rejects arguments it
    // cannot represent safely. Manually invoking `cmd.exe /C` here would bypass
    // that protection for user-controlled prompts and model identifiers.
    let mut command = Command::new(path);
    command.args(args);
    command
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command
            .as_std_mut()
            .creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

async fn pump_stream<R>(
    mut reader: R,
    sender: mpsc::Sender<ProcessLine>,
    stdout: bool,
    max_stream: usize,
) where
    R: AsyncRead + Unpin,
{
    let stream_name = if stdout { "stdout" } else { "stderr" };
    let mut pending = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut total = 0_usize;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => return,
        };
        total = total.saturating_add(read);
        if total > max_stream {
            let _ = sender.send(ProcessLine::Limit(stream_name)).await;
            return;
        }
        pending.extend_from_slice(&buffer[..read]);
        while let Some(index) = pending.iter().position(|byte| *byte == b'\n') {
            let mut line = pending.drain(..=index).collect::<Vec<_>>();
            if line.last() == Some(&b'\n') {
                line.pop();
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let value = String::from_utf8_lossy(&line).into_owned();
            let item = if stdout {
                ProcessLine::Stdout(value)
            } else {
                ProcessLine::Stderr(value)
            };
            if sender.send(item).await.is_err() {
                return;
            }
        }
        if pending.len() > MAX_LINE {
            let _ = sender.send(ProcessLine::Limit(stream_name)).await;
            return;
        }
    }
    if !pending.is_empty() {
        let value = String::from_utf8_lossy(&pending).into_owned();
        let item = if stdout {
            ProcessLine::Stdout(value)
        } else {
            ProcessLine::Stderr(value)
        };
        let _ = sender.send(item).await;
    }
}

async fn terminate_child(child: &mut Child) {
    let pid = child.id();
    if let Some(pid) = pid {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
        }
    }
    #[cfg(unix)]
    {
        sleep(Duration::from_millis(400)).await;
        if let Some(pid) = pid {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
    if timeout(Duration::from_secs(2), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

fn push_tail(target: &mut String, line: &str) {
    target.push_str(line);
    target.push('\n');
    if target.len() > STDERR_TAIL {
        let mut start = target.len() - STDERR_TAIL;
        while start < target.len() && !target.is_char_boundary(start) {
            start += 1;
        }
        *target = target[start..].to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::{platform_command, probe_version, push_tail};
    use std::{ffi::OsStr, path::Path};

    #[test]
    fn platform_command_keeps_the_resolved_executable_as_the_program() {
        let arguments = vec!["literal & shell metacharacters".to_string()];
        let command = platform_command(Path::new("provider.cmd"), &arguments);
        assert_eq!(command.as_std().get_program(), OsStr::new("provider.cmd"));
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            vec![OsStr::new("literal & shell metacharacters")]
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn batch_shims_cannot_execute_prompt_metacharacters() {
        let root =
            std::env::temp_dir().join(format!("onyx-batch-argument-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let shim = root.join("provider.cmd");
        let marker = root.join("injected.txt");
        std::fs::write(&shim, "@echo off\r\nexit /b 0\r\n").unwrap();

        let argument = format!("safe & echo injected > {}", marker.display());
        let _ = platform_command(&shim, &[argument]).output().await;

        assert!(!marker.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stderr_tail_truncates_multibyte_text() {
        let mut value = "é".repeat(5_000);
        push_tail(&mut value, "done");
        assert!(value.len() <= 8 * 1024);
        assert!(value.ends_with("done\n"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_probe_reads_a_bounded_process_stream() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("onyx-version-probe-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let provider = root.join("provider");
        std::fs::write(&provider, "#!/bin/sh\nprintf 'provider 1.2.3\\n'\n").unwrap();
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(
            probe_version(&provider).await.as_deref(),
            Some("provider 1.2.3")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn version_probe_rejects_oversized_output() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("onyx-version-probe-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let provider = root.join("provider");
        std::fs::write(&provider, "#!/bin/sh\n/usr/bin/head -c 70000 /dev/zero\n").unwrap();
        std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(probe_version(&provider).await.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}
