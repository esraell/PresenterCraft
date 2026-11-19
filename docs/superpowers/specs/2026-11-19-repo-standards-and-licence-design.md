# PresenterCraft: licence, commit standard and contributor setup

Design, 2026-11-19. Status: approved by the owner on 2026-11-19, with four changes I
proposed folded in (AGENTS.md links to the README instead of repeating it; the "How it's
made" note moves to the README rewrite; a pinned Rust version; outdated CI runs cancelled).
The owner chose the workflow, the licence and the naming rule in conversation; the rest are
my calls, marked as such. Not legal advice: a lawyer should read the licence exception
before the first release that includes NDI.

## 1. Intent

The repository went public with no licence, no contributor guide, no templates and no
written rules for commits. Nearly every commit is written by Claude sessions, and outside
people (church volunteers, Rust developers, other people's AI agents) can now open issues
and send pull requests. The owner wants:

- a licence that lets anyone use, change and improve PresenterCraft and send PRs, while
  nobody can turn it into a closed product;
- one standard for what a commit says, followed by the owner, their Claude sessions and
  outside contributors alike;
- issue and PR templates and a contributor guide, so people know what is expected;
- a repository that AI agents can work in well ("agent and Claude friendly");
- whatever else a public open-source repository is missing today.

## 2. Decided by the owner (2026-11-19)

- **Workflow.** Until 1.0, the owner and their Claude sessions commit straight to `master`
  and push. From 1.0, every change goes through a PR. Outsiders always fork and send a PR;
  they cannot push. The owner is the only maintainer and the only one who merges.
- **Licence: GPL-3.0-or-later**, with an additional permission for the closed video
  libraries the app works with (NDI and Blackmagic DeckLink). No contributor licence
  agreement. Outside commits carry a DCO sign-off.
- **NDI and DeckLink are installed by the user,** never shipped with PresenterCraft. The app
  must start and work without NDI, and NDI must never slow the screens (handed to the main
  session as product work, separate from this design).
- **PresenterCraft's code will not be reused inside VoxLive.** VoxLive talks to it as a
  separate program (HTTP API, a web page shown in the app).
- **Commit trailers.** Keep `Co-Authored-By: Claude …`. Drop `Claude-Session:` links; leave
  the commits already pushed as they are.
- **Naming other products.** Never name ProPresenter in app text, identifiers, comments,
  file formats, commits, the README, issues or PRs; describe the behaviour itself. Ordinary
  terms of the trade (Dissolve, Push, slide, playlist) are fine; no adopting another
  product's distinctive names. Exceptions: the README's not-affiliated line, and a plain
  statement of fact for a future importer that uses ProPresenter's public API. Reason:
  Renewed Vision's Terms & Conditions (effective 2025-12-04), which the owner accepted as a
  customer: "Our trademarks and trade dress may not be used in connection with any product
  or service without the prior written consent of the Company." (Already applied:
  commits eb6c8d7, 0331267, 549e836.)

## 3. Goals, non-goals, anti-goals

**Goals**

- GitHub recognises the licence as GPL-3.0 on the repository page.
- Anyone reading the repository can tell, within a minute, how to report a bug, suggest
  something, send a change, and report a security problem privately.
- Every commit, from any author, reads the same way: plain English, what changed for the
  person using PresenterCraft, how it was checked.
- An AI agent opening the repository finds, in one file, how to build, test, run the app
  safely and what it must never do.
- A PR from an outsider is checked automatically before the owner reviews it (for a
  first-time contributor, once the owner approves the run; see 6.8).

**Non-goals for now**

- Release packaging, changelogs and version numbering (before the first release; see
  section 9).
- Discussions, a website, translations of these files.
- Dependency update bots (the graphics crates are pinned exactly on purpose).
- Requiring PRs or passing checks for the owner's own pushes (until 1.0).

**Anti-goals**

- A contributor agreement that would let anyone, the owner included, close the code.
- Licence or template text copied from another project's wording beyond standard texts
  meant to be copied (the GPL, the Contributor Covenant, the DCO).
- Anything that makes outside contributors install a closed SDK or accept a vendor's
  licence just to build PresenterCraft (since 415c6bd the build needs no NDI SDK; CI keeps it
  that way).
- A standard so heavy that the owner's sessions stop following it at 25 commits a day.

## 4. Constraints

- **Another session commits to `master` every few minutes** from the main working tree.
  This work is written in a separate git worktree on branch `repo-standards` and handed to
  that session to cherry-pick, as was done for the naming cleanup. Nothing is written into
  the main working tree directly.
- **GitHub settings change public state.** They are applied with `gh` only after the owner
  approves this design, and each is listed in section 6.8.
- **The builder may push the `repo-standards` branch to GitHub and open a draft PR from it**
  (my call), so CI and the DCO check run before anything reaches `master`. Pushing a branch
  other than `master` does not disturb the main session.
- **The build no longer needs the NDI SDK** (commit 415c6bd): pc-ndi declares the few NDI
  functions it uses itself, following NDI's MIT-licensed headers, and opens the runtime only
  when an NDI screen is turned on. The app starts without NDI. The SDK is needed only for
  pc-ndi's two examples (`--features sdk-examples`). `unsafe` is now allowed in two
  confined places: pc-video's macOS module and pc-ndi's `runtime` module.
- **The picture tests in `pc-render` need a GPU and the macOS system font.** Whether
  GitHub's Mac runners provide a GPU is not known; see section 6.7.
- **Licences of all 535 dependent crates are compatible with GPL-3.0** (checked with
  `cargo metadata`: MIT/Apache variants, 14 Apache-2.0-only, MPL-2.0 symphonia without an
  "incompatible" notice, Unicode, Zlib, BSD, ISC, BSL, CDLA-Permissive-2.0). GPL-2.0 would
  not be, because of the Apache-2.0-only crates.

## 5. Approaches considered

| Decision | Options | Chosen | Why |
|---|---|---|---|
| Workflow | Everything through PRs now / owner pushes, outsiders PR / hybrid | Owner pushes to `master` until 1.0, outsiders PR (owner) | Early-stage speed; outsiders still gated |
| Licence | MIT or Apache-2.0 / GPL-3.0-or-later / MPL-2.0 / AGPL-3.0 | GPL-3.0-or-later (owner) | Nobody can close it; MPL allows closed products; AGPL would bind churches running a changed web remote |
| Contributions | Nothing extra / DCO sign-off / CLA | DCO (owner, on my proposal) | Certifies origin, which matters with the clean-room rule and AI-written code; a CLA would let the owner close the code |
| Commit style | Conventional Commits (`feat:`) / the repo's plain-English style with written rules | Plain English (my call) | Matches every commit so far; reads for people, not changelog tools |
| Issue templates | Free-text Markdown / GitHub issue forms (YAML) | Issue forms (my call) | Required fields and dropdowns help non-technical volunteers and are easy for agents to read |
| Agent instructions | Long CLAUDE.md / AGENTS.md read by every agent, short CLAUDE.md importing it | AGENTS.md + CLAUDE.md (my call) | Outside contributors use Codex, Cursor and others too |
| Where the commit standard lives | Its own file / CONTRIBUTING.md | CONTRIBUTING.md (my call) | One place for people; AGENTS.md links to it |

## 6. The design

### 6.1 Licence files

- **`LICENSE`:** the GPL-3.0 text, verbatim and unchanged (from gnu.org or
  `gh api licenses/gpl-3.0`). Nothing appended, so GitHub detects GPL-3.0.
- **The additional permission**, in its own file at the repository root, named so GitHub's
  licence detection ignores it (`ADDITIONAL-PERMISSION.md` or similar; see section 10).
  Wording, adapted from the review's draft:

  > **PresenterCraft: additional permission for separately installed video libraries**
  > (GNU General Public License version 3, section 7)
  >
  > Copyright (C) 2026 Esrael Nate and the PresenterCraft contributors.
  >
  > 1. "The Listed Components" are: (a) the NDI® runtime library and NDI SDK published by
  >    Vizrt NDI AB; and (b) the Desktop Video drivers and DeckLink SDK published by
  >    Blackmagic Design; in each case including later versions their publishers release.
  > 2. If you modify PresenterCraft, or any covered work, by linking or combining it with any
  >    of the Listed Components, the licensors of PresenterCraft grant you additional
  >    permission to convey the resulting work. Corresponding Source for the combination
  >    need not include the Listed Components.
  > 3. This permission applies only if (a) every copy you convey that is built to use a
  >    Listed Component carries the notices for it in NOTICE.md, and (b) any Listed
  >    Component you convey yourself stays under, and is conveyed in compliance with, its
  >    publisher's terms. Those terms apply to the Listed Components only; neither they nor
  >    this file restrict what the GNU GPL lets you do with PresenterCraft's own code.
  > 4. This permission covers the Listed Components and nothing else. You may remove it from
  >    your copy (GPL section 7). If you modify PresenterCraft, you may extend it to your
  >    changes, but you are not required to.

- **`NOTICE.md`:** copyright line ("Copyright (C) 2026 Esrael Nate and the PresenterCraft
  contributors"), the licence in one paragraph, and the third-party notices:
  - NDI: not part of PresenterCraft, not under the GPL, installed by the user; the
    pass-through terms NDI's agreement asks for (clause 3(d): no modifying or reverse
    engineering the NDI components or their protocols, no circumventing their limits, keep
    their notices, NDI's warranty and liability disclaimers, US export law, NDI's copyright),
    stated as applying to the NDI components only; the MIT notice from NDI's header files,
    which `crates/pc-ndi/src/runtime.rs` follows for its declarations;
    "NDI® is a registered trademark of Vizrt NDI AB" and https://ndi.video/.
  - DeckLink: a placeholder line saying the same model applies, to be filled in from the
    Desktop Video SDK licence when DeckLink work starts.
  - Fonts compiled in through egui's default fonts: Hack (MIT and Bitstream Vera licence),
    Ubuntu Light (Ubuntu Font Licence 1.0), Noto Emoji (OFL 1.1), emoji-icon-font (MIT).
  - `crates/pc-bible/data/*.vrs`: "Copyright (c) 2007-2026 SIL Global", MIT text.
  - Bibles: not under the GPL; each keeps its own terms, listed in `docs/licences/bibles.md`.
  - FFmpeg: a separate program, never linked; when bundled, an LGPL build with its licence
    and a link to its exact source (as `docs/licences/ffmpeg.md` says).
  - **The PresenterCraft name:** the GPL grants no rights to it (GPL section 7(e)); a changed
    version passed on to others must use another name. Plain mentions such as "based on
    PresenterCraft" are fine.
- **`Cargo.toml`:** `license = "…"` in `[workspace.package]` and `license.workspace = true`
  in each of the seven crates (none has a licence field today). The SPDX expression is left
  to the builder (section 10), with `GPL-3.0-or-later` as the safe fallback.
- **No per-file licence headers.**
- **README "Licences" section** rewritten: one line saying PresenterCraft is
  GPL-3.0-or-later with the additional permission, links to `LICENSE`, the permission file
  and `NOTICE.md`; the existing Bible, FFmpeg and NDI paragraphs kept; the not-affiliated
  line stays where it is. "PresenterCraft's own licence is still being chosen" goes.
- **README "How it's made"** (most of the code is written by Claude under the owner's
  direction and review, every such commit says so, and outside AI-assisted work is welcome
  on the terms in CONTRIBUTING.md) is written in the README rewrite that follows this work,
  not here, so the README is not edited twice. This work changes only the README's
  "Licences" section and its sentence about `unsafe` code.
- **`docs/licences/ndi.md` corrections:** add clause 3(d)(viii) (third parties building on
  the product must accept NDI's terms, which the GPL cannot impose, hence: no public
  NDI-enabled binary until NDI agrees in writing); NDI's headers are MIT-licensed and may be
  included (SDK documentation §4.1, §6); the SDK documentation asks for a link to
  https://ndi.video/ wherever NDI is offered, and a trademark line in the About box.

### 6.2 The commit standard (goes in CONTRIBUTING.md)

**Subject line**

- Says what changed, in plain English, as the person running a service would see it; for
  internal work, as a developer would. Starts with a verb: "Add…", "Show…", "Keep…",
  "Fix…", "Let…".
- At most 72 characters. No full stop at the end. No type prefixes (`feat:`, `fix:`). An
  area in front is fine when it helps: "Bible: …", "README: …".
- One change per commit: something that could be reverted on its own. Unrelated changes go
  in separate commits.

**Body**

- After a blank line, prose wrapped at about 100 characters.
- What changed and why, from the operator's side first; then what a developer needs to know
  (a saved-file change, a new setting, compatibility with files saved before).
- A new testing switch is named in its own line: `Testing aid: PRESENTERCRAFT_X …`.
- The last paragraph says how it was checked, and what was not:
  `Checked: cargo test -p pc-model; by hand, dragged a song onto a playlist. Not checked:
  video sound over NDI.` A commit that changes only text files may leave this out.

**Never in a commit** (message, code, tests or files)

- Another presentation product's name, wording, feature names, screenshots or look (the two
  exceptions in section 2).
