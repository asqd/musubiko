#!/usr/bin/env python3
"""Checks what Musubiko says about branches against plain git, computed independently.

    scripts/check-statuses.py REPO [--base BRANCH] [--musubiko PATH] [--git PATH]

Runs `musubiko branches --json` and, for every branch, recomputes with git: which ref is the
branch, which base it is compared with, ahead/behind, the status (merged, squashed, conflict and
its files, open, new — created and never committed to), sync with the remote copy, a gone upstream, the owner and HEAD. Also checks that
no branch is missing from the list. Exits 1 when something differs.

Two kinds of output: DIFF — Musubiko disagrees with git; NOTE — they agree, but git sees
something worth a look (a squash git cherry recognises, an upstream with another name).
"""

import argparse
import json
import os
import subprocess
import sys
import time

p = argparse.ArgumentParser()
p.add_argument("repo")
p.add_argument("--base")
p.add_argument("--musubiko", default=os.path.join(os.path.dirname(__file__), "..", "target", "debug", "musubiko"))
p.add_argument("--git", help="default: the git Musubiko itself chose")
p.add_argument("--quiet", action="store_true", help="only DIFF lines and the summary")
args = p.parse_args()

ENV = dict(os.environ, GIT_OPTIONAL_LOCKS="0", LC_ALL="C")


def git(*a, ok=(0,)):
    r = subprocess.run([args.git, "-C", args.repo, *a], capture_output=True, env=ENV)
    if r.returncode not in ok:
        raise RuntimeError(f"git {' '.join(a)}: {r.stderr.decode(errors='replace').strip()}")
    return r.returncode, r.stdout.decode(errors="replace")


def out(*a):
    return git(*a)[1]


def has(ref):
    return git("rev-parse", "-q", "--verify", ref + "^{commit}", ok=(0, 1))[0] == 0


started = time.time()
mus = [args.musubiko]
info = json.loads(subprocess.run(mus + ["info", args.repo, "--json"], capture_output=True, check=True).stdout)
cmd = mus + ["branches", args.repo, "--json"] + (["--base", args.base] if args.base else [])
r = subprocess.run(cmd, capture_output=True)
if r.returncode:
    sys.exit(f"musubiko branches failed: {r.stderr.decode().strip()}")
listed = json.loads(r.stdout)
took = time.time() - started

args.git = args.git or info["git"].split(" ", 1)[1]
flow = info["flow"]
MASTER, DEVELOP, PREFIX, SUFFIX = flow["master"], flow["develop"], flow["hotfixPrefix"], flow["devSuffix"]
REMOTE = info.get("remote")

# Every local and remote-tracking branch, by short name.
refs = {}
fmt = "%(refname)%00%(objectname)%00%(upstream)%00%(upstream:track)%00%(authoremail:trim)"
heads = ["refs/heads"] + ([f"refs/remotes/{REMOTE}/"] if REMOTE else [])
for line in out("for-each-ref", "--format=" + fmt, *heads).splitlines():
    name, sha, upstream, track, email = line.split("\0")
    if name.startswith("refs/heads/"):
        e = refs.setdefault(name[len("refs/heads/"):], {})
        e.update(local=sha, upstream=upstream, track=track)
    elif REMOTE and name.startswith(f"refs/remotes/{REMOTE}/"):
        short = name[len(f"refs/remotes/{REMOTE}/"):]
        if short != "HEAD":
            refs.setdefault(short, {})["remote"] = sha

chosen = args.base or (DEVELOP if DEVELOP in refs else MASTER)


def base_ref(name):
    """master and develop as on the server; any other base as the user has it."""
    e = refs.get(name, {})
    if name in (MASTER, DEVELOP):
        if "remote" in e:
            return f"refs/remotes/{REMOTE}/{name}"
        return f"refs/heads/{name}" if "local" in e else None
    return branch_ref(name)


def branch_ref(name):
    e = refs.get(name, {})
    if "local" in e:
        return f"refs/heads/{name}"
    return f"refs/remotes/{REMOTE}/{name}" if "remote" in e else None


def target_for(name):
    """The spec: a hotfix half ending in the suffix is the -dev half when its master half exists
    (or when there is no way to tell); otherwise it is a master half."""
    if not name.startswith(PREFIX):
        return chosen, None
    if name.endswith(SUFFIX):
        pair = name[: -len(SUFFIX)]
        if pair in refs:
            return DEVELOP, None
        if name + SUFFIX in refs:
            return MASTER, f"{name + SUFFIX} exists, so {name} is a master half"
        return DEVELOP, None
    return MASTER, None


def left_right(a, b):
    l, r = out("rev-list", "--left-right", "--count", f"{a}...{b}").split()
    return int(l), int(r)


diffs = notes = 0


def diff(name, what, got, want):
    global diffs
    diffs += 1
    print(f"DIFF  {name}: {what}: musubiko {got!r}, git {want!r}")


