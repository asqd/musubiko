//! Diffs the way a pull request shows them: a branch against the point where it left its base,
//! single commits, and a preview of merge conflicts.

use serde::Serialize;

use crate::{Commit, Error, Person, Repo, Result};

/// The empty tree: the "parent" of a root commit.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
/// Lines kept per file; the rest of a huge file (lock files, dumps) is dropped.
const MAX_FILE_LINES: usize = 5000;
/// Context lines around each conflict region.
const CONFLICT_CONTEXT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChange {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The enclosing function or class git prints after `@@`.
    pub section: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// Set for renames and copies.
    pub old_path: Option<String>,
    pub change: FileChange,
    pub binary: bool,
    /// `100644 → 100755` when only the mode changed along the way.
    pub mode_change: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub hunks: Vec<Hunk>,
    /// More than [`MAX_FILE_LINES`] lines: the hunks stop early.
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchDiff {
    pub branch: String,
    pub target: String,
    /// Where the branch left the target; the diff starts here, as in a pull request.
    pub merge_base: Commit,
    /// Hash of the branch tip, for reading file contents around hunks.
    pub tip: String,
    /// Commits of the branch, oldest first.
    pub commits: Vec<Commit>,
    pub files: Vec<FileDiff>,
}

/// A file that would conflict if the branch were merged into its base.
#[derive(Debug, Clone, Serialize)]
pub struct ConflictFile {
    pub path: String,
    pub regions: Vec<ConflictRegion>,
    /// Why there are no regions: the conflict is about deleting or renaming, not text.
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConflictRegion {
    /// 1-based line of the region in the merged file.
    pub line: u32,
    pub before: Vec<String>,
    /// The base branch's side.
    pub base: Vec<String>,
    /// The work branch's side.
    pub branch: Vec<String>,
    pub after: Vec<String>,
}

impl Repo {
    /// The branch's own changes: from its merge base with the target to its tip.
    pub fn branch_diff(&self, name: &str, base: &str, ignore_whitespace: bool) -> Result<BranchDiff> {
        let (branch_ref, target, target_ref) = self.resolve(name, base)?;
        let tip = self.run(&["rev-parse", &branch_ref])?.trim().to_string();
        let mb = self.git.output(&self.root, &["merge-base", &target_ref, &tip])?;
        if !mb.status.success() {
            return Err(Error::Command {
                args: format!("merge-base {target} {name}"),
                stderr: format!("у {name} и {target} нет общей истории"),
            });
        }
        let mb = String::from_utf8_lossy(&mb.stdout).trim().to_string();
        let merge_base = self
            .log(&["-1", &mb])?
            .pop()
            .ok_or_else(|| Error::NoBranch(mb.clone()))?;
        let commits = self.log(&["--reverse", &format!("{target_ref}..{tip}")])?;
        let files = parse_patch(&self.diff_text(&mb, &tip, ignore_whitespace)?);
        Ok(BranchDiff { branch: name.into(), target, merge_base, tip, commits, files })
    }

    /// One commit against its first parent.
    pub fn commit_diff(&self, hash: &str, ignore_whitespace: bool) -> Result<Vec<FileDiff>> {
        let parent = self.git.output(&self.root, &["rev-parse", "--verify", "-q", &format!("{hash}^")])?;
        let parent = if parent.status.success() {
            String::from_utf8_lossy(&parent.stdout).trim().to_string()
        } else {
            EMPTY_TREE.to_string()
        };
        Ok(parse_patch(&self.diff_text(&parent, hash, ignore_whitespace)?))
    }

    /// Lines of a file at a revision, for showing the context hidden between hunks.
    pub fn file_lines(&self, rev: &str, path: &str) -> Result<Vec<String>> {
        let text = self.run(&["cat-file", "blob", &format!("{rev}:{path}")])?;
        let mut lines: Vec<String> = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).to_string()).collect();
        if lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        Ok(lines)
    }

    /// Merges the branch into its target in memory and returns the conflicting regions.
    pub fn conflicts(&self, name: &str, base: &str) -> Result<Vec<ConflictFile>> {
        let (branch_ref, _, target_ref) = self.resolve(name, base)?;
        let out = self.git.output(
            &self.root,
            &[
                "-c",
                "merge.conflictStyle=merge",
                "merge-tree",
                "--write-tree",
                "-z",
                "--name-only",
                "--no-messages",
                &target_ref,
                &branch_ref,
            ],
        )?;
        if out.status.code() != Some(1) {
            return Ok(Vec::new());
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut parts = stdout.split(['\0', '\n']).filter(|p| !p.is_empty());
        let tree = parts.next().unwrap_or_default().to_string();
        let mut paths: Vec<&str> = parts.collect();
        paths.dedup();
        Ok(paths
            .into_iter()
            .map(|path| {
                let content = self.run(&["cat-file", "blob", &format!("{tree}:{path}")]);
                let regions = content.as_deref().map(parse_conflicts).unwrap_or_default();
                let note = regions.is_empty().then(|| {
                    "Конфликт не в тексте: файл удалён или переименован в одной ветке и изменён в другой".to_string()
                });
                ConflictFile { path: path.to_string(), regions, note }
            })
            .collect())
    }

    fn diff_text(&self, from: &str, to: &str, ignore_whitespace: bool) -> Result<String> {
        // Fixed prefixes and no external tools, whatever the user's diff config says.
        let mut args = vec![
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-relative",
            "-M",
            "-U3",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ];
        if ignore_whitespace {
            args.push("-w");
        }
        args.extend([from, to, "--"]);
        self.run(&args)
    }

    fn log(&self, args: &[&str]) -> Result<Vec<Commit>> {
        let mut all = vec!["log", "--format=%H%x00%h%x00%s%x00%an%x00%ae%x00%ct"];
        all.extend_from_slice(args);
        Ok(self
            .run(&all)?
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\0').collect();
                (f.len() >= 6).then(|| Commit {
                    hash: f[0].into(),
                    short: f[1].into(),
                    subject: f[2].into(),
                    author: Person { name: f[3].into(), email: f[4].into() },
                    date: f[5].parse().unwrap_or(0),
                })
            })
            .collect())
    }
}