- Secrets, tokens, local paths (`/Users/…`), links to private things (Claude sessions,
  private documents), personal details of real people.
- Song lyrics or Bible text that isn't public domain; tests use text written for them.

**Trailers**

- `Co-Authored-By: Claude <model> <noreply@anthropic.com>` on every commit an AI helped
  write. Other AI tools use their own equivalent line.
- No `Claude-Session:` lines or other links to private sessions.
- `Signed-off-by: Name <email>` (`git commit -s`) on every commit in an outside PR. It is the
  person's own statement under the DCO 1.1: an AI tool never adds it by itself; the person
  adds it (`git commit -s`, or `git rebase --signoff` before sending).

**Other rules**

- British spelling, as the code and README use (colour, licence, behaviour).
- Never force-push or rewrite `master`. To undo a pushed commit, revert it.
- PRs are squash-merged. The PR title becomes the commit subject (same rules). The body is
  the PR's commit messages, which is where contributors' `Signed-off-by:` and AI trailers
  live, so they reach `master`'s history. The owner tidies that body at merge time when a
  PR's commits are small fix-ups, keeping the sign-offs and trailers. The PR description is
  for review and does not go into history.

### 6.3 CONTRIBUTING.md

Short, in this order:

1. Thanks, and the three ways in: report a bug, suggest an idea or a Bible, send a change.
   Security problems go to SECURITY.md, never an issue.