def note(name, what):
    global notes
    notes += 1
    if not args.quiet:
        print(f"NOTE  {name}: {what}")


expected_names = {n for n in refs if n not in (MASTER, DEVELOP, chosen)}
got_names = {b["name"] for b in listed}
for n in sorted(expected_names - got_names):
    diff(n, "listed", False, True)
for n in sorted(got_names - expected_names):
    diff(n, "listed", True, False)

head = git("symbolic-ref", "--short", "-q", "HEAD", ok=(0, 1))[1].strip() or None

for b in listed:
    name = b["name"]
    e = refs.get(name)
    if e is None:
        continue
    bref = branch_ref(name)

    if b["tip"]["hash"] != out("rev-parse", bref).strip():
        diff(name, "tip", b["tip"]["hash"], bref)
    loc = "both" if "local" in e and "remote" in e else "local" if "local" in e else "remote"
    if b["location"] != loc:
        diff(name, "location", b["location"], loc)
    if b["isHead"] != (head == name):
        diff(name, "isHead", b["isHead"], head == name)

    # Sync with the remote copy of the same name, and the configured upstream.
    sync = None
    if "local" in e and "remote" in e and e["local"] != e["remote"]:
        a, bh = left_right(e["local"], e["remote"])
        sync = {"ahead": a, "behind": bh}
    if b["sync"] != sync:
        diff(name, "sync", b["sync"], sync)
    gone = "local" in e and "gone" in e.get("track", "")
    if b["upstreamGone"] != gone:
        diff(name, "upstreamGone", b["upstreamGone"], gone)
    up = e.get("upstream")
    if up and REMOTE and up != f"refs/remotes/{REMOTE}/{name}" and not gone:
        note(name, f"tracks {up}, but sync is shown against {REMOTE}/{name}" if "remote" in e
             else f"tracks {up}; shown as never pushed")

    target, why = target_for(name)
    if b["target"] != target:
        diff(name, "target" + (f" ({why})" if why else ""), b["target"], target)
        continue  # everything below depends on the target
    tref = base_ref(target)
    if tref is None:
        if b["status"]["kind"] != "unknown":
            diff(name, "status", b["status"]["kind"], f"unknown (no {target})")
        continue

    behind, ahead = left_right(tref, bref)
    if (b["ahead"], b["behind"]) != (ahead, behind):
        diff(name, "ahead/behind", (b["ahead"], b["behind"]), (ahead, behind))

    creator = None
    if ahead == 0:
        assert git("merge-base", "--is-ancestor", bref, tref, ok=(0, 1))[0] == 0
        want = {"kind": "merged"}
        if "local" in e:
            code, log = git("reflog", "show", "--format=%gs%x00%ge", f"refs/heads/{name}", "--", ok=(0, 128))
            entries = [l.split("\0") for l in log.splitlines() if l]
            if code == 0 and entries and all(
                x[0].startswith(("branch: Created from", "Branch: renamed")) for x in entries
            ):
                want = {"kind": "new"}
                creator = entries[-1][1]
    else:
        code, mt = git("merge-tree", "--write-tree", "-z", "--name-only", "--no-messages", tref, bref, ok=(0, 1, 128))
        lines = mt.split("\0")
        if code == 0:
            base_tree = out("rev-parse", tref + "^{tree}").strip()
            want = {"kind": "squashed" if lines[0] == base_tree else "open"}
        elif code == 1:
            files = []
            for f in lines[1:]:
                if not f:
                    break
                if f not in files:
                    files.append(f)
            want = {"kind": "conflict", "files": files}
        else:
            want = {"kind": "unknown"}
    got = b["status"]
    if got["kind"] != want["kind"]:
        diff(name, "status", got["kind"], want["kind"])
    elif want["kind"] == "conflict" and sorted(got["files"]) != sorted(want["files"]):
        diff(name, "conflict files", got["files"], want["files"])

    # Second opinion on "open": git cherry knows patches that went in under other hashes.
    if want["kind"] == "open":
        cherry = out("cherry", tref, bref).split()  # "+ hash" / "- hash"
        marks = cherry[0::2]
        if marks and all(m == "-" for m in marks):
            note(name, "open, but git cherry finds every commit in the base (rebase or cherry-pick on top of later changes?)")

    # Owner: author of the oldest commit on the branch's own line (first parents, graph order).
    if creator:
        owner = creator
    elif ahead == 0:
        owner = out("log", "-1", "--format=%ae", bref).strip()
    else:
        line = out("log", "--first-parent", "--topo-order", "--format=%ae", f"{tref}..{bref}").split()
        owner = line[-1] if line else ""
    if b["owner"]["email"] != owner:
        diff(name, "owner", b["owner"]["email"], owner)

print(f"{len(listed)} branches against {chosen} (musubiko {took:.1f}s): {diffs} differ, {notes} notes")
sys.exit(1 if diffs else 0)
