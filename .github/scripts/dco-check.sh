#!/bin/sh
# Checks that every commit in a pull request carries its author's sign-off
# (Developer Certificate of Origin 1.1; see CONTRIBUTING.md).
#
# Usage: dco-check.sh <base commit> <head commit>
#
# Each non-merge commit in base..head needs a "Signed-off-by: <name> <email>" trailer with the
# author's name exactly as git records it and the author's email in any capitals. Skipped:
# Dependabot and GitHub Actions, by their exact addresses, and the owner's own commits (the
# owner holds the copyright and certifies nothing to themselves). Every other author, other
# bots and AI apps included, needs a person's sign-off.
set -eu

base=$1
head=$2
owner=105669618+esraell@users.noreply.github.com
dependabot=49699333+dependabot[bot]@users.noreply.github.com
actions=41898282+github-actions[bot]@users.noreply.github.com

lower() {
    printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

if ! commits=$(git rev-list --no-merges "$base..$head"); then
    printf '%s\n' "Couldn't read the commits between $base and $head, so nothing was checked."
    exit 1
fi

failed=0
for commit in $commits; do
    name=$(git log -1 --format=%an "$commit")
    email=$(lower "$(git log -1 --format=%ae "$commit")")
    case "$email" in
        "$owner" | "$dependabot" | "$actions") continue ;;
    esac

    signed=0
    others=""
    signoffs=$(git log -1 --format='%(trailers:key=Signed-off-by,valueonly,unfold)' "$commit")
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        signer=${line%% <*}
        signer_email=${line##*<}
        signer_email=$(lower "${signer_email%%>*}")
        if [ "$signer" = "$name" ] && [ "$signer_email" = "$email" ]; then
            signed=1
        else
            others="${others:+$others, }$line"
        fi
    done <<EOF
$signoffs
EOF

    if [ "$signed" -eq 0 ]; then
        what=$(git log -1 --format='%h "%s"' "$commit")
        if [ -n "$others" ]; then
            printf '%s\n' "$what is signed off as $others, but its author is $name <$email>."
        else
            printf '%s\n' "$what has no sign-off from its author, $name <$email>."
        fi
        failed=1
    fi
done

if [ "$failed" -ne 0 ]; then
    cat <<'EOF'

Each commit needs a sign-off from its author (see CONTRIBUTING.md). To sign off every commit on
your branch as yourself:

    git fetch https://github.com/esraell/PresenterCraft.git master
    git rebase --signoff FETCH_HEAD
    git push --force-with-lease

If a commit's author isn't you as git is set up on this computer (a commit made on GitHub's
website, for example), make yourself its author and sign it off instead:

    git fetch https://github.com/esraell/PresenterCraft.git master
    git rebase --exec 'git commit --amend --no-edit --reset-author --signoff' FETCH_HEAD
    git push --force-with-lease
EOF
    exit 1
fi
printf '%s\n' "Every commit is signed off."
