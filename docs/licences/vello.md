# Vello: the changed copy in `third_party/vello`

SoftPresenter draws every slide with Vello, Linebender's GPU renderer
(<https://github.com/linebender/vello>). Most of Vello comes from crates.io like any other
library. One of its crates, `vello` 0.12.0, is kept in this repository with changes, in
[`third_party/vello`](../../third_party/vello), and built in place of the published one
(`[patch.crates-io]` in the workspace's `Cargo.toml`).

## The licence

Vello is copyright the Vello Authors and licensed under Apache-2.0 OR MIT, the user's choice.
Both can be combined with the GNU GPL version 3. The copy keeps both licence files
(`LICENSE-APACHE`, `LICENSE-MIT`), every file keeps its copyright line, and each change is
marked in the source, as the Apache licence asks of changed files. SoftPresenter's changes to
that folder are offered under the same two licences, so they can go back to Vello.

It ships as part of the app like the rest of the code, so [NOTICE.md](../../NOTICE.md) names it.

## What was changed, and why

[`third_party/vello/PROVENANCE.md`](../../third_party/vello/PROVENANCE.md) has the exact
source and the list of changes. In short: Vello 0.12 keeps about 165 MB of GPU working memory
for every renderer whatever is drawn, and silently draws nothing when a picture needs more.
The copy sizes that memory from what is drawn, checks every picture, draws a picture again
with more room when it needs it, and gives the memory back afterwards.

`vello_encoding` and `vello_shaders`, which `vello` is built on, are the published crates,
unchanged.
