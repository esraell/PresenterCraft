# Licence, commit standard and contributor setup: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the GPL-3.0-or-later licence with its additional permission, the commit standard
and contributor files, GitHub templates, agent instructions and CI, then land them on `master`
and apply the GitHub settings.

**Architecture:** Text and configuration files only, written in the worktree on branch
`repo-standards`, checked on a draft PR, handed to the main session to cherry-pick onto
`master`, then GitHub settings applied with `gh`. No Rust source changes.

**Tech Stack:** Markdown, GitHub issue forms (YAML), GitHub Actions (YAML, POSIX shell), Cargo
manifests, `gh`.

**Spec:** `docs/superpowers/specs/2026-11-19-repo-standards-and-licence-design.md`

## Global Constraints

- Work in a separate git worktree on branch `repo-standards`. Never write in the main session's working tree; never push `master`.
- Every commit follows spec 6.2: plain-English subject of at most 72 characters, prose body wrapped at about 100, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; no `Claude-Session:`; no `Signed-off-by:` added by the agent.
- No other presentation product's name anywhere except the lines spec 7.2 allows.
- British spelling. Plain words.
- Licence: GPL-3.0-or-later; copyright line "Copyright (C) 2026 Esrael Nate and the PresenterCraft contributors".
- Conduct contact: `presentercraft-condu.xa5ui@passmail.net`. Security reply: within a week.
- Rust pin: 1.100.0 with rustfmt and clippy.
- Run nothing that starts the app. Don't run `cargo build` or `cargo test` (the main session's builds own the CPU); `cargo metadata` and `cargo fmt --check` are fine.

## Review Focus

- GitHub shows the licence as "Other" or NOASSERTION instead of GPL-3.0 because of the extra permission or notice files: checked through the licence API on the branch (Task 1) and on `master` (Task 7).
- An issue form with a schema mistake is silently dropped from the issue chooser: each form parsed and its required keys checked (Task 3), and the chooser checked after landing (Task 7).
- The DCO check rejects a correct sign-off (different capitalisation, several trailers) or passes a missing one: the check is a script tested locally on made-up commits (Task 5).
- The first run of the checks anywhere but the owner's Mac fails on `master` for everyone to see: it runs on the draft PR first (Task 6).
- The main session's cherry-pick conflicts on README.md or Cargo.toml, which it edits often: the branch is rebased onto the latest `master` just before handover, and the result is checked on `origin/master` (Task 7).

---

### Task 0: Bring the branch up to date

- [ ] **Step 1:** In the worktree, `git fetch origin && git rebase origin/master`. Expected: the design commit replays cleanly onto the new tip.
- [ ] **Step 2:** Commit this plan: `git add docs/superpowers/plans/2026-11-19-repo-standards-and-licence.md` with subject "Add the plan for the licence, commit standard and contributor setup".

### Task 1: Licence files

**Files:**
- Create: `LICENSE`, `ADDITIONAL-PERMISSION.md`, `NOTICE.md`
- Modify: `Cargo.toml` (`[workspace.package]`), each `crates/*/Cargo.toml` (`[package]`), `README.md` ("Licences" section; the `unsafe` sentence under "For developers"), `docs/licences/ndi.md`

- [ ] **Step 1:** Write `LICENSE` from `gh api licenses/gpl-3.0 --jq .body`, unchanged. Check: `head -3 LICENSE` shows "GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007".
- [ ] **Step 2:** Write `ADDITIONAL-PERMISSION.md` with the quoted text of spec 6.1 word for word (heading, copyright line, clauses 1–4).
- [ ] **Step 3:** Write `NOTICE.md` with every item in spec 6.1's NOTICE bullet: copyright and licence paragraph; NDI (pass-through terms limited to NDI's components, the MIT notice copied from the top of `crates/pc-ndi/src/runtime.rs`, trademark line, https://ndi.video/); DeckLink placeholder; egui's fonts (Hack: MIT and Bitstream Vera; Ubuntu Light: Ubuntu Font Licence 1.0; Noto Emoji: OFL 1.1; emoji-icon-font: MIT); `crates/pc-bible/data/*.vrs` with "Copyright (c) 2007-2026 SIL Global" and the MIT text; Bibles not under the GPL (link `docs/licences/bibles.md`); FFmpeg as a separate program; the PresenterCraft name paragraph.
- [ ] **Step 4:** Add `license = "GPL-3.0-or-later WITH AdditionRef-PresenterCraft-additional-permission"` to `[workspace.package]` and `license.workspace = true` to the seven crates. Run `cargo metadata --format-version 1 --offline >/dev/null`. If it prints a warning about the expression, use `license = "GPL-3.0-or-later"` instead and run it again. Expected: no output on stderr.
- [ ] **Step 5:** README: replace "PresenterCraft's own licence is still being chosen and will be added to this repository." with a sentence naming GPL-3.0-or-later and linking `LICENSE`, `ADDITIONAL-PERMISSION.md` and `NOTICE.md`. Keep the Bibles, FFmpeg and NDI paragraphs and the not-affiliated line. Change the `unsafe` sentence to name pc-video's macOS module and pc-ndi's `runtime` module.
- [ ] **Step 6:** `docs/licences/ndi.md`: add clause 3(d)(viii) and its consequence (no public NDI-enabled binary until NDI agrees in writing), the MIT status of the headers (SDK documentation §4.1, §6), and the ndi.video link and About-box trademark line the SDK documentation asks for. Keep what 415c6bd wrote.
- [ ] **Step 7:** Commit (subject: "Licence PresenterCraft under GPL-3.0-or-later, with a permission for NDI and DeckLink").

