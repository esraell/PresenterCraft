# Contributing to SoftPresenter

Thank you for helping. There are three ways in:

- **Something doesn't work?** [Open a bug report](https://github.com/esraell/SoftPresenter/issues/new/choose).
- **An idea, or a Bible translation you'd like offered?** Open an idea or a Bible request from
  the same page.
- **A change you'd like to make?** Read on.

A security problem is never an issue: report it privately, as [SECURITY.md](SECURITY.md) says.
Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

## Before you start

For anything bigger than a small fix, open an issue first and say what you'd like to do. A
short conversation up front saves work that can't be used.

## Building and checking

The [README](README.md#get-started) says how to build and run SoftPresenter; nothing from
NDI is needed to build it. Before you send a change, run the same three checks the automatic
checks run, with test windows kept out of the way:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    SOFTPRESENTER_QUIET=1 cargo test --workspace

`rust-toolchain.toml` pins the Rust version, so `rustup` installs the right one by itself. The
picture tests in `crates/sp-render/tests/golden.rs` compare against pictures made on the
maintainer's Mac, so they may differ on yours; say so in your pull request if they do.

### Signing the Mac app you build

`scripts/macos-app.sh` signs the app it makes. With no signing identity on your Mac it signs
"ad hoc", and macOS treats every new build as a different app, so it asks again whether the app
may read the VoxLive key kept in your Keychain. To make "Always Allow" last from build to build,
give your Mac a code-signing identity of its own, once; the script uses one named
`SoftPresenter Local Signing` (or the name in `SOFTPRESENTER_SIGN_IDENTITY`) when it finds it:

    /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 \
        -keyout key.pem -out cert.pem -subj "/CN=SoftPresenter Local Signing" \
        -addext "basicConstraints=critical,CA:false" \
        -addext "keyUsage=critical,digitalSignature" \
        -addext "extendedKeyUsage=critical,codeSigning"
    /usr/bin/openssl pkcs12 -export -inkey key.pem -in cert.pem \
        -name "SoftPresenter Local Signing" -out local.p12 -passout pass:choose-one
    security import local.p12 -k ~/Library/Keychains/login.keychain-db \
        -P choose-one -T /usr/bin/codesign
    rm key.pem cert.pem local.p12

It signs only what you build on that Mac; it isn't trusted anywhere else and needs no Apple
account. Never commit a key or certificate: `.gitignore` refuses the usual file types. Builds
for others will be signed with a proper Developer ID when there are any.

## Sending a change

1. Fork the repository and make a branch for your change.
2. Keep to one change per pull request. Unrelated fixes go in their own.
3. Open a pull request into `master` and fill in the template.
4. The automatic checks run on every pull request. A first-time contributor's checks start once
   the maintainer approves them.
5. Pull requests are squash-merged: the title becomes the commit's subject line, and your
   commits' messages, with their sign-offs, become its body. The maintainer may tidy that body
   when merging.

## Commits

Every commit in SoftPresenter, from the maintainer, from AI tools and from contributors, is
written the same way.

### The subject line

- Say what changed, in plain English, as the person running a service would see it; for
  internal work, as a developer would. Start with a verb: "Add…", "Show…", "Keep…", "Fix…",
  "Let…".
- At most 72 characters. No full stop at the end. No type prefixes such as `feat:` or `fix:`.
  An area in front is fine when it helps: "Bible: …", "README: …".
- One change per commit: something that could be reverted on its own.

### The body

- After a blank line, write prose wrapped at about 100 characters.
- Say what changed and why, from the operator's side first; then what a developer needs to
  know, such as a change to saved files, a new setting, or how files saved before still open.
- Name a new testing switch on a line of its own: `Testing aid: SOFTPRESENTER_X …`.
- End by saying how it was checked, and what wasn't:
  `Checked: cargo test -p sp-model; by hand, dragged a song onto a playlist. Not checked:
  video sound over NDI.` A commit that changes only text files may leave this out.

An example:

    Show a playlist's songs one after another in the slide area

    A song opened from a playlist is shown with the playlist's other items around it, in the
    playlist's order, each under a bar with its name. Scrolling down goes on into the next
    song. A song opened from a library still shows only its own slides.

    Testing aid: SOFTPRESENTER_OPEN_ITEM opens an item of the playlist in front by its place.

    Checked: cargo test -p sp-app; by hand, scrolled through a three-song playlist.

### Never in a commit

This covers the message, the code, the tests and every file:

- Another presentation product's name, wording, names for its features, screenshots or look.
  Familiar behaviour is fine; describe it in your own words.
- Secrets, tokens, local paths such as `/Users/…`, links to private things, or personal details
  of real people.
- Song lyrics or Bible text that isn't public domain. Tests use text written for them.

### Trailers

- A commit an AI tool helped write ends with that tool's co-author line, for Claude
  `Co-Authored-By: Claude <model> <noreply@anthropic.com>`. Never add links to private AI
  sessions.
- Every commit in a pull request from a contributor carries your sign-off,
  `Signed-off-by: Your Name <your@email>`, added with `git commit -s` (see below). An AI tool
  never adds it for you.

### Other rules

- British spelling, as the code and the README use: colour, licence, behaviour.
- Nobody force-pushes or rewrites `master`. A pushed commit is undone by reverting it.

## Clean room

SoftPresenter is our own work. Behaviour that operators know from other presentation software
is welcome; the names, wording, icons, artwork and looks of other products are not, and nothing
may come from decompiling another product or from file formats extracted from one. If you know
another product well, describe what you need in plain words.

## AI-assisted contributions

Welcome. Say in your pull request how AI was used. You are responsible for what you send: you
have read it, run it, and checked what you say you checked ("tested on two displays" means you
did). The sign-off is yours to give, never the tool's.

## Licence of contributions

SoftPresenter is licensed under the GNU General Public License, version 3 or later, with the
additional permission in [ADDITIONAL-PERMISSION.md](ADDITIONAL-PERMISSION.md). Your contribution
is licensed the same way. There is no other agreement to sign.

Your `Signed-off-by:` line certifies the [Developer Certificate of Origin 1.1](https://developercertificate.org/):
in short, that you wrote the change or otherwise have the right to submit it under the
project's licence. To sign off as you commit, use `git commit -s`. To add a forgotten sign-off
to every commit on your branch:

    git fetch https://github.com/esraell/SoftPresenter.git master
    git rebase --signoff FETCH_HEAD
    git push --force-with-lease

The sign-off has to match each commit's author. If a commit's author isn't you as git is set up
on your computer (a commit made on GitHub's website, for example), make yourself its author and
sign it off in one go:

    git fetch https://github.com/esraell/SoftPresenter.git master
    git rebase --exec 'git commit --amend --no-edit --reset-author --signoff' FETCH_HEAD
    git push --force-with-lease