2. **Before you start:** for anything bigger than a small fix, open an issue first so the
   work isn't wasted.
3. **Building and checking:** the commands from the README (with and without NDI), and the
   three checks (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
   warnings`, `cargo test --workspace`), run with `PRESENTERCRAFT_QUIET=1`.
4. **Sending a change:** fork, branch, PR into `master`; one change per PR; squash-merged.
5. **The commit standard** (section 6.2).
6. **Clean room:** behaviour may be familiar; names, wording, icons and looks of other
   presentation products may not be used, and nothing may come from decompiling another
   product or from schemas extracted from one.
7. **AI-assisted contributions:** welcome. Say in the PR how AI was used. The person
   sending it has read and run it, answers for it, and signs off; claims in the PR ("tested
   on two displays") must be things the person actually did.
8. **Licence of contributions:** GPL-3.0-or-later with the additional permission, the same
   as the project; the sign-off certifies the DCO 1.1 (https://developercertificate.org/).
   No other agreement.

### 6.4 Issue forms (`.github/ISSUE_TEMPLATE/`)

- **`bug.yml`, "Something doesn't work"** (label `bug`). Required: what you did; what happened
  and what you expected; the version (About PresenterCraft, plus the commit from `git log -1
  --oneline` if built from source); the Mac (macOS version; a dropdown for Apple silicon or
  Intel). Optional: the screens set-up (displays, NDI); the log
  (`~/Library/Logs/PresenterCraft/PresenterCraft.log`, with a note that it can contain song
  titles and file names, so read it first); screenshots of PresenterCraft. Checkboxes:
  searched existing issues; this is not a security problem.
- **`idea.yml`, "An idea"** (label `enhancement`). Required: what you want to do in a
  service; why it would help. Optional: how you do it today. A note: describe it in your own
  words; please don't paste screenshots or text from other presentation software.
- **`bible.yml`, "Add a Bible translation"** (label `bible`). Required: name and
  abbreviation; language; where the text is published (a link); its licence or terms.
  Checkbox: its terms let anyone pass it on (public domain or an open licence), as
  `docs/licences/bibles.md` requires.
- **`config.yml`:** blank issues off; a contact link "Report a security problem" to the
  repository's private vulnerability reporting page.

### 6.5 PR template, SECURITY.md, CODE_OF_CONDUCT.md

- **`.github/pull_request_template.md`:** (for review only; it does not become the commit
  message) a comment saying the title becomes the commit subject and each commit follows
  the standard in CONTRIBUTING.md; sections **What changed**, **Why** (issue link), **Checked**
  (commands and what was tried by hand), **Not checked**, **Screenshots** (for anything
  visible); checklist: the three checks pass; README "What works today" / "Not built yet"
  updated if what works changed; no other product's names, wording, screenshots or looks;
  no lyrics or Bible text that isn't public domain; every commit signed off; how AI was
  used, if it was.
- **`SECURITY.md`:** report privately through the Security tab's "Report a vulnerability";
  never in a public issue. What counts: a song, theme, playlist, Bible or media file that
  crashes or hangs the app or runs code; a downloaded Bible accepted without its checksum;
  anything reachable over the network (NDI today, the planned remote-control API); file
  names reaching FFmpeg's command line unsafely; secrets or personal data in logs. Fixes go
  into `master` and the latest release. A reply within a week (the owner agreed).
- **`CODE_OF_CONDUCT.md`:** the Contributor Covenant 2.1, verbatim, with conduct reports
  going to `presentercraft-condu.xa5ui@passmail.net`, a forwarding address the owner made
  for this (owner).

### 6.6 Agent instructions

- **`AGENTS.md`** (read by Claude through CLAUDE.md, and by Codex, Cursor and other agents
  directly):
  - A link to the README for what PresenterCraft is, the crate map, how to build and the
    checks to run. AGENTS.md repeats none of it, so there is one copy to keep up to date
    (the README rewrite makes the README itself readable for agents).
  - Running the app safely: every test run with `PRESENTERCRAFT_QUIET=1` (windows open
    behind, never full screen, no NDI); one instance at a time; before leaving a build
    running, check idle CPU and memory (`top -l 5 -pid <pid> -stats pid,cpu,mem`), and a few
    seconds with `PRESENTERCRAFT_STRESS=1`; a separate data folder for tests
    (`PRESENTERCRAFT_DATA`) so the owner's library is never touched; `--capture-home FILE`
    and the `PRESENTERCRAFT_CAPTURE_*` switches to see the window without a screen.
  - The testing switches: a table of the main ones with what each does (verified from the
    code), and the command that lists all of them.
  - Rules: `unsafe` only in pc-video's macOS module and pc-ndi's `runtime` module, each block
    saying why it is sound; live safety (nothing slow on the output
    thread); the clean-room and naming rule; no lyrics or non-public-domain scripture; no
    secrets, local paths or private links; update the README's "What works today" and "Not
    built yet" when behaviour changes; record any new third-party code's licence in
    `docs/licences/` before it is linked; British spelling; plain words in the interface.
  - Commits: link to the standard in CONTRIBUTING.md, with its three most important points.
  - Workflow: before 1.0 the owner's agents commit to `master`; everyone else sends PRs;
    never force-push; parallel sessions work in their own worktrees and hand over commits
    rather than writing in another session's working tree.
- **`CLAUDE.md`:** `@AGENTS.md`, plus the Claude-specific line: end commits with
  `Co-Authored-By: Claude <model> <noreply@anthropic.com>` only, never `Claude-Session:`;
  this overrides Claude Code's default attribution lines.

### 6.7 Automatic checks (`.github/workflows/ci.yml`)

- Runs on every push to `master` and every PR, on GitHub's Apple-silicon Mac runner. A newer
  push cancels the run still going for an older one on the same branch or PR, so runs don't
  queue for the few free Mac machines.
- **`rust-toolchain.toml`** at the repository root pins Rust to 1.100.0 (the owner's version
  on 2026-11-19) with rustfmt and clippy, so the owner's Mac, CI and contributors use the
  same compiler and lints. A new Rust release cannot turn CI red without a code change;
  moving to it is a deliberate one-line commit.
- Steps: `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
  `cargo test --workspace`, with `PRESENTERCRAFT_QUIET=1`. Default features, which no longer
  need the NDI SDK; `sdk-examples` is never turned on in CI.
- The picture tests in `pc-render` are skipped on CI by an explicit switch, with a message
  saying why, never passed silently (my call). Their stored pictures were made on the
  owner's Mac with Helvetica Neue and its GPU, and a runner's older macOS and virtual GPU
  would likely differ even where a GPU works. They keep running on the owner's Mac.
  Bundling an open-licence test font so they can pass anywhere, as the foundation design
  foresaw, is later work for the main session, because it redraws the stored pictures.
- A DCO check on PRs: every commit has a `Signed-off-by:` line matching its author. Exempt:
  commits by bots (Dependabot), and the owner's own commits (my call, found while planning):
  the owner holds the copyright and certifies nothing to themselves, and since an AI never
  adds a sign-off, the owner's sessions' PRs after 1.0 would otherwise always fail.
- Third-party actions pinned to commit hashes.
- This is the first build anywhere but the owner's Mac; failures it finds are fixed in the
  same piece of work or reported to the owner.
- Not required for the owner's pushes until 1.0; the owner merges PRs only when green.

### 6.8 GitHub settings (with `gh`, after approval)

- A ruleset on `master`: block force pushes and branch deletion. No PR or check requirement
  yet.
- Merging: squash only; squash commit = PR title and the commit messages (GitHub's current
  default, kept); delete branches after merge.
- Private vulnerability reporting: on.
- Secret scanning and push protection: on (my call; free for public repositories, and they
  back the "no secrets" rule).
- Dependabot security updates: stay on (my call). They only propose fixes for known
  vulnerabilities, and cannot move the exactly pinned graphics crates; version-update PRs
  stay off. The DCO check exempts them.
- Approval before running workflows on first-time contributors' PRs: stays as it is (my
  call). CI runs a stranger's code on a Mac runner; the owner approves a newcomer's first
  run, and later PRs from them run by themselves.
