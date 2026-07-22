use crate::providers::process::{JsonProcess, ProcessOutput, find_executable, platform_command};
use serde::Serialize;
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::time::{Instant, timeout_at};

const MAX_COMMAND_OUTPUT: usize = 2 * 1024 * 1024;
const MAX_DIFF_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_CHANGED_FILES: usize = 1_000;
const MAX_UNTRACKED_DIFF_FILES: usize = 256;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoFileChange {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub is_repo: bool,
    pub branch: Option<String>,
    pub changed_files: Vec<RepoFileChange>,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub has_remote: bool,
    pub pr_commit_count: Option<u32>,
    pub pr_url: Option<String>,
}

impl RepoSummary {
    fn not_repo() -> Self {
        Self {
            is_repo: false,
            branch: None,
            changed_files: Vec::new(),
            staged_count: 0,
            unstaged_count: 0,
            untracked_count: 0,
            ahead: 0,
            behind: 0,
            has_upstream: false,
            has_remote: false,
            pr_commit_count: None,
            pr_url: None,
        }
    }

    fn has_changes(&self) -> bool {
        self.staged_count > 0 || self.unstaged_count > 0 || self.untracked_count > 0
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFile {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorTarget {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitActionResult {
    pub message: String,
    pub url: Option<String>,
}

struct CommandResult {
    success: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

pub fn canonical_workspace(workspace: &str) -> Result<PathBuf, String> {
    if workspace.trim().is_empty() {
        return Err("Choose a workspace first".to_string());
    }
    let path = Path::new(workspace)
        .canonicalize()
        .map_err(|error| format!("Workspace is unavailable: {error}"))?;
    if !path.is_dir() {
        return Err("Workspace must be a directory".to_string());
    }
    Ok(path)
}

pub async fn repo_summary(workspace: String) -> Result<RepoSummary, String> {
    let workspace = canonical_workspace(&workspace)?;
    let Some(git) = find_executable("git") else {
        return Err("Git is not installed or is unavailable on PATH".to_string());
    };
    let root_result = run_command(
        &git,
        &workspace,
        &["rev-parse", "--show-toplevel"],
        "Git repository check",
        Duration::from_secs(8),
    )
    .await?;
    if !root_result.success {
        return Ok(RepoSummary::not_repo());
    }
    let repository_root = Path::new(trim_line_end(&root_result.stdout))
        .canonicalize()
        .map_err(|error| format!("Git reported an unavailable repository root: {error}"))?;
    // Git searches parent directories automatically. Requiring the selected workspace to be
    // the repository root prevents a workspace scoped to a subdirectory from exposing or
    // mutating files outside that selected boundary.
    if repository_root != workspace {
        return Ok(RepoSummary::not_repo());
    }
    repo_summary_at(&git, &workspace).await
}

async fn repo_summary_at(git: &Path, workspace: &Path) -> Result<RepoSummary, String> {
    let status = checked(
        "Read Git status",
        run_command(
            git,
            workspace,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            "Git status",
            Duration::from_secs(15),
        )
        .await?,
    )?;
    let ParsedStatus {
        changed_files,
        staged_count,
        unstaged_count,
        untracked_count,
    } = parse_porcelain_status(&status.stdout);

    let branch_result = run_command(
        git,
        workspace,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        "Git branch",
        Duration::from_secs(8),
    )
    .await?;
    let branch = branch_result
        .success
        .then(|| non_empty(branch_result.stdout.trim()))
        .flatten();

    let upstream_result = run_command(
        git,
        workspace,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
        "Git upstream",
        Duration::from_secs(8),
    )
    .await?;
    let has_upstream = upstream_result.success && !upstream_result.stdout.trim().is_empty();
    let (behind, ahead) = if has_upstream {
        let counts = run_command(
            git,
            workspace,
            &["rev-list", "--left-right", "--count", "@{upstream}...HEAD"],
            "Git ahead/behind",
            Duration::from_secs(10),
        )
        .await?;
        if counts.success {
            parse_ahead_behind(&counts.stdout)
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    let remotes = run_command(
        git,
        workspace,
        &["remote"],
        "Git remotes",
        Duration::from_secs(8),
    )
    .await?;
    let remote_names = if remotes.success {
        remotes
            .stdout
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let primary_remote = remote_names
        .iter()
        .find(|remote| remote.as_str() == "origin")
        .or_else(|| (remote_names.len() == 1).then(|| &remote_names[0]));
    let has_remote = !remote_names.is_empty();
    let pr_commit_count = if let (Some(_), Some(remote)) = (branch.as_deref(), primary_remote) {
        pull_request_commit_count(git, workspace, remote).await
    } else {
        None
    };

    let pr_url = if branch.is_some() && has_remote && find_executable("gh").is_some() {
        current_pr_url(workspace).await.ok().flatten()
    } else {
        None
    };

    Ok(RepoSummary {
        is_repo: true,
        branch,
        changed_files,
        staged_count,
        unstaged_count,
        untracked_count,
        ahead,
        behind,
        has_upstream,
        has_remote,
        pr_commit_count,
        pr_url,
    })
}

async fn pull_request_commit_count(git: &Path, workspace: &Path, remote: &str) -> Option<u32> {
    let remote_head = format!("refs/remotes/{remote}/HEAD");
    let symbolic = run_command_owned(
        git,
        workspace,
        vec![
            "symbolic-ref".to_string(),
            "--quiet".to_string(),
            "--short".to_string(),
            remote_head,
        ],
        "Git default branch",
        Duration::from_secs(8),
    )
    .await
    .ok();
    let mut base = symbolic
        .filter(|result| result.success)
        .and_then(|result| non_empty(&result.stdout));
    if base.is_none() {
        for name in ["main", "master"] {
            let short = format!("{remote}/{name}");
            let reference = format!("refs/remotes/{short}");
            let exists = run_command_owned(
                git,
                workspace,
                vec![
                    "show-ref".to_string(),
                    "--verify".to_string(),
                    "--quiet".to_string(),
                    reference,
                ],
                "Git default branch fallback",
                Duration::from_secs(8),
            )
            .await
            .ok();
            if exists.is_some_and(|result| result.success) {
                base = Some(short);
                break;
            }
        }
    }
    let base = base?;
    let range = format!("{base}..HEAD");
    let count = run_command_owned(
        git,
        workspace,
        vec!["rev-list".to_string(), "--count".to_string(), range],
        "Git pull request commits",
        Duration::from_secs(10),
    )
    .await
    .ok()?;
    count
        .success
        .then(|| count.stdout.trim().parse().ok())
        .flatten()
}

pub async fn git_diff(workspace: String) -> Result<String, String> {
    let (git, workspace) = repository_root(&workspace).await?;
    let staged = checked(
        "Read staged diff",
        run_command(
            &git,
            &workspace,
            &[
                "diff",
                "--cached",
                "--no-ext-diff",
                "--no-color",
                "--find-renames",
            ],
            "Staged Git diff",
            Duration::from_secs(20),
        )
        .await?,
    )?
    .stdout;
    let unstaged = checked(
        "Read working tree diff",
        run_command(
            &git,
            &workspace,
            &["diff", "--no-ext-diff", "--no-color", "--find-renames"],
            "Working tree Git diff",
            Duration::from_secs(20),
        )
        .await?,
    )?
    .stdout;
    let untracked = untracked_diff(&git, &workspace).await?;
    Ok(bounded_diff([staged, unstaged, untracked]))
}

async fn untracked_diff(git: &Path, workspace: &Path) -> Result<String, String> {
    let listed = checked(
        "Read untracked files",
        run_command(
            git,
            workspace,
            &["ls-files", "--others", "--exclude-standard", "-z"],
            "Untracked Git files",
            Duration::from_secs(15),
        )
        .await?,
    )?;
    #[cfg(windows)]
    let null_path = "NUL";
    #[cfg(not(windows))]
    let null_path = "/dev/null";
    let paths = listed
        .stdout
        .split('\0')
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    let truncated = paths.len() > MAX_UNTRACKED_DIFF_FILES;
    let mut output = String::new();
    for path in paths.into_iter().take(MAX_UNTRACKED_DIFF_FILES) {
        let diff = run_command_owned(
            git,
            workspace,
            vec![
                "diff".to_string(),
                "--no-index".to_string(),
                "--no-ext-diff".to_string(),
                "--no-color".to_string(),
                "--".to_string(),
                null_path.to_string(),
                path.to_string(),
            ],
            "Untracked file diff",
            Duration::from_secs(15),
        )
        .await?;
        if !diff.success && diff.code != Some(1) {
            return checked("Read untracked file diff", diff).map(|_| String::new());
        }
        if !diff.stdout.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&diff.stdout);
        }
        if output.len() >= MAX_DIFF_OUTPUT {
            break;
        }
    }
    if truncated {
        output.push_str("\n\n# zAI: additional untracked files omitted from this preview.\n");
    }
    Ok(output)
}

fn bounded_diff(parts: impl IntoIterator<Item = String>) -> String {
    let mut output = String::new();
    for part in parts.into_iter().filter(|part| !part.is_empty()) {
        if !output.is_empty() {
            output.push('\n');
        }
        let remaining = MAX_DIFF_OUTPUT.saturating_sub(output.len());
        if part.len() <= remaining {
            output.push_str(&part);
            continue;
        }
        let mut end = remaining;
        while end > 0 && !part.is_char_boundary(end) {
            end -= 1;
        }
        output.push_str(&part[..end]);
        output.push_str("\n\n# zAI: diff preview truncated at 4 MiB.\n");
        break;
    }
    output
}

pub fn read_file(workspace: String, path: String) -> Result<WorkspaceFile, String> {
    let workspace = canonical_workspace(&workspace)?;
    let target = scoped_existing_path(&workspace, &path)?;
    if !target.is_file() {
        return Err("The selected workspace entry is not a file".to_string());
    }
    let metadata = target
        .metadata()
        .map_err(|error| format!("Unable to inspect file: {error}"))?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len())
            .unwrap_or(MAX_FILE_BYTES + 1)
            .min(MAX_FILE_BYTES + 1),
    );
    std::fs::File::open(&target)
        .map_err(|error| format!("Unable to open file: {error}"))?
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Unable to read file: {error}"))?;
    if bytes.iter().take(MAX_FILE_BYTES).any(|byte| *byte == 0) {
        return Err("Binary files cannot be previewed as text".to_string());
    }
    let truncated = bytes.len() > MAX_FILE_BYTES;
    if truncated {
        bytes.truncate(MAX_FILE_BYTES);
    }
    let content = match std::str::from_utf8(&bytes) {
        Ok(content) => content.to_string(),
        Err(error) if truncated && error.error_len().is_none() => {
            String::from_utf8(bytes[..error.valid_up_to()].to_vec())
                .map_err(|_| "This file is not valid UTF-8 and cannot be previewed".to_string())?
        }
        Err(_) => return Err("This file is not valid UTF-8 and cannot be previewed".to_string()),
    };
    let relative = target
        .strip_prefix(&workspace)
        .map_err(|_| "File escaped the selected workspace".to_string())?;
    Ok(WorkspaceFile {
        path: relative.to_string_lossy().into_owned(),
        content,
        truncated,
    })
}

pub fn editors() -> Vec<EditorTarget> {
    let mut targets = Vec::new();
    for (id, label, command, mac_app) in [
        ("cursor", "Cursor", "cursor", "Cursor.app"),
        ("vscode", "VS Code", "code", "Visual Studio Code.app"),
        (
            "vscode-insiders",
            "VS Code Insiders",
            "code-insiders",
            "Visual Studio Code - Insiders.app",
        ),
        ("vscodium", "VSCodium", "codium", "VSCodium.app"),
        ("zed", "Zed", "zed", "Zed.app"),
        ("sublime", "Sublime Text", "subl", "Sublime Text.app"),
    ] {
        targets.push(EditorTarget {
            id: id.to_string(),
            label: label.to_string(),
            available: find_executable(command).is_some() || mac_application_exists(mac_app),
        });
    }
    // Match T3's Open action: prefer an installed code editor and retain the
    // platform file manager as a dependable fallback at the end of the menu.
    #[cfg(target_os = "macos")]
    targets.push(EditorTarget {
        id: "finder".to_string(),
        label: "Finder".to_string(),
        available: Path::new("/usr/bin/open").is_file(),
    });
    #[cfg(target_os = "windows")]
    targets.push(EditorTarget {
        id: "explorer".to_string(),
        label: "Explorer".to_string(),
        available: true,
    });
    #[cfg(all(unix, not(target_os = "macos")))]
    targets.push(EditorTarget {
        id: "files".to_string(),
        label: "Files".to_string(),
        available: find_executable("xdg-open").is_some(),
    });
    targets
}

pub fn open_workspace(workspace: String, target: String) -> Result<(), String> {
    let workspace = canonical_workspace(&workspace)?;
    let target = target.trim();
    match target {
        "finder" | "explorer" | "files" | "file-manager" => open_file_manager(&workspace),
        "cursor" => open_editor(&workspace, "cursor", "Cursor"),
        "vscode" => open_editor(&workspace, "code", "Visual Studio Code"),
        "vscode-insiders" => {
            open_editor(&workspace, "code-insiders", "Visual Studio Code - Insiders")
        }
        "vscodium" => open_editor(&workspace, "codium", "VSCodium"),
        "zed" => open_editor(&workspace, "zed", "Zed"),
        "sublime" => open_editor(&workspace, "subl", "Sublime Text"),
        _ => Err(format!("Unknown workspace opener: {target}")),
    }
}

pub async fn commit(workspace: String, message: Option<String>) -> Result<GitActionResult, String> {
    let (git, workspace) = repository_root(&workspace).await?;
    let before = repo_summary_at(&git, &workspace).await?;
    if !before.has_changes() {
        return Err("There are no workspace changes to commit".to_string());
    }
    let message = normalized_commit_message(message, &before)?;
    let add = run_command(
        &git,
        &workspace,
        &["add", "--all"],
        "Stage workspace changes",
        Duration::from_secs(60),
    )
    .await?;
    checked("Stage workspace changes", add)?;

    let commit = run_command_owned(
        &git,
        &workspace,
        vec!["commit".to_string(), "-m".to_string(), message.clone()],
        "Commit workspace changes",
        Duration::from_secs(10 * 60),
    )
    .await?;
    let commit = checked("Commit workspace changes after staging", commit)?;
    Ok(GitActionResult {
        message: useful_output(&commit).unwrap_or_else(|| format!("Committed: {message}")),
        url: None,
    })
}

pub async fn push(workspace: String) -> Result<GitActionResult, String> {
    let (git, workspace) = repository_root(&workspace).await?;
    let summary = repo_summary_at(&git, &workspace).await?;
    let output = push_current(&git, &workspace, &summary, false).await?;
    Ok(GitActionResult {
        message: useful_output(&output).unwrap_or_else(|| "Pushed current branch".to_string()),
        url: None,
    })
}

pub async fn create_pr(workspace: String) -> Result<GitActionResult, String> {
    let (git, workspace) = repository_root(&workspace).await?;
    let mut summary = repo_summary_at(&git, &workspace).await?;
    if let Some(url) = summary.pr_url.clone() {
        return Ok(GitActionResult {
            message: "Pull request already exists".to_string(),
            url: Some(url),
        });
    }
    let branch = summary
        .branch
        .clone()
        .ok_or_else(|| "Cannot create a pull request from detached HEAD".to_string())?;
    if summary.has_changes() {
        return Err("Commit local changes before creating a pull request".to_string());
    }
    if !summary.has_remote {
        return Err("Add a Git remote before creating a pull request".to_string());
    }
    if summary.pr_commit_count == Some(0) {
        return Err("There are no branch commits to include in a pull request".to_string());
    }

    let Some(gh) = find_executable("gh") else {
        return Err("GitHub CLI (`gh`) is required to create a pull request".to_string());
    };
    let auth = run_command(
        &gh,
        &workspace,
        &["auth", "status"],
        "GitHub authentication check",
        Duration::from_secs(15),
    )
    .await?;
    checked(
        "GitHub CLI is not authenticated; run `gh auth login` and retry",
        auth,
    )?;

    let pushed = !summary.has_upstream || summary.ahead > 0;
    if pushed {
        push_current(&git, &workspace, &summary, true)
            .await
            .map_err(|error| {
                format!("Pull request was not created because push failed: {error}")
            })?;
        summary = repo_summary_at(&git, &workspace).await?;
        if let Some(url) = summary.pr_url.clone() {
            return Ok(GitActionResult {
                message: "Pull request is ready".to_string(),
                url: Some(url),
            });
        }
    }

    let created = run_command_owned(
        &gh,
        &workspace,
        vec![
            "pr".to_string(),
            "create".to_string(),
            "--fill".to_string(),
            "--head".to_string(),
            branch,
        ],
        "Create GitHub pull request",
        Duration::from_secs(60),
    )
    .await?;
    let created = checked(
        if pushed {
            "The branch was pushed, but GitHub pull request creation failed"
        } else {
            "Create GitHub pull request"
        },
        created,
    )?;
    let url = extract_https_url(&created.stdout)
        .or_else(|| extract_https_url(&created.stderr))
        .or(current_pr_url(&workspace).await?);
    Ok(GitActionResult {
        message: "Pull request created".to_string(),
        url,
    })
}

async fn push_current(
    git: &Path,
    workspace: &Path,
    summary: &RepoSummary,
    allow_no_ahead: bool,
) -> Result<CommandResult, String> {
    if summary.has_changes() {
        return Err("Commit or stash local changes before pushing".to_string());
    }
    if summary.branch.is_none() {
        return Err("Cannot push from detached HEAD".to_string());
    }
    if !summary.has_remote {
        return Err("Add a Git remote before pushing".to_string());
    }
    if summary.has_upstream && summary.ahead == 0 && !allow_no_ahead {
        return Err("There are no local commits to push".to_string());
    }
    if summary.behind > 0 && summary.ahead > 0 {
        return Err(
            "The branch has diverged from upstream; reconcile it before pushing".to_string(),
        );
    }
    let args = if summary.has_upstream {
        vec!["push".to_string(), "--porcelain".to_string()]
    } else {
        let remotes = checked(
            "Read Git remotes",
            run_command(
                git,
                workspace,
                &["remote"],
                "Git remotes",
                Duration::from_secs(8),
            )
            .await?,
        )?;
        let remotes = remotes
            .stdout
            .lines()
            .map(str::trim)
            .filter(|remote| !remote.is_empty())
            .collect::<Vec<_>>();
        let remote = if remotes.contains(&"origin") {
            "origin"
        } else if remotes.len() == 1 {
            remotes[0]
        } else {
            return Err("Configure an upstream or an `origin` remote before pushing".to_string());
        };
        vec![
            "push".to_string(),
            "--porcelain".to_string(),
            "--set-upstream".to_string(),
            remote.to_string(),
            "HEAD".to_string(),
        ]
    };
    let result = run_command_owned(
        git,
        workspace,
        args,
        "Push current branch",
        Duration::from_secs(2 * 60),
    )
    .await?;
    checked("Push current branch", result)
}

async fn repository_root(workspace: &str) -> Result<(PathBuf, PathBuf), String> {
    let workspace = canonical_workspace(workspace)?;
    let git = find_executable("git")
        .ok_or_else(|| "Git is not installed or is unavailable on PATH".to_string())?;
    let result = run_command(
        &git,
        &workspace,
        &["rev-parse", "--show-toplevel"],
        "Git repository check",
        Duration::from_secs(8),
    )
    .await?;
    if !result.success {
        return Err("The selected workspace is not a Git repository".to_string());
    }
    let root = Path::new(trim_line_end(&result.stdout))
        .canonicalize()
        .map_err(|error| format!("Git reported an unavailable repository root: {error}"))?;
    if root != workspace {
        return Err("Select the Git repository root before using repository actions".to_string());
    }
    Ok((git, workspace))
}

async fn current_pr_url(workspace: &Path) -> Result<Option<String>, String> {
    let Some(gh) = find_executable("gh") else {
        return Ok(None);
    };
    let result = run_command(
        &gh,
        workspace,
        &["pr", "view", "--json", "url", "--jq", ".url"],
        "Read current pull request",
        Duration::from_secs(8),
    )
    .await?;
    if !result.success {
        return Ok(None);
    }
    Ok(extract_https_url(&result.stdout))
}

async fn run_command(
    executable: &Path,
    cwd: &Path,
    args: &[&str],
    label: &str,
    duration: Duration,
) -> Result<CommandResult, String> {
    run_command_owned(
        executable,
        cwd,
        args.iter().map(|value| (*value).to_string()).collect(),
        label,
        duration,
    )
    .await
}

async fn run_command_owned(
    executable: &Path,
    cwd: &Path,
    args: Vec<String>,
    label: &str,
    duration: Duration,
) -> Result<CommandResult, String> {
    let mut command = platform_command(executable, &args);
    command
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut process = JsonProcess::spawn(command, label, MAX_COMMAND_OUTPUT).await?;
    if let Err(error) = process.close_stdin().await {
        process.shutdown().await;
        return Err(error);
    }
    let deadline = Instant::now() + duration;
    let mut stdout = String::new();
    loop {
        let next = match timeout_at(deadline, process.next_stdout()).await {
            Ok(result) => result,
            Err(_) => {
                process.shutdown().await;
                return Err(format!(
                    "{label} timed out after {} seconds",
                    duration.as_secs()
                ));
            }
        };
        match next {
            Ok(ProcessOutput::Stdout(line)) => {
                if !stdout.is_empty() {
                    stdout.push('\n');
                }
                stdout.push_str(&line);
                if stdout.len() > MAX_COMMAND_OUTPUT {
                    process.shutdown().await;
                    return Err(format!("{label} produced too much output"));
                }
            }
            Ok(ProcessOutput::Exited(status)) => {
                return Ok(CommandResult {
                    success: status.success(),
                    code: status.code(),
                    stdout: redact_credentials(stdout),
                    stderr: redact_credentials(process.stderr_tail().to_string()),
                });
            }
            Err(error) => {
                process.shutdown().await;
                return Err(error);
            }
        }
    }
}

fn checked(context: &str, result: CommandResult) -> Result<CommandResult, String> {
    if result.success {
        return Ok(result);
    }
    let detail =
        useful_output(&result).unwrap_or_else(|| "No error detail was returned".to_string());
    match result.code {
        Some(code) => Err(format!("{context} failed (exit {code}): {detail}")),
        None => Err(format!("{context} failed: {detail}")),
    }
}

fn useful_output(result: &CommandResult) -> Option<String> {
    let value = if !result.stderr.trim().is_empty() {
        result.stderr.trim()
    } else {
        result.stdout.trim()
    };
    non_empty(value).map(|value| tail_chars(&value, 8 * 1024))
}

fn scoped_existing_path(workspace: &Path, value: &str) -> Result<PathBuf, String> {
    let raw = Path::new(value);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        validate_relative_path(raw)?;
        workspace.join(raw)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("Workspace path is unavailable: {error}"))?;
    if !canonical.starts_with(workspace) || canonical == workspace {
        return Err("Path must resolve to a file inside the selected workspace".to_string());
    }
    Ok(canonical)
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("Choose a workspace file first".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("Workspace file paths cannot escape the selected workspace".to_string());
    }
    Ok(())
}

fn normalized_commit_message(
    requested: Option<String>,
    summary: &RepoSummary,
) -> Result<String, String> {
    let requested = requested.unwrap_or_default();
    let value = if requested.trim().is_empty() {
        if summary.changed_files.len() == 1 {
            format!("Update {}", summary.changed_files[0].path)
        } else {
            "Update workspace changes".to_string()
        }
    } else {
        requested.trim().to_string()
    };
    if value.len() > 16 * 1024 {
        return Err("Commit messages are limited to 16 KiB".to_string());
    }
    if value.contains('\0') {
        return Err("Commit messages cannot contain NUL bytes".to_string());
    }
    Ok(value)
}

struct ParsedStatus {
    changed_files: Vec<RepoFileChange>,
    staged_count: usize,
    unstaged_count: usize,
    untracked_count: usize,
}

fn parse_porcelain_status(value: &str) -> ParsedStatus {
    let records = value.split('\0').collect::<Vec<_>>();
    let mut index = 0;
    let mut changed_files = Vec::new();
    let mut staged_count = 0;
    let mut unstaged_count = 0;
    let mut untracked_count = 0;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.len() < 3 {
            continue;
        }
        let bytes = record.as_bytes();
        let index_status = bytes[0] as char;
        let worktree_status = bytes[1] as char;
        let path = record.get(3..).unwrap_or_default();
        if path.is_empty() {
            continue;
        }
        if index_status == '?' && worktree_status == '?' {
            untracked_count += 1;
        } else {
            if index_status != ' ' {
                staged_count += 1;
            }
            if worktree_status != ' ' {
                unstaged_count += 1;
            }
        }
        let status = if index_status == '?' && worktree_status == '?' {
            "?".to_string()
        } else if worktree_status != ' ' {
            worktree_status.to_string()
        } else {
            index_status.to_string()
        };
        if changed_files.len() < MAX_CHANGED_FILES {
            changed_files.push(RepoFileChange {
                path: path.to_string(),
                status,
            });
        }
        if matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C') {
            // In porcelain -z mode the original rename/copy path is a second NUL record.
            index = index.saturating_add(1);
        }
    }
    ParsedStatus {
        changed_files,
        staged_count,
        unstaged_count,
        untracked_count,
    }
}

fn parse_ahead_behind(value: &str) -> (u32, u32) {
    let mut values = value.split_whitespace();
    let behind = values
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let ahead = values
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    (behind, ahead)
}

fn extract_https_url(value: &str) -> Option<String> {
    value
        .split_whitespace()
        .find(|part| part.starts_with("https://"))
        .map(|part| {
            part.trim_end_matches(|character: char| {
                matches!(character, '.' | ',' | ')' | ']' | '}' | '\'' | '"')
            })
            .to_string()
        })
}

fn redact_credentials(value: String) -> String {
    let mut output = String::with_capacity(value.len());
    let mut remaining = value.as_str();
    while let Some(scheme) = remaining.find("://") {
        let prefix_end = scheme + 3;
        output.push_str(&remaining[..prefix_end]);
        remaining = &remaining[prefix_end..];
        let authority_end = remaining
            .find(['/', ' ', '\n', '\r', '\t'])
            .unwrap_or(remaining.len());
        let authority = &remaining[..authority_end];
        if let Some(at) = authority.rfind('@') {
            output.push_str(&authority[at + 1..]);
        } else {
            output.push_str(authority);
        }
        remaining = &remaining[authority_end..];
    }
    output.push_str(remaining);
    output
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_string())
}

