import { describe, expect, it } from "vitest"
import type { RepoSummary } from "./types"
import { deriveWorkspaceGitActions } from "./workspace-actions"

const summary = (values: Partial<RepoSummary> = {}): RepoSummary => ({
  isRepo: true,
  branch: "feature",
  changedFiles: [],
  stagedCount: 0,
  unstagedCount: 0,
  untrackedCount: 0,
  ahead: 1,
  behind: 0,
  hasUpstream: true,
  hasRemote: true,
  prCommitCount: 1,
  prUrl: null,
  ...values,
})

describe("deriveWorkspaceGitActions", () => {
  it("keeps an existing pull request openable with local changes", () => {
    const state = deriveWorkspaceGitActions(summary({
      changedFiles: [{ path: "src/App.tsx", status: "M" }],
      behind: 2,
      prUrl: "https://github.com/acme/repo/pull/7",
    }), false)
    expect(state.createPr.disabled).toBe(false)
    expect(state.createPr.label).toBe("Open PR")
    expect(state.primary).toBe("create-pr")
  })

  it("prefers commit for a dirty worktree", () => {
    const state = deriveWorkspaceGitActions(summary({
      changedFiles: [{ path: "new.txt", status: "?" }],
    }), false)
    expect(state.commit.disabled).toBe(false)
    expect(state.push.disabled).toBe(true)
    expect(state.createPr.disabled).toBe(true)
    expect(state.primary).toBe("commit")
  })

  it("blocks outbound actions while behind upstream", () => {
    const state = deriveWorkspaceGitActions(summary({ behind: 1 }), false)
    expect(state.push.disabled).toBe(true)
    expect(state.createPr.disabled).toBe(true)
  })

  it("blocks an empty pull request", () => {
    const state = deriveWorkspaceGitActions(summary({ ahead: 0, prCommitCount: 0 }), false)
    expect(state.createPr.disabled).toBe(true)
    expect(state.createPr.hint).toContain("No branch commits")
  })

  it("offers push for a branch without an upstream", () => {
    const state = deriveWorkspaceGitActions(summary({ ahead: 0, hasUpstream: false }), false)
    expect(state.push.disabled).toBe(false)
    expect(state.primary).toBe("push")
  })
})
