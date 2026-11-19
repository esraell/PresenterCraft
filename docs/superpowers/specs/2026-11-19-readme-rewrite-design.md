# SoftPresenter: README rewrite

Design, 2026-11-19. Short design (a documentation change), approved by the owner in
conversation on 2026-11-19.

## Intent

The owner wants the README to be understandable for people visiting the repository and for AI
agents: to the point, but in enough depth that an agent understands the project, its use
cases, how to use it from start to finish, what it offers and where it is heading. It goes out
under the project's new name, SoftPresenter, with fresh screenshots.

## Decided by the owner

- The structure below, and moving the detail into `docs/` pages.
- The README says nothing about softProjector and has no website link yet.
- Fresh screenshots, taken by the main session after its rename lands.

## Approaches

| Approach | Trade-off | Chosen |
|---|---|---|
| One long README with everything (as before, 437 lines) | Complete, but the overview drowns | No |
| A short README that leads to fuller pages in `docs/` | Scannable front door; detail one click away | Yes (owner) |
| README plus a website on softpresenter.com | Good later; the owner wants no website yet | Later |

## Design

- **README (about 150 to 200 lines):** name and one line; badges (licence, checks, macOS);
  the main screenshot; the early-days note; why SoftPresenter (its values); who it is for (use
  cases); what it does, one line per area with three or four screenshots and a link to the
  full list; a service from start to finish; getting started; keys, the song text format and
  where things are kept; where it is heading, with a link to `docs/roadmap.md`; for developers
  and AI agents; contributing, security and licence, with the not-affiliated line.
- **`docs/features.md` (new):** the full "What works today" list, moved from the README and kept
  up to date from now on.
- **"Not built yet"** is covered by `docs/roadmap.md` and leaves the README.
- **Pointers:** AGENTS.md, CONTRIBUTING.md and the PR template point at `docs/features.md` and
  `docs/roadmap.md` instead of the README's old sections.
- **Words:** the project's own (no other product named, apart from the not-affiliated line),
  British spelling, plain words.

## Left to the builder

Exact wording; which screenshots go where; badge style.
