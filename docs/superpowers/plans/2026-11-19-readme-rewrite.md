# README rewrite: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A short, clear README for SoftPresenter, with the full feature list in `docs/features.md`.

**Architecture:** Markdown only, written in a separate worktree on branch `readme-rewrite`, handed
to the main session to cherry-pick, like the standards work.

**Tech Stack:** Markdown, GitHub rendering, shields.io badges.

**Spec:** `docs/superpowers/specs/2026-11-19-readme-rewrite-design.md`

## Global Constraints

- The name is SoftPresenter. No mention of softProjector, no website link.
- No other presentation product named, except the not-affiliated line.
- British spelling, plain words. Commits follow CONTRIBUTING.md#commits.
- Never write in the main session's working tree; it is renaming the code meanwhile and leaves
  README.md to this work.

## Review Focus

- A feature in the old "What works today" goes missing in the move: every bullet is checked
  against `docs/features.md` (Task 1).
- A link or image breaks on GitHub (relative paths, renamed files, the renamed repository):
  every link is checked after rebasing onto the rename (Task 4).
- The README states something the app doesn't do, or calls a feature by a name the app doesn't
  use: each claim is checked against `docs/features.md` and `docs/app-map.md` (Task 2).
- The start-to-finish walkthrough asks the reader for a step that doesn't exist (a menu, a key,
  a file location): each step is checked against `docs/app-map.md` and the code (Task 2).
- Switch names, paths and commands still use the old name after the rename: checked with
  `git grep -i presentercraft` on the branch (Task 4).

---

### Task 1: docs/features.md
- [ ] Move the README's "What works today" into `docs/features.md`, grouped as now, under the
      new name, with an introduction linking back to the README and to `docs/roadmap.md`.
- [ ] Check: every bullet heading of the old section appears in the new page.
- [ ] Commit.

### Task 2: README.md
- [ ] Write the README in the order the spec gives, at most about 220 lines.
- [ ] Check each claim and each walkthrough step against `docs/features.md`,
      `docs/app-map.md` and `docs/cli.md`.
- [ ] Commit.

### Task 3: pointers
- [ ] After rebasing onto the rename: AGENTS.md, CONTRIBUTING.md and the PR template point at
      `docs/features.md` and `docs/roadmap.md` instead of the README's old sections.
- [ ] Commit.

### Task 4: screenshots, links, review, handover
- [ ] Rebase onto the latest master once the rename and the screenshots are in; place the
      screenshots; carry over any features added to the old README section meanwhile.
- [ ] Check every relative link and image path exists; `git grep -i presentercraft` lists only
      dated history docs.
- [ ] A fresh reviewer reads the README as a newcomer and as an agent.
- [ ] Hand the commits to the main session; check the README on GitHub after the push.
