#!/usr/bin/env bash
# Builds the test bench from docs/TESTING.md: throwaway repositories with branches in every state
# Musubiko knows about, plus odd repositories and a broken server.
#
#   scripts/sandbox.sh [DEST]      default: /tmp/musubiko-sandbox; rebuilt from scratch every time
#
# DEST must be outside any repository, or git finds that one from not-a-repo.
#
# Each world has a bare origin.git standing in for the server. DEST/expected.tsv lists what the
# app should say about the key branches; DEST/README.md lists the repositories.
set -euo pipefail

DEST=${1:-/tmp/musubiko-sandbox}
MARK=.musubiko-sandbox

# The same git the app would choose: the newest one, at least 2.38.
pick_git() {
  local best="" best_v=0 bin v
  for bin in ${MUSUBIKO_GIT:-} $(which -a git 2>/dev/null) /opt/homebrew/bin/git /usr/local/bin/git /usr/bin/git; do
    [ -x "$bin" ] || continue
    v=$("$bin" --version 2>/dev/null | awk '{split($3, p, "."); printf "%d%03d%03d", p[1], p[2], p[3]}')
    [ -n "$v" ] && [ "$v" -gt "$best_v" ] && best=$bin best_v=$v
  done
  if [ "$best_v" -lt 2038000 ]; then
    echo "sandbox: need git 2.38 or newer" >&2
    exit 1
  fi
  echo "$best"
}
GIT=$(pick_git)

if [ -e "$DEST" ]; then
  if [ ! -e "$DEST/$MARK" ] && [ -n "$(ls -A "$DEST")" ]; then
    echo "sandbox: $DEST is not empty and was not made by this script" >&2
    exit 1
  fi
  rm -rf "$DEST"
fi
mkdir -p "$DEST"
touch "$DEST/$MARK"
DEST=$(cd "$DEST" && pwd -P)
if "$GIT" -C "$DEST" rev-parse --git-dir >/dev/null 2>&1; then
  echo "sandbox: $DEST is inside a git repository; choose a folder outside it" >&2
  rm -rf "$DEST"
  exit 1
fi

# Isolated from the user's config: no global hooks, aliases or autocrlf.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_TERMINAL_PROMPT=0 GIT_EDITOR=true GIT_MERGE_AUTOEDIT=no
unset GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL

NOW=$(date +%s)
DAY=86400
# Commits start three days ago, a minute apart: recent, but in a stable order.
T=$((NOW - 3 * DAY))

stamp() { export GIT_AUTHOR_DATE="$1 +0300" GIT_COMMITTER_DATE="$1 +0300"; }
tick() { T=$((T + 60)); stamp "$T"; }
at() { local d=$1; shift; "$GIT" -C "$d" "$@"; }

# put DIR FILE CONTENT — writes the file, making directories.
put() { mkdir -p "$(dirname "$1/$2")"; printf '%s' "$3" >"$1/$2"; }