### Task 2: Contributor files

**Files:** Create `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`.

- [ ] **Step 1:** `CONTRIBUTING.md`: the eight parts of spec 6.3 in that order, with spec 6.2 as its "Commits" section (all four groups: subject, body, never, trailers, plus the other rules). Add the one-line fix for a missing sign-off: `git rebase --signoff origin/master`, then force-push the PR branch.
- [ ] **Step 2:** `SECURITY.md` per spec 6.5.
- [ ] **Step 3:** `CODE_OF_CONDUCT.md`: the Contributor Covenant 2.1 text, verbatim, from https://raw.githubusercontent.com/EthicalSource/contributor_covenant/release/content/version/2/1/code_of_conduct.md (if that path moved, find the 2.1 Markdown in that repository). Only `[INSERT CONTACT METHOD]` changes, to the conduct address. Check: `grep -c 'INSERT' CODE_OF_CONDUCT.md` prints 0, and the file says "version 2.1".
- [ ] **Step 4:** Commit (subject: "Add the contributor guide, commit standard, security policy and code of conduct").

### Task 3: Issue forms and PR template

**Files:** Create `.github/ISSUE_TEMPLATE/bug.yml`, `idea.yml`, `bible.yml`, `config.yml`, `.github/pull_request_template.md`.

- [ ] **Step 1:** Write the three forms and `config.yml` per spec 6.4. Each form has top-level `name`, `description`, `labels`, `body`; every `body` item has a `type` (`markdown`, `textarea`, `input`, `dropdown` or `checkboxes`), and every non-markdown item has a unique `id` and `attributes.label`; required fields use `validations: required: true`. The security link in `config.yml` is `https://github.com/esraell/PresenterCraft/security/advisories/new`.
- [ ] **Step 2:** Check: `yamllint -d relaxed .github/ISSUE_TEMPLATE/` reports no errors, and `python3 -I -c` loading each file with `yaml.safe_load` asserts the keys above.
- [ ] **Step 3:** Write the PR template per spec 6.5.
- [ ] **Step 4:** Commit (subject: "Add issue forms for bugs, ideas and Bibles, and a pull request template").

### Task 4: Agent instructions and the foundation design sentence

**Files:** Create `AGENTS.md`, `CLAUDE.md`. Modify `docs/superpowers/specs/2026-11-18-presentercraft-foundation-design.md` (the "The owner wants the home window to work much like it, and it will:" sentence).

- [ ] **Step 1:** Build the testing-switch table: list the switches with `git grep -ohE 'PRESENTERCRAFT_[A-Z_]+' crates | sort -u`, and read each one's use in the code for a one-line description. Include `PRESENTERCRAFT_QUIET`, `PRESENTERCRAFT_DATA`, `PRESENTERCRAFT_STRESS`, `PRESENTERCRAFT_HIDE_NDI`, the `PRESENTERCRAFT_CAPTURE_*` group and `--capture-home`; state the listing command for the rest.
- [ ] **Step 2:** Write `AGENTS.md` per spec 6.6: a link to the README for the project, crates, build and checks (no copy of them); running the app safely; the switch table; the rules; commits (link to CONTRIBUTING.md); workflow. Add the rule that a new test needing a GPU belongs in `crates/pc-render/tests/golden.rs` or must be added to CI's skip list in `.github/workflows/ci.yml`.
- [ ] **Step 3:** Write `CLAUDE.md`: `@AGENTS.md` and the commit-trailer line of spec 6.6.
- [ ] **Step 4:** Reword the foundation design sentence per spec 6.9.
- [ ] **Step 5:** Check: `git grep -n -i propresenter` lists only the lines spec 7.2 allows.
- [ ] **Step 6:** Commit (subject: "Add AGENTS.md and CLAUDE.md: how agents build, test and run PresenterCraft safely").

### Task 5: Rust pin and CI

**Files:** Create `rust-toolchain.toml`, `.github/workflows/ci.yml`, `.github/scripts/dco-check.sh`.

**Interfaces:**
- Produces: `.github/scripts/dco-check.sh <base-sha> <head-sha>`: exits 0 when every non-merge commit in `base..head` has a `Signed-off-by: <author name> <author email>` trailer (name and email compared exactly as git records them, email case-insensitively), skipping commits whose author email ends in `[bot]@users.noreply.github.com` and commits by the owner (`105669618+esraell@users.noreply.github.com`); otherwise prints each failing commit's short hash and subject with the fix (`git rebase --signoff`) and exits 1.

