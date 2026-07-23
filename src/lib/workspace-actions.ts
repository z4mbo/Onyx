import type { RepoSummary } from "./types"

export type GitActionName = "commit" | "push" | "create-pr"

export function deriveWorkspaceGitActions(summary: RepoSummary | null, loading: boolean) {
  const isRepo = summary?.isRepo ?? false
  const hasChanges = (summary?.changedFiles.length ?? 0) > 0
  const hasExistingPr = !!summary?.prUrl
  const behind = summary?.behind ?? 0
  const ahead = summary?.ahead ?? 0

  const commit = {
    disabled: loading || !isRepo || !hasChanges,
    hint: hasChanges ? "Stage and commit all workspace changes" : "The working tree is clean",
  }
  const push = {
    disabled:
      loading ||
      !isRepo ||
      !summary?.hasRemote ||
      hasChanges ||
      behind > 0 ||
      (ahead === 0 && summary?.hasUpstream !== false),
    hint: hasChanges
      ? "Commit local changes before pushing"
      : behind > 0
        ? "Pull or rebase before pushing"
        : "Push the current branch",
  }
  const createPr = {
    disabled:
      loading ||
      !isRepo ||
      (!hasExistingPr && (
        !summary?.hasRemote ||
        !summary?.branch ||
        hasChanges ||
        behind > 0 ||
        summary?.prCommitCount === 0
      )),
    label: hasExistingPr ? "Open PR" : "Create PR",
    hint: hasExistingPr
      ? "Open the existing pull request"
      : hasChanges
        ? "Commit local changes before creating a PR"
        : behind > 0
          ? "Pull or rebase before creating a PR"
          : summary?.prCommitCount === 0
            ? "No branch commits to include in a pull request"
            : "Create a pull request",
  }
  const primary: GitActionName = hasExistingPr
    ? "create-pr"
    : hasChanges
      ? "commit"
      : ahead > 0 || (isRepo && summary?.hasUpstream === false)
        ? "push"
        : "create-pr"

  return { commit, push, createPr, primary }
}