# commit DIR MESSAGE [FILE CONTENT]... — writes the files and commits them.
commit() {
  local d=$1 msg=$2; shift 2
  while [ $# -gt 0 ]; do put "$d" "$1" "$2"; at "$d" add -- "$1"; shift 2; done
  tick
  at "$d" commit -q -m "$msg"
}

# commit_old DAYS DIR MESSAGE FILE CONTENT — a commit made DAYS ago.
commit_old() {
  local days=$1; shift
  put "$1" "$3" "$4"; at "$1" add -- "$3"
  stamp $((NOW - days * DAY))
  at "$1" commit -q -m "$2"
  tick
}

merge() { local d=$1; shift; tick; at "$d" merge -q "$@"; }
pick() { local d=$1; shift; tick; at "$d" cherry-pick -x "$@" >/dev/null; }
sw() { at "$1" checkout -q "${@:2}"; }

me() { at "$1" config user.name "Me"; at "$1" config user.email "me@example.com"; at "$1" config core.autocrlf false; }
colleague() { at "$1" config user.name "Коллега Петров"; at "$1" config user.email "petrov@example.com"; at "$1" config core.autocrlf false; }

# A repository with a bare origin next to it: world DIR [BRANCH]
world() {
  mkdir -p "$1"
  "$GIT" init -q --bare -b "${2:-master}" "$1/origin.git"
  "$GIT" init -q -b "${2:-master}" "$1/me"
  me "$1/me"
  at "$1/me" remote add origin "$1/origin.git"
}

EXPECTED=$DEST/expected.tsv
printf 'repo\tbranch\ttarget\tstatus\tdetails\n' >"$EXPECTED"
# expect REPO BRANCH TARGET STATUS [DETAILS]
expect() { printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "${5:-}" >>"$EXPECTED"; }

lines() { seq "$1" "$2" | sed "s/^/${3:-line} /"; }

# ---------------------------------------------------------------------------------------------
# flow: master + develop, a branch of every kind, hotfixes in every radar state, a colleague.
# ---------------------------------------------------------------------------------------------
W=$DEST/flow
world "$W"
ME=$W/me

commit "$ME" "Initial commit" \
  README.md $'# Shop\n' \
  app/c.txt $'one\ntwo\nthree\n' \
  app/c2.txt $'alpha\nbeta\ngamma\n' \
  app/d.txt $'d1\nd2\nd3\n' \
  app/gone.txt $'will be deleted on develop\n' \
  "docs/План работ.md" $'черновик\n' \
  app/ws.js $'function f() {\n  return 1;\n}\n' \
  app/old-name.txt "$(lines 1 20 text)"$'\n' \
  app/long.txt "$(lines 1 300)"$'\n' \
  app/noeol.txt $'no newline at the end' \
  app/to-delete.txt $'bye\n' \
  scripts/run.sh $'#!/bin/sh\necho run\n'
mkdir -p "$ME/assets"; printf '\x89PNG\r\n\x1a\n' >"$ME/assets/logo.png"
seq 1 200 | tr '\n' '\0' >>"$ME/assets/logo.png"
printf 'a\r\nb\r\nc\r\n' >"$ME/app/crlf.txt"
at "$ME" add assets/logo.png app/crlf.txt
tick; at "$ME" commit -q -m "Add logo and a CRLF file"
at "$ME" push -q -u origin master
at "$ME" branch develop
at "$ME" push -q -u origin develop
sw "$ME" develop
commit "$ME" "Develop: first feature" app/dev1.txt $'dev 1\n'

# Merged with a merge commit.
sw "$ME" -b feature/merged
commit "$ME" "Merged work" app/m.txt $'m\n'
sw "$ME" develop; merge "$ME" --no-ff -m "Merge feature/merged" feature/merged
at "$ME" push -q -u origin feature/merged
expect flow feature/merged develop merged

# Merged by fast-forward: the tip is in develop itself.
sw "$ME" -b feature/merged-ff
commit "$ME" "Fast-forward work" app/ff.txt $'ff\n'
sw "$ME" develop; merge "$ME" --ff-only feature/merged-ff
at "$ME" push -q -u origin feature/merged-ff
expect flow feature/merged-ff develop merged "ahead 0"

# Squash-merged: other commits, same content.
sw "$ME" -b feature/squashed
commit "$ME" "Squash 1" app/s.txt $'s1\n'
commit "$ME" "Squash 2" app/s.txt $'s2\n'
sw "$ME" develop; tick; at "$ME" merge -q --squash feature/squashed >/dev/null; at "$ME" commit -q -m "Squashed feature/squashed"
at "$ME" push -q -u origin feature/squashed
expect flow feature/squashed develop squashed

# Rebased onto develop: the same patches under new hashes.
sw "$ME" -b feature/rebased
commit "$ME" "Rebase 1" app/r1.txt $'r1\n'
commit "$ME" "Rebase 2" app/r2.txt $'r2\n'
sw "$ME" develop
tick; at "$ME" cherry-pick feature/rebased~1 >/dev/null
tick; at "$ME" cherry-pick feature/rebased >/dev/null
at "$ME" push -q -u origin feature/rebased
expect flow feature/rebased develop squashed "rebase: same patches, new hashes"

# Merged once, then more work.
sw "$ME" -b feature/partly-merged
commit "$ME" "Partly 1" app/p.txt $'p1\n'
sw "$ME" develop; merge "$ME" --no-ff -m "Merge feature/partly-merged" feature/partly-merged
sw "$ME" feature/partly-merged
commit "$ME" "Partly 2" app/p.txt $'p2\n'
at "$ME" push -q -u origin feature/partly-merged
expect flow feature/partly-merged develop open "ahead 1"

# Conflicts: branches cut before develop changes the same lines.
sw "$ME" develop
sw "$ME" -b feature/conflict
commit "$ME" "Change c on the branch" app/c.txt $'one\ntwo (feature)\nthree\n'
at "$ME" push -q -u origin feature/conflict
sw "$ME" develop
sw "$ME" -b feature/conflict-multi
commit "$ME" "Change d, the plan and gone.txt" \
  app/d.txt $'d1\nd2 (feature)\nd3\n' \
  "docs/План работ.md" $'черновик (ветка)\n' \
  app/gone.txt $'edited on the branch\n'
at "$ME" push -q -u origin feature/conflict-multi
sw "$ME" develop
commit "$ME" "Change c on develop" app/c.txt $'one\ntwo (develop)\nthree\n'
commit "$ME" "Change d and the plan on develop" app/d.txt $'d1\nd2 (develop)\nd3\n' "docs/План работ.md" $'черновик (develop)\n'
tick; at "$ME" rm -q app/gone.txt; at "$ME" commit -q -m "Delete gone.txt on develop"
expect flow feature/conflict develop conflict "app/c.txt"
expect flow feature/conflict-multi develop conflict "app/d.txt, docs/План работ.md, app/gone.txt (modify/delete)"

sw "$ME" -b feature/open
commit "$ME" "Open work" app/o.txt $'o\n'
at "$ME" push -q -u origin feature/open
expect flow feature/open develop open

# Remote tracking: unpushed, behind, diverged, never pushed.
sw "$ME" develop; sw "$ME" -b feature/unpushed
commit "$ME" "Pushed" app/u.txt $'u1\n'
at "$ME" push -q -u origin feature/unpushed
commit "$ME" "Not pushed 1" app/u.txt $'u2\n'
commit "$ME" "Not pushed 2" app/u.txt $'u3\n'
expect flow feature/unpushed develop open "sync ahead 2"

sw "$ME" develop; sw "$ME" -b feature/behind-remote
commit "$ME" "Behind 1" app/b.txt $'b1\n'
commit "$ME" "Behind 2" app/b.txt $'b2\n'
at "$ME" push -q -u origin feature/behind-remote
at "$ME" reset -q --hard HEAD~1
expect flow feature/behind-remote develop open "sync behind 1"

sw "$ME" develop; sw "$ME" -b feature/diverged
commit "$ME" "Common" app/dv.txt $'common\n'
commit "$ME" "Remote side" app/dv.txt $'remote\n'
at "$ME" push -q -u origin feature/diverged
at "$ME" reset -q --hard HEAD~1
commit "$ME" "Local side" app/dv-local.txt $'local\n'
expect flow feature/diverged develop open "sync ahead 1 behind 1"

sw "$ME" develop; sw "$ME" -b feature/never-pushed
commit "$ME" "Never pushed" app/np.txt $'np\n'
expect flow feature/never-pushed develop open "local only, no upstream"

# Deleted on the server: after the merge, and with unmerged work.
sw "$ME" develop; sw "$ME" -b feature/gone-merged
commit "$ME" "Gone after merge" app/gm.txt $'gm\n'
at "$ME" push -q -u origin feature/gone-merged
sw "$ME" develop; merge "$ME" --no-ff -m "Merge feature/gone-merged" feature/gone-merged
at "$ME" push -q origin --delete feature/gone-merged
expect flow feature/gone-merged develop merged "upstream gone"

sw "$ME" develop; sw "$ME" -b feature/gone-unmerged
commit "$ME" "Gone with my work" app/gu.txt $'gu\n'
at "$ME" push -q -u origin feature/gone-unmerged
at "$ME" push -q origin --delete feature/gone-unmerged
expect flow feature/gone-unmerged develop open "upstream gone, own commits: attention"

# No commits for 40 days: "old" in the list, "quiet" in my branches.
sw "$ME" develop; sw "$ME" -b feature/stale
commit_old 40 "$ME" "Old work" app/stale.txt $'old\n'
at "$ME" push -q -u origin feature/stale
expect flow feature/stale develop open "40 days without commits"

# Cut now; develop runs 25 commits ahead below.
sw "$ME" develop; sw "$ME" -b feature/base-ran-away
commit "$ME" "Before develop ran away" app/rw.txt $'rw\n'
at "$ME" push -q -u origin feature/base-ran-away
expect flow feature/base-ran-away develop open "behind > 20: attention"

# Only whitespace changes: empty with -w.
sw "$ME" develop; sw "$ME" -b feature/whitespace
commit "$ME" "Tabs instead of spaces" app/ws.js $'function f() {\n\treturn 1;  \n}\n'
at "$ME" push -q -u origin feature/whitespace
expect flow feature/whitespace develop open "diff is empty with -w"

# Every kind of file change the diff has to draw.
sw "$ME" develop; sw "$ME" -b feature/diff-zoo
at "$ME" mv app/old-name.txt app/new-name.txt
put "$ME" app/new-name.txt "$(lines 1 20 text | sed 's/^text 7$/text seven/')"$'\n'
at "$ME" add app/new-name.txt
tick; at "$ME" commit -q -m "Rename with an edit"
cp "$ME/assets/logo.png" "$ME/assets/logo-new.png"; printf 'tail' >>"$ME/assets/logo.png"
lines 1 6000 >"$ME/app/big.txt"
{ echo '{'; seq 1 3000 | sed 's/.*/  "pkg&": "1.0.&",/'; echo '  "end": true'; echo '}'; } >"$ME/package-lock.json"
mkdir -p "$ME/dist"; printf 'var a=%s;\n' "$(seq -s, 1 1500)" >"$ME/dist/app.min.js"
at "$ME" add assets app/big.txt package-lock.json dist
tick; at "$ME" commit -q -m "Binary, big and generated files"
put "$ME" app/noeol.txt $'no newline at the end, changed'
printf 'a\r\nB\r\nc\r\n' >"$ME/app/crlf.txt"
at "$ME" rm -q app/to-delete.txt
chmod +x "$ME/scripts/run.sh"
put "$ME" "docs/Новый файл с пробелами.md" $'# Привет\n'
ln -s c.txt "$ME/app/link-to-c"
: >"$ME/app/empty.txt"
put "$ME" app/oneline.js "$(printf 'x%.0s' $(seq 1 3000))"$'\n'
lines 1 300 | sed -e 's/^line 10$/line ten/' -e 's/^line 150$/line one hundred fifty/' -e 's/^line 290$/line two hundred ninety/' >"$ME/app/long.txt"
at "$ME" add -A
tick; at "$ME" commit -q -m "Odd files: no EOL, CRLF, deleted, mode, spaces, symlink, empty, long line, far hunks"
at "$ME" push -q -u origin feature/diff-zoo
expect flow feature/diff-zoo develop open "rename, binary, big, generated, CRLF, mode, symlink, spaces in path"

# Hundreds of files in one branch.
sw "$ME" develop; sw "$ME" -b feature/many-files
for i in $(seq 1 400); do put "$ME" "gen/m$((i % 20))/file$i.txt" "file $i"$'\n'; done
at "$ME" add gen; tick; at "$ME" commit -q -m "Four hundred files"
at "$ME" push -q -u origin feature/many-files
expect flow feature/many-files develop open "400 files"

# Names: Cyrillic, very long, a tag with the same name, look-alikes of the bases.
sw "$ME" develop
n=0
for b in "feature/LT-77-исправить-поиск" \
         "feature/$(printf 'very-long-name-%.0s' $(seq 1 12))end" \
         bugfix/LT-5-typo release/1.2 develop-old master-copy hotfix-without-slash; do
  sw "$ME" develop; sw "$ME" -b "$b"
  n=$((n + 1)); commit "$ME" "Work on $b" "app/name-$n.txt" "$b"$'\n'
  at "$ME" push -q -u origin "$b"
  expect flow "$b" develop open "name"
done
sw "$ME" develop; sw "$ME" -b feature/tagged
commit "$ME" "Tagged branch" app/tg.txt $'tg\n'
at "$ME" push -q -u origin feature/tagged
at "$ME" tag feature/tagged master
expect flow feature/tagged develop open "a tag with the same name points at master"

# Checked out in a linked worktree.
sw "$ME" develop; sw "$ME" -b feature/worktree-checked
commit "$ME" "In a worktree" app/wt.txt $'wt\n'
at "$ME" push -q -u origin feature/worktree-checked
expect flow feature/worktree-checked develop open "checked out in ../me-wt: cannot delete or switch to"

# My branch a colleague adds to below: still mine.
sw "$ME" develop; sw "$ME" -b feature/mine-then-colleague
commit "$ME" "Started by me" app/mtc.txt $'me\n'
at "$ME" push -q -u origin feature/mine-then-colleague
expect flow feature/mine-then-colleague develop open "owner Me"

sw "$ME" develop; at "$ME" push -q origin develop

# Hotfixes. Each half compares with its own base.
sw "$ME" master
# LT-101: carried over by cherry-pick, nothing merged.
sw "$ME" -b hotfix/LT-101-ok master
commit "$ME" "LT-101 fix" app/h101.txt $'fix\n'
sw "$ME" -b hotfix/LT-101-ok-dev develop
pick "$ME" hotfix/LT-101-ok
# LT-102: two commits, only the first carried over.
sw "$ME" -b hotfix/LT-102-partial master
commit "$ME" "LT-102 fix a" app/h102a.txt $'a\n'
commit "$ME" "LT-102 fix b" app/h102b.txt $'b\n'
sw "$ME" -b hotfix/LT-102-partial-dev develop
pick "$ME" hotfix/LT-102-partial~1
# LT-103: merged into master, no -dev at all.
sw "$ME" -b hotfix/LT-103-merged master
commit "$ME" "LT-103 fix" app/h103.txt $'fix\n'
sw "$ME" master; merge "$ME" --no-ff -m "Merge hotfix/LT-103-merged" hotfix/LT-103-merged
# LT-104: merged on both sides: done.
sw "$ME" -b hotfix/LT-104-done master
commit "$ME" "LT-104 fix" app/h104.txt $'fix\n'
sw "$ME" master; merge "$ME" --no-ff -m "Merge hotfix/LT-104-done" hotfix/LT-104-done
sw "$ME" -b hotfix/LT-104-done-dev develop
pick "$ME" hotfix/LT-104-done
sw "$ME" develop; merge "$ME" --no-ff -m "Merge hotfix/LT-104-done-dev" hotfix/LT-104-done-dev
# LT-105: the -dev side has the same content in one squashed commit.
sw "$ME" -b hotfix/LT-105-squash master
commit "$ME" "LT-105 fix a" app/h105.txt $'a\n'
commit "$ME" "LT-105 fix b" app/h105.txt $'a\nb\n'
sw "$ME" -b hotfix/LT-105-squash-dev develop
commit "$ME" "LT-105 fix (squashed)" app/h105.txt $'a\nb\n'
# LT-106: carrying over conflicts: develop changed the same line.
sw "$ME" develop
commit "$ME" "Develop changes beta" app/c2.txt $'alpha\nbeta (develop)\ngamma\n'
sw "$ME" -b hotfix/LT-106-conflict master
commit "$ME" "LT-106 fix beta" app/c2.txt $'alpha\nbeta (fixed)\ngamma\n'
sw "$ME" -b hotfix/LT-106-conflict-dev develop
# LT-107: just started, master half only, not merged.
sw "$ME" -b hotfix/LT-107-wip master
commit "$ME" "LT-107 work in progress" app/h107.txt $'wip\n'
# LT-108: the task text itself ends in "dev", so the master half ends in -dev too.
sw "$ME" -b hotfix/LT-108-fix-on-dev master
commit "$ME" "LT-108 fix" app/h108.txt $'fix\n'
sw "$ME" -b hotfix/LT-108-fix-on-dev-dev develop
pick "$ME" hotfix/LT-108-fix-on-dev
# A fix made straight on master.
sw "$ME" master
commit "$ME" "Manual fix on master" app/manual.txt $'manual\n'
at "$ME" push -q origin master develop
for b in LT-101-ok LT-101-ok-dev LT-102-partial LT-102-partial-dev LT-103-merged LT-104-done LT-104-done-dev \
         LT-105-squash LT-105-squash-dev LT-106-conflict LT-106-conflict-dev LT-107-wip LT-108-fix-on-dev LT-108-fix-on-dev-dev; do
  at "$ME" push -q -u origin "hotfix/$b"
done
expect flow hotfix/LT-101-ok master open "radar: carried over"
expect flow hotfix/LT-101-ok-dev develop open
expect flow hotfix/LT-102-partial master open "radar: 1 of 2 commits missing on -dev"
expect flow hotfix/LT-103-merged master merged "radar alarm: in master, no -dev"
expect flow hotfix/LT-104-done master merged "radar: done, pair can be deleted"
expect flow hotfix/LT-104-done-dev develop merged
expect flow hotfix/LT-105-squash master open "radar: carried over by content"
expect flow hotfix/LT-106-conflict master open "radar: carrying over conflicts in app/c2.txt"
expect flow hotfix/LT-106-conflict-dev develop merged "no own commits yet"
expect flow hotfix/LT-107-wip master open "radar: no -dev yet"
expect flow hotfix/LT-108-fix-on-dev master open "master half whose name ends in -dev"
expect flow hotfix/LT-108-fix-on-dev-dev develop open "its pair"
expect flow "" "" "" "radar alarm: 'Manual fix on master' is not in develop"

# The colleague: own branches, a merge into develop, develop running ahead.
COL=$W/colleague
"$GIT" clone -q "$W/origin.git" "$COL"
colleague "$COL"
sw "$COL" -b feature/colleague-open origin/develop
commit "$COL" "Colleague's open work" app/col.txt $'col\n'
at "$COL" push -q -u origin feature/colleague-open
expect flow feature/colleague-open develop open "remote only, owner Коллега Петров"
sw "$COL" -b feature/colleague-merged origin/develop
commit "$COL" "Colleague's merged work" app/colm.txt $'colm\n'
at "$COL" push -q -u origin feature/colleague-merged
sw "$COL" develop
merge "$COL" --no-ff -m "Merge feature/colleague-merged" feature/colleague-merged
expect flow feature/colleague-merged develop merged "remote only, someone else's"
sw "$COL" -b feature/colleague-started origin/develop
commit "$COL" "Started by the colleague" app/cs.txt $'cs\n'
at "$COL" push -q -u origin feature/colleague-started
expect flow feature/colleague-started develop open "owner Коллега Петров, Me added a commit"
at "$COL" fetch -q origin
sw "$COL" -b feature/mine-then-colleague origin/feature/mine-then-colleague
commit "$COL" "Colleague adds to my branch" app/mtc.txt $'me\ncolleague\n'
at "$COL" push -q origin feature/mine-then-colleague
sw "$COL" develop
for i in $(seq 1 25); do commit "$COL" "Develop runs ahead $i" app/ahead.txt "$i"$'\n'; done
at "$COL" push -q origin develop
sw "$COL" master
commit "$COL" "Colleague's commit on master" app/master-col.txt $'col\n'
at "$COL" push -q origin master

# Back to me: see the colleague's work; my master and develop now lag behind origin.
at "$ME" fetch -q --prune origin
sw "$ME" -b feature/colleague-started origin/feature/colleague-started
commit "$ME" "Me adds to the colleague's branch" app/cs.txt $'cs\nme\n'
at "$ME" push -q origin feature/colleague-started
sw "$ME" feature/mine-then-colleague
at "$ME" merge -q --ff-only origin/feature/mine-then-colleague
sw "$ME" master
commit "$ME" "Local tweak on master, not pushed" app/local-master.txt $'local\n'
expect flow "" "" "" "local master: 1 unpushed, 1 behind origin; local develop behind origin/develop by 25+"

at "$ME" worktree add -q "$W/me-wt" feature/worktree-checked
sw "$ME" feature/open
put "$ME" app/o.txt $'o\nwork in progress\n'
tick; at "$ME" stash push -q -m "wip on feature/open"
at "$ME" fetch -q --prune origin

# ---------------------------------------------------------------------------------------------
# Copies of flow in odd states. Built from flow's origin, so the branches are the same.
# ---------------------------------------------------------------------------------------------
clone_flow() { "$GIT" clone -q "$W/origin.git" "$1"; me "$1"; }

# Detached HEAD.
clone_flow "$DEST/detached"; sw "$DEST/detached" --detach origin/develop~3

# Stopped in the middle of a rebase and of a merge.
clone_flow "$DEST/rebasing"
sw "$DEST/rebasing" -b feature/conflict origin/feature/conflict
tick; at "$DEST/rebasing" rebase -q origin/develop >/dev/null 2>&1 || true
clone_flow "$DEST/merging"
sw "$DEST/merging" -b feature/conflict-multi origin/feature/conflict-multi
tick; at "$DEST/merging" merge -q origin/develop >/dev/null 2>&1 || true

# Shallow: the merge base may be missing.
"$GIT" clone -q --depth 3 --no-single-branch "file://$W/origin.git" "$DEST/shallow"; me "$DEST/shallow"

# The server cannot be reached.
clone_flow "$DEST/broken-origin"
at "$DEST/broken-origin" remote set-url origin "$DEST/no-such-server/origin.git"

# A token in the remote URL: the error report must hide it. Fetch fails (no such host).
clone_flow "$DEST/token-url"
at "$DEST/token-url" remote set-url origin "https://user:s3cr3t-token-123@git.example.invalid/shop.git"

# Every push is refused by a hook.
clone_flow "$DEST/hook-fails"
printf '#!/bin/sh\necho "pre-push: rejected by the sandbox hook" >&2\nexit 1\n' >"$DEST/hook-fails/.git/hooks/pre-push"
chmod +x "$DEST/hook-fails/.git/hooks/pre-push"

# Spaces and Cyrillic in the repository path.
mkdir -p "$DEST/Папка с пробелами"
clone_flow "$DEST/Папка с пробелами/shop"

# ---------------------------------------------------------------------------------------------
# Other shapes of repository.
# ---------------------------------------------------------------------------------------------

# No develop: hotfix is one branch from master.
W=$DEST/nodevelop; world "$W"; ME=$W/me
commit "$ME" "Initial commit" README.md $'# Lib\n' lib.txt $'v1\n'
at "$ME" push -q -u origin master
sw "$ME" -b feature/merged; commit "$ME" "Merged" m.txt $'m\n'
sw "$ME" master; merge "$ME" --no-ff -m "Merge feature/merged" feature/merged
sw "$ME" -b feature/conflict master~1; commit "$ME" "Branch v2" lib.txt $'v2 branch\n'
sw "$ME" master; commit "$ME" "Master v2" lib.txt $'v2 master\n'
sw "$ME" -b feature/open; commit "$ME" "Open" o.txt $'o\n'
sw "$ME" -b hotfix/LT-1-fix master; commit "$ME" "LT-1 fix" h.txt $'h\n'
sw "$ME" master; merge "$ME" --no-ff -m "Merge hotfix/LT-1-fix" hotfix/LT-1-fix
sw "$ME" -b hotfix/LT-2-wip; commit "$ME" "LT-2 wip" h2.txt $'h2\n'
at "$ME" push -q -u origin master feature/merged feature/conflict feature/open hotfix/LT-1-fix hotfix/LT-2-wip
sw "$ME" feature/open
expect nodevelop feature/merged master merged
expect nodevelop feature/conflict master conflict "lib.txt"
expect nodevelop feature/open master open
expect nodevelop hotfix/LT-1-fix master merged "radar: reached master, no alarm"
expect nodevelop hotfix/LT-2-wip master open "radar: not in master yet"

# main instead of master.
W=$DEST/main-develop; world "$W" main; ME=$W/me
commit "$ME" "Initial commit" README.md $'# App\n'
at "$ME" push -q -u origin main
at "$ME" branch develop; at "$ME" push -q -u origin develop
sw "$ME" develop; sw "$ME" -b feature/open; commit "$ME" "Open" o.txt $'o\n'
at "$ME" push -q -u origin feature/open
sw "$ME" -b hotfix/LT-9-fix main; commit "$ME" "LT-9" h.txt $'h\n'
at "$ME" push -q -u origin hotfix/LT-9-fix
expect main-develop feature/open develop open
expect main-develop hotfix/LT-9-fix main open "stable base is main"

# Both master and main. mm-head: origin/HEAD says main. mm-nohead: no origin/HEAD, main is fresher.
# mm-local-main: main exists only locally, so master (on the server) wins.
for v in mm-head mm-nohead mm-local-main; do
  W=$DEST/$v; world "$W"; ME=$W/me
  commit "$ME" "Initial commit" README.md $'# Moved\n'
  at "$ME" branch develop
  commit "$ME" "Old master work" old.txt $'old\n'
  sw "$ME" -b main
  commit "$ME" "Moved to main" new.txt $'new\n'
  sw "$ME" develop; sw "$ME" -b feature/x; commit "$ME" "X" x.txt $'x\n'
  if [ "$v" = mm-local-main ]; then
    at "$ME" push -q -u origin master develop feature/x
  else
    at "$ME" push -q -u origin master main develop feature/x
  fi
  if [ "$v" = mm-head ]; then
    "$GIT" -C "$W/origin.git" symbolic-ref HEAD refs/heads/main
    at "$ME" remote set-head origin main >/dev/null
  fi
done
expect mm-head "" "" "" "stable base: main (origin/HEAD)"
expect mm-nohead "" "" "" "stable base: main (fresher commits)"
expect mm-local-main "" "" "" "stable base: master (only one on the server)"

# A submodule, and a branch that moves it.
W=$DEST/submodule; world "$W"; ME=$W/me
commit "$ME" "Initial commit" README.md $'# With a submodule\n'
tick; at "$ME" -c protocol.file.allow=always submodule add -q "$DEST/nodevelop/origin.git" lib >/dev/null
at "$ME" commit -q -m "Add lib as a submodule"
at "$ME" branch develop
at "$ME" push -q -u origin master develop
sw "$ME" develop; sw "$ME" -b feature/bump-lib
at "$ME/lib" checkout -q HEAD~1
at "$ME" add lib; tick; at "$ME" commit -q -m "Move lib back one commit"
at "$ME" push -q -u origin feature/bump-lib
expect submodule feature/bump-lib develop open "diff of a gitlink"

# Empty: no commits at all.
"$GIT" init -q -b master "$DEST/empty"; me "$DEST/empty"

# Not a repository.
mkdir -p "$DEST/not-a-repo"; echo "just a folder" >"$DEST/not-a-repo/file.txt"

# ---------------------------------------------------------------------------------------------
cat >"$DEST/README.md" <<EOF
# Musubiko test bench

Built by scripts/sandbox.sh with $("$GIT" --version) at $(date '+%Y-%m-%d %H:%M').
"me" clones belong to Me <me@example.com>; the colleague is Коллега Петров <petrov@example.com>.
Every world has origin.git next to it. Rebuild with \`scripts/sandbox.sh\`; expected.tsv says
what the app should show.

| Repository | What it is |
|---|---|
| flow/me | master + develop: every branch status, sync state, gone upstream, stale, odd files, odd names, hotfixes in every radar state; a stash; a worktree at flow/me-wt; local master ahead 1 and behind 1, local develop behind 25+ |
| flow/colleague | the colleague's clone, for moving branches on the server during a test |
| flow/me-wt | linked worktree of flow/me on feature/worktree-checked |
| detached | flow, HEAD detached at origin/develop~3 |
| rebasing | flow, stopped in a rebase of feature/conflict onto develop |
| merging | flow, stopped in a merge of develop into feature/conflict-multi |
| shallow | flow cloned with --depth 3 |
| broken-origin | flow whose origin points nowhere |
| token-url | flow whose origin is https://user:<token>@git.example.invalid — the report must hide the token |
| hook-fails | flow whose pre-push hook always fails |
| Папка с пробелами/shop | flow at a path with spaces and Cyrillic |
| nodevelop/me | master only: hotfix is one branch |
| main-develop/me | main + develop |
| mm-head/me, mm-nohead/me, mm-local-main/me | master and main together |
| submodule/me | a submodule and a branch that moves it |
| empty | git init, no commits |
| not-a-repo | a plain folder |
| flow/me/app | a folder inside a repository: opens flow/me |
| flow/origin.git | a bare repository |

To make a working copy dirty: \`echo x >> flow/me/README.md; touch flow/me/new.txt\`.
EOF

echo "Sandbox: $DEST ($(wc -l <"$EXPECTED" | tr -d ' ') expectations, git $("$GIT" --version | awk '{print $3}'))"