- [ ] **Step 1:** `rust-toolchain.toml`: `[toolchain]` with `channel = "1.100.0"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`.
- [ ] **Step 2:** Write the test first: a throwaway repo in the scratchpad with five commits (one signed off correctly; one without; one with the email in other capitals; one by `dependabot[bot]`; one by the owner's address without sign-off). Expected results when the script is pointed at each single-commit range: pass, fail, pass, pass (skipped), pass (skipped).
- [ ] **Step 3:** Run the test against a stub script that exits 1. Expected: the "pass" cases fail.
- [ ] **Step 4:** Write `dco-check.sh` (POSIX `sh`, `git log --format` with trailers, no extra tools). Run the test again. Expected: all five as above.
- [ ] **Step 5:** Write `ci.yml`:
  - `on: push: branches: [master]` and `pull_request`.
  - `concurrency: group: ci-${{ github.event.pull_request.number || github.ref }}`, `cancel-in-progress: true`.
  - `permissions: contents: read`.
  - Job `checks` on `macos-latest`: checkout; Swatinem/rust-cache; `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace --exclude pc-render -- --skip stage::tests::stage_picture`; `cargo test -p pc-render --lib`; then an `echo "::notice::…"` step saying the picture tests (`crates/pc-render/tests/golden.rs` and `stage::tests::stage_picture`) were skipped because their stored pictures come from the owner's Mac, GPU and font. `env: PRESENTERCRAFT_QUIET: "1"`.
  - Job `dco`, `if: github.event_name == 'pull_request'`: checkout with `fetch-depth: 0`, then `sh .github/scripts/dco-check.sh ${{ github.event.pull_request.base.sha }} ${{ github.event.pull_request.head.sha }}`.
  - Every third-party action pinned to a full commit hash, with its version in a comment. Get each hash with `gh api repos/<owner>/<action>/commits/<tag> --jq .sha`.
- [ ] **Step 6:** Check: `yamllint -d relaxed .github/workflows/ci.yml` reports no errors; `sh -n .github/scripts/dco-check.sh` is clean.
- [ ] **Step 7:** Commit (subject: "Check every push and pull request on a Mac, and pin Rust to 1.100.0").

### Task 6: Check it on a draft PR

- [ ] **Step 1:** `git push -u origin repo-standards`.
- [ ] **Step 2:** `gh pr create --draft --base master --head repo-standards` with title "Licence, contributor guide, templates, agent instructions and CI" and a body saying it is opened to run the checks and will be closed unmerged once the commits are on `master`.
- [ ] **Step 3:** `gh api "repos/esraell/PresenterCraft/license?ref=repo-standards" --jq .license.spdx_id`. Expected: `GPL-3.0`. If not, rename the permission file (spec section 10) and check again.
- [ ] **Step 4:** `gh pr checks --watch`. Expected: `checks` and `dco` pass. On a failure: fix files on this branch and push; if the fix needs Rust code, report it to the owner and hand it to the main session, without editing Rust here.

### Task 7: Land, configure, verify, clean up

- [ ] **Step 1:** `git fetch origin && git rebase origin/master`, push the branch again (`--force-with-lease`, this branch only), wait for the checks to pass.
- [ ] **Step 2:** Send the main session the commit hashes, oldest first, to `git cherry-pick` and push.
- [ ] **Step 3:** After it reports them pushed: `git cherry origin/master repo-standards` lists every commit with `-`; `gh api repos/esraell/PresenterCraft/license --jq .license.spdx_id` prints `GPL-3.0`; `gh api repos/esraell/PresenterCraft/community/profile` shows the licence, code of conduct, contributing guide, issue template and PR template.
- [ ] **Step 4:** Apply spec 6.8 with `gh`:
  - The ruleset: `POST repos/esraell/PresenterCraft/rulesets`, with `target: branch`, `enforcement: active`, `conditions.ref_name.include: ["~DEFAULT_BRANCH"]` and rules `deletion` and `non_fast_forward`.
  - Merging: `PATCH repos/esraell/PresenterCraft` with `allow_merge_commit=false`, `allow_rebase_merge=false`, `allow_squash_merge=true`, `squash_merge_commit_title=PR_TITLE`, `squash_merge_commit_message=COMMIT_MESSAGES`, `delete_branch_on_merge=true`.
  - Private vulnerability reporting: `PUT repos/esraell/PresenterCraft/private-vulnerability-reporting`.
  - Secret scanning and push protection: `security_and_analysis` on through the same PATCH.
  - Labels: `gh label create bible` and `gh label create "needs info"`.
  - Topics: `gh repo edit --add-topic church,worship,presentation-software,lyrics,bible,rust,macos,ndi`.

  Read each setting back.
- [ ] **Step 5:** Watch the first `master` run (`gh run list --branch master --limit 1`, then `gh run watch`), and report its result.
- [ ] **Step 6:** Close the draft PR unmerged with a comment naming the `master` commits; `git push origin --delete repo-standards`; `git worktree remove` the worktree; `git branch -D repo-standards`.
- [ ] **Step 7:** Update the memory file `presentercraft-repo-standards-decisions.md`: the work is done, plus anything still open.