fn trim_line_end(value: &str) -> &str {
    value.trim_end_matches(['\r', '\n'])
}

fn tail_chars(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_string()
}

fn mac_application_exists(name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        Path::new("/Applications").join(name).is_dir()
            || dirs::home_dir().is_some_and(|home| home.join("Applications").join(name).is_dir())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        false
    }
}

fn open_file_manager(workspace: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new(
        find_executable("xdg-open").ok_or_else(|| "xdg-open is unavailable".to_string())?,
    );
    command
        .arg(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Unable to open workspace: {error}"))?;
    Ok(())
}

fn open_editor(workspace: &Path, command_name: &str, mac_name: &str) -> Result<(), String> {
    if let Some(executable) = find_executable(command_name) {
        std::process::Command::new(executable)
            .arg(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Unable to open editor: {error}"))?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .args(["-a", mac_name, "--"])
            .arg(workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Unable to open {mac_name}: {error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(format!(
            "{mac_name} is not installed or unavailable on PATH"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RepoFileChange, RepoSummary, commit, extract_https_url, git_diff,
        normalized_commit_message, parse_ahead_behind, parse_porcelain_status, redact_credentials,
        repo_summary,
    };
    use std::{path::PathBuf, process::Command};
    use uuid::Uuid;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("zai-workspace-test-{}", Uuid::new_v4()));
            std::fs::create_dir(&path).expect("create isolated test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_porcelain_status_and_rename_records() {
        let value = " M src/lib.rs\0A  new.txt\0?? untracked file\0R  renamed.txt\0old.txt\0";
        let parsed = parse_porcelain_status(value);
        assert_eq!(parsed.staged_count, 2);
        assert_eq!(parsed.unstaged_count, 1);
        assert_eq!(parsed.untracked_count, 1);
        assert_eq!(parsed.changed_files.len(), 4);
        assert_eq!(parsed.changed_files[3].path, "renamed.txt");
    }

    #[test]
    fn parses_upstream_counts_in_behind_ahead_order() {
        assert_eq!(parse_ahead_behind("3\t7\n"), (3, 7));
        assert_eq!(parse_ahead_behind("garbage"), (0, 0));
    }

    #[test]
    fn redacts_http_userinfo_from_command_output() {
        let value = redact_credentials(
            "fatal: https://user:secret@github.com/acme/repo.git failed".to_string(),
        );
        assert_eq!(value, "fatal: https://github.com/acme/repo.git failed");
        assert!(!value.contains("secret"));
    }

    #[test]
    fn extracts_clean_pull_request_url() {
        assert_eq!(
            extract_https_url("Created https://github.com/acme/repo/pull/12\n"),
            Some("https://github.com/acme/repo/pull/12".to_string())
        );
    }

    #[test]
    fn generates_bounded_default_commit_message() {
        let summary = RepoSummary {
            is_repo: true,
            branch: Some("main".to_string()),
            changed_files: vec![RepoFileChange {
                path: "src/main.rs".to_string(),
                status: "M".to_string(),
            }],
            staged_count: 0,
            unstaged_count: 1,
            untracked_count: 0,
            ahead: 0,
            behind: 0,
            has_upstream: true,
            has_remote: true,
            pr_commit_count: Some(0),
            pr_url: None,
        };
        assert_eq!(
            normalized_commit_message(None, &summary).unwrap(),
            "Update src/main.rs"
        );
        assert!(normalized_commit_message(Some("x".repeat(20_000)), &summary).is_err());
    }

    #[tokio::test]
    async fn local_repository_summary_diff_and_commit_round_trip() {
        if which::which("git").is_err() {
            return;
        }
        let directory = TestDirectory::new();
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.email", "zai-tests@example.invalid"],
            vec!["config", "user.name", "zAI tests"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&directory.0)
                    .status()
                    .expect("run git fixture command")
                    .success()
            );
        }
        std::fs::write(directory.0.join("tracked.txt"), "first\n").expect("write fixture");
        assert!(
            Command::new("git")
                .args(["add", "tracked.txt"])
                .current_dir(&directory.0)
                .status()
                .expect("stage fixture")
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "--quiet", "-m", "Initial commit"])
                .current_dir(&directory.0)
                .status()
                .expect("commit fixture")
                .success()
        );
        std::fs::write(directory.0.join("tracked.txt"), "second\n").expect("modify fixture");
        std::fs::write(directory.0.join("untracked.txt"), "new file\n")
            .expect("write untracked fixture");

        let workspace = directory.0.to_string_lossy().into_owned();
        let summary = repo_summary(workspace.clone())
            .await
            .expect("read repository summary");
        assert!(summary.is_repo);
        assert_eq!(summary.unstaged_count, 1);
        assert_eq!(summary.untracked_count, 1);
        assert_eq!(summary.changed_files[0].path, "tracked.txt");
        let diff = git_diff(workspace.clone()).await.expect("read diff");
        assert!(diff.contains("+second"));
        assert!(diff.contains("untracked.txt"));
        assert!(diff.contains("+new file"));

        let result = commit(workspace.clone(), Some("Update fixture".to_string()))
            .await
            .expect("commit through workspace API");
        assert!(!result.message.is_empty());
        let after = repo_summary(workspace)
            .await
            .expect("read clean repository summary");
        assert_eq!(after.staged_count, 0);
        assert_eq!(after.unstaged_count, 0);
        assert_eq!(after.untracked_count, 0);
    }
}