/// Parses `git diff` output. Paths come from `rename`/`copy` lines and `---`/`+++` lines when
/// present, and from the `diff --git` header otherwise (mode-only changes, pure renames).
pub(crate) fn parse_patch(text: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut stored = 0usize;
    // Lines still expected in the current hunk, old side and new side.
    let (mut old_left, mut new_left) = (0u32, 0u32);

    for line in text.split('\n') {
        if old_left > 0 || new_left > 0 {
            let Some(f) = files.last_mut() else { break };
            let (kind, body) = match line.as_bytes().first() {
                Some(b'+') => (LineKind::Add, &line[1..]),
                Some(b'-') => (LineKind::Del, &line[1..]),
                Some(b'\\') => continue, // "\ No newline at end of file"
                Some(b' ') => (LineKind::Context, &line[1..]),
                _ => (LineKind::Context, ""),
            };
            match kind {
                LineKind::Add => {
                    new_left = new_left.saturating_sub(1);
                    f.additions += 1;
                }
                LineKind::Del => {
                    old_left = old_left.saturating_sub(1);
                    f.deletions += 1;
                }
                LineKind::Context => {
                    old_left = old_left.saturating_sub(1);
                    new_left = new_left.saturating_sub(1);
                }
            }
            if stored >= MAX_FILE_LINES {
                f.truncated = true;
                continue;
            }
            stored += 1;
            if let Some(h) = f.hunks.last_mut() {
                h.lines.push(Line { kind, text: body.strip_suffix('\r').unwrap_or(body).to_string() });
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (a, b) = parse_header_paths(rest);
            stored = 0;
            files.push(FileDiff {
                path: b,
                old_path: Some(a),
                change: FileChange::Modified,
                binary: false,
                mode_change: None,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
                truncated: false,
            });
            continue;
        }
        let Some(f) = files.last_mut() else { continue };
        if let Some(h) = line.strip_prefix("@@ ") {
            if let Some(hunk) = parse_hunk_header(h) {
                old_left = hunk.old_lines;
                new_left = hunk.new_lines;
                if stored < MAX_FILE_LINES {
                    f.hunks.push(hunk);
                } else {
                    f.truncated = true;
                }
            }
        } else if line.starts_with("new file mode") {
            f.change = FileChange::Added;
        } else if line.starts_with("deleted file mode") {
            f.change = FileChange::Deleted;
        } else if let Some(p) = line.strip_prefix("rename from ") {
            f.change = FileChange::Renamed;
            f.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            f.path = unquote(p);
        } else if let Some(p) = line.strip_prefix("copy from ") {
            f.change = FileChange::Copied;
            f.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy to ") {
            f.path = unquote(p);
        } else if let Some(m) = line.strip_prefix("old mode ") {
            f.mode_change = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("new mode ") {
            f.mode_change = Some(format!("{} → {m}", f.mode_change.take().unwrap_or_default()));
        } else if let Some(p) = line.strip_prefix("--- ") {
            if let Some(p) = strip_side(p, "a/") {
                f.old_path = Some(p);
            }
        } else if let Some(p) = line.strip_prefix("+++ ") {
            if let Some(p) = strip_side(p, "b/") {
                f.path = p;
            }
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            f.binary = true;
        }
    }

    for f in &mut files {
        if f.change == FileChange::Deleted {
            if let Some(old) = f.old_path.clone() {
                f.path = old;
            }
        }
        if !matches!(f.change, FileChange::Renamed | FileChange::Copied) {
            f.old_path = None;
        }
    }
    files
}

/// `a/x b/y` from the `diff --git` header. Both halves are equal unless the file was renamed,
/// and for renames the `rename from/to` lines override this guess.
fn parse_header_paths(rest: &str) -> (String, String) {
    if rest.starts_with('"') {
        let (a, tail) = split_quoted(rest);
        let b = tail.trim_start();
        let b = if b.starts_with('"') { split_quoted(b).0 } else { b.to_string() };
        return (strip_prefix_owned(a, "a/"), strip_prefix_owned(b, "b/"));
    }
    // Same path on both sides: "a/" + p + " b/" + p.
    if rest.len() >= 5 && (rest.len() - 5) % 2 == 0 {
        let n = (rest.len() - 5) / 2;
        if rest.is_char_boundary(2 + n) && rest.is_char_boundary(5 + n) {
            let (a, b) = (&rest[2..2 + n], &rest[5 + n..]);
            if rest.starts_with("a/") && &rest[2 + n..5 + n] == " b/" && a == b {
                return (a.to_string(), b.to_string());
            }
        }
    }
    match rest.split_once(" b/") {
        Some((a, b)) => (a.trim_start_matches("a/").to_string(), b.to_string()),
        None => (rest.to_string(), rest.to_string()),
    }
}

fn strip_prefix_owned(s: String, prefix: &str) -> String {
    s.strip_prefix(prefix).map(str::to_string).unwrap_or(s)
}

/// Path from a `---`/`+++` line; `None` for `/dev/null`.
fn strip_side(p: &str, prefix: &str) -> Option<String> {
    let p = p.strip_suffix('\t').unwrap_or(p);
    if p == "/dev/null" {
        return None;
    }
    let p = unquote(p);
    Some(p.strip_prefix(prefix).map(str::to_string).unwrap_or(p))
}

/// Undoes git's C-style quoting of unusual paths: `"tab\there"`, `"\320\277"`.
fn unquote(s: &str) -> String {
    if s.starts_with('"') { split_quoted(s).0 } else { s.to_string() }
}

/// Reads one quoted string at the start of `s`; returns it and the rest after the closing quote.
fn split_quoted(s: &str) -> (String, &str) {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (String::from_utf8_lossy(&out).into_owned(), &s[i + 1..]),
            b'\\' if i + 1 < bytes.len() => {
                i += 1;
                match bytes[i] {
                    b'n' => out.push(b'\n'),
                    b't' => out.push(b'\t'),
                    b'r' => out.push(b'\r'),
                    b'0'..=b'7' if i + 2 < bytes.len() => {
                        let oct = &s[i..i + 3];
                        out.push(u8::from_str_radix(oct, 8).unwrap_or(b'?'));
                        i += 2;
                    }
                    other => out.push(other),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    (String::from_utf8_lossy(&out).into_owned(), "")
}

/// `-12,7 +12,9 @@ def fetch` → a hunk without lines.
fn parse_hunk_header(h: &str) -> Option<Hunk> {
    let (ranges, section) = h.split_once(" @@").unwrap_or((h, ""));
    let mut it = ranges.split_whitespace();
    let range = |r: &str| -> Option<(u32, u32)> {
        let (start, len) = r.split_once(',').unwrap_or((r, "1"));
        Some((start.parse().ok()?, len.parse().ok()?))
    };
    let (old_start, old_lines) = range(it.next()?.strip_prefix('-')?)?;
    let (new_start, new_lines) = range(it.next()?.strip_prefix('+')?)?;
    Some(Hunk { old_start, old_lines, new_start, new_lines, section: section.trim().to_string(), lines: Vec::new() })
}

/// Finds `<<<<<<<` … `=======` … `>>>>>>>` regions (a `|||||||` base section is skipped).
pub(crate) fn parse_conflicts(content: &str) -> Vec<ConflictRegion> {
    let lines: Vec<&str> = content.lines().collect();
    let mut regions: Vec<ConflictRegion> = Vec::new();
    let mut last_end = 0usize;
    let mut i = 0usize;
    while i < lines.len() {
        if !lines[i].starts_with("<<<<<<<") {
            i += 1;
            continue;
        }
        let start = i;
        let (mut base, mut branch) = (Vec::new(), Vec::new());
        let mut side = 0; // 0 base, 1 skipped ancestor, 2 branch
        i += 1;
        while i < lines.len() && !lines[i].starts_with(">>>>>>>") {
            let l = lines[i];
            if l.starts_with("|||||||") {
                side = 1;
            } else if l.starts_with("=======") {
                side = 2;
            } else if side == 0 {
                base.push(l.to_string());
            } else if side == 2 {
                branch.push(l.to_string());
            }
            i += 1;
        }
        let end = i.min(lines.len().saturating_sub(1));
        let from = start.saturating_sub(CONFLICT_CONTEXT).max(last_end);
        let before = lines[from..start].iter().map(|s| s.to_string()).collect();
        let after_end = (end + 1 + CONFLICT_CONTEXT).min(lines.len());
        let after = lines[(end + 1).min(after_end)..after_end]
            .iter()
            .take_while(|l| !l.starts_with("<<<<<<<"))
            .map(|s| s.to_string())
            .collect();
        regions.push(ConflictRegion { line: start as u32 + 1, before, base, branch, after });
        last_end = end + 1;
        i = end + 1;
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/app/price.rb b/app/price.rb
index 1111111..2222222 100644
--- a/app/price.rb
+++ b/app/price.rb
@@ -1,3 +1,4 @@ class Price
 a
-b
+B
+c
 d
diff --git a/old name.txt b/new name.txt
similarity index 100%
rename from old name.txt
rename to new name.txt
diff --git a/img.png b/img.png
new file mode 100644
index 0000000..3333333
Binary files /dev/null and b/img.png differ
diff --git a/run.sh b/run.sh
old mode 100644
new mode 100755
diff --git a/gone.rb b/gone.rb
deleted file mode 100644
index 4444444..0000000
--- a/gone.rb
+++ /dev/null
@@ -1 +0,0 @@
-puts 1
\\ No newline at end of file
diff --git \"a/\\321\\204\\320\\260\\320\\271\\320\\273.txt\" \"b/\\321\\204\\320\\260\\320\\271\\320\\273.txt\"
--- \"a/\\321\\204\\320\\260\\320\\271\\320\\273.txt\"
+++ \"b/\\321\\204\\320\\260\\320\\271\\320\\273.txt\"
@@ -1 +1 @@
-x
+y
";

    #[test]
    fn parses_files_and_hunks() {
        let files = parse_patch(PATCH);
        let summary: Vec<(&str, Option<&str>, FileChange, bool, u32, u32)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.old_path.as_deref(), f.change, f.binary, f.additions, f.deletions))
            .collect();
        assert_eq!(
            summary,
            [
                ("app/price.rb", None, FileChange::Modified, false, 2, 1),
                ("new name.txt", Some("old name.txt"), FileChange::Renamed, false, 0, 0),
                ("img.png", None, FileChange::Added, true, 0, 0),
                ("run.sh", None, FileChange::Modified, false, 0, 0),
                ("gone.rb", None, FileChange::Deleted, false, 0, 1),
                ("файл.txt", None, FileChange::Modified, false, 1, 1),
            ]
        );
        let h = &files[0].hunks[0];
        assert_eq!((h.old_start, h.old_lines, h.new_start, h.new_lines, h.section.as_str()), (1, 3, 1, 4, "class Price"));
        let kinds: Vec<(LineKind, &str)> = h.lines.iter().map(|l| (l.kind, l.text.as_str())).collect();
        assert_eq!(
            kinds,
            [(LineKind::Context, "a"), (LineKind::Del, "b"), (LineKind::Add, "B"), (LineKind::Add, "c"), (LineKind::Context, "d")]
        );
        assert_eq!(files[3].mode_change.as_deref(), Some("100644 → 100755"));
        assert_eq!(files[4].hunks[0].lines.len(), 1, "the no-newline marker is not a line");
    }

    #[test]
    fn hunk_lines_that_look_like_headers_stay_lines() {
        let patch = "diff --git a/x.md b/x.md\n--- a/x.md\n+++ b/x.md\n@@ -1,2 +1,2 @@\n--- a/not-a-header\n+diff --git a/still b/content\n x\n";
        let files = parse_patch(patch);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].hunks[0].lines.len(), 3);
        assert_eq!(files[0].hunks[0].lines[0].text, "-- a/not-a-header");
    }

    #[test]
    fn finds_conflict_regions_with_context() {
        let content = "one\ntwo\nthree\nfour\n<<<<<<< refs/remotes/origin/develop\nbase side\n=======\nbranch side\nsecond\n>>>>>>> refs/heads/feature\nfive\n";
        let r = parse_conflicts(content);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].line, 5);
        assert_eq!(r[0].before, ["two", "three", "four"]);
        assert_eq!(r[0].base, ["base side"]);
        assert_eq!(r[0].branch, ["branch side", "second"]);
        assert_eq!(r[0].after, ["five"]);
    }

    #[test]
    fn skips_the_ancestor_section_of_diff3_markers() {
        let content = "<<<<<<< ours\na\n||||||| base\nold\n=======\nb\n>>>>>>> theirs\n";
        let r = parse_conflicts(content);
        assert_eq!((r[0].base.as_slice(), r[0].branch.as_slice()), (&["a".to_string()][..], &["b".to_string()][..]));
    }
}