- Labels: keep GitHub's defaults; add `bible` and `needs info`.
- Topics: church, worship, presentation-software, lyrics, bible, rust, macos, ndi. Never
  another product's name.

### 6.9 The foundation design spec

`docs/superpowers/specs/2026-11-18-presentercraft-foundation-design.md` stays as a dated
record (my call). One sentence changes: "The owner wants the home window to work much like
it, and it will: …" becomes a sentence saying the home window works the way operators of
presentation software expect, followed by the existing description. The legal guardrail
lines that name ProPresenter stay, because they state the rule.

## 7. How it lands

1. Everything above is written in the worktree on branch `repo-standards`, as small
   commits (licence; contributor files and templates; agent files; CI; README and docs
   edits), each following the new commit standard.
2. Checks before handing over: `cargo fmt --all --check`; `cargo metadata` succeeds with the
   new licence fields; the issue forms and workflow pass a YAML check (actionlint if
   available); `git grep -i propresenter` shows only the allowed lines: the foundation
   design's remaining factual and guardrail lines, the first-build plan's rule line, the
   README's not-affiliated line, and this design itself.
3. The branch is pushed to GitHub and a draft PR is opened from it, with signed-off commits,
   so CI and the DCO check run there first. Fixes to CI or the new files are made on the
   branch. A fix that needs Rust code the main session is changing is handed to it instead,
   or the failing check is reported to the owner.
4. Once the draft PR is green, the commits are handed to the main session to cherry-pick
   and push, as before, and the draft PR is closed unmerged (its commits are already on
   `master`).
5. After they are on GitHub: the settings in 6.8 are applied; the repository page shows
   GPL-3.0; the issue chooser shows the three forms; the first CI run on `master` is watched
   and its result reported.
6. The worktree, the local branch and the pushed branch are removed.

## 8. Owned elsewhere

- **The main session:** NDI loaded at run time and off the output thread (done in 415c6bd);
  later, the About box lines (section 9).
- **In this work, because 415c6bd changed them:** the README's developer section says
  `unsafe` is allowed only in pc-video; it now also covers pc-ndi's `runtime` module, and the
  sentence is updated. AGENTS.md lists `PRESENTERCRAFT_HIDE_NDI` (runs as if NDI were not
  installed) among the testing switches.

## 9. Before the first release (not now)

- The About box: copyright, "no warranty", the licence, the NDI trademark line and link.
- Each download carries `LICENSE`, `NOTICE.md`, a generated third-party licence list
  (cargo-about or similar) and the source (for example a `cargo vendor` tarball attached to
  the release).
- NDI's written agreement before any NDI-enabled public build; until then, releases build
  without NDI.
- The DeckLink SDK licence read and recorded in `docs/licences/decklink.md` before DeckLink
  work starts.
- A lawyer reads the additional permission.
- Distribution is direct download: the GPL rules out the Mac App Store.

## 10. Left to the builder

- Exact wording of every file, within this design.
- The permission file's name, provided GitHub still detects GPL-3.0 (check after pushing).
- The SPDX expression in `Cargo.toml`: `GPL-3.0-or-later WITH AdditionRef-…` if Cargo and
  `cargo metadata` accept it without warnings, else `GPL-3.0-or-later`.
- CI details: toolchain set-up, caching, how the GPU tests detect a missing GPU.
- Label colours; the table of testing switches (verified against the code).

## 11. Open: the owner's to answer

Nothing. The Code of Conduct contact and the security reply time were answered on
2026-11-19 (section 6.5).
