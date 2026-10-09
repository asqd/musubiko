use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::{Error, Result};

/// `git merge-tree --write-tree` appeared in 2.38; merge status depends on it.
pub const MIN_VERSION: (u32, u32, u32) = (2, 38, 0);

/// A git binary new enough for Musubiko.
#[derive(Debug, Clone)]
pub struct Git {
    bin: PathBuf,
    version: (u32, u32, u32),
}

impl Git {
    /// Picks the newest git among `$MUSUBIKO_GIT`, `$PATH` and the usual install locations.
    /// An app started from Finder sees a short PATH, so the fixed locations matter.
    pub fn discover() -> Result<Git> {
        let mut found: Vec<(PathBuf, (u32, u32, u32))> = Vec::new();
        for bin in candidates() {
            if found.iter().any(|(b, _)| *b == bin) {
                continue;
            }
            if let Some(v) = version_of(&bin) {
                found.push((bin, v));
            }
        }
        found
            .iter()
            .filter(|(_, v)| *v >= MIN_VERSION)
            .max_by_key(|(_, v)| *v)
            .map(|(bin, version)| Git {
                bin: bin.clone(),
                version: *version,
            })
            .ok_or_else(|| Error::GitNotFound {
                found: found
                    .iter()
                    .map(|(b, v)| format!("{} ({})", b.display(), fmt_version(*v)))
                    .collect(),
            })
    }

    pub fn path(&self) -> &Path {
        &self.bin
    }

    pub fn version(&self) -> String {
        fmt_version(self.version)
    }

    fn command(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.bin);
        cmd.current_dir(dir)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        cmd
    }

    /// Runs git and returns its raw output whatever the exit code.
    pub(crate) fn output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        Ok(self.command(dir, args).output()?)
    }

    /// Runs git with `input` on stdin and returns stdout (`git patch-id` reads a patch this way).
    pub(crate) fn run_with_stdin(
        &self,
        dir: &Path,
        args: &[&str],
        input: Vec<u8>,
    ) -> Result<String> {
        let out = self.output_with(dir, args, &[], input)?;
        if !out.status.success() {
            return Err(Error::Command {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Runs git with extra environment and `input` on stdin; returns the raw output.
    pub(crate) fn output_with(
        &self,
        dir: &Path,
        args: &[&str],
        env: &[(&str, &Path)],
        input: Vec<u8>,
    ) -> Result<Output> {
        use std::io::Write;
        use std::process::Stdio;
        let mut cmd = self.command(dir, args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        // Write from another thread: a large patch would otherwise fill the pipe and deadlock.
        let writer = std::thread::spawn(move || stdin.write_all(&input));
        let out = child.wait_with_output()?;
        let _ = writer.join();
        Ok(out)
    }

    /// Runs git and returns stdout; a non-zero exit becomes [`Error::Command`].
    pub(crate) fn run(&self, dir: &Path, args: &[&str]) -> Result<String> {
        let out = self.output(dir, args)?;
        if !out.status.success() {
            return Err(Error::Command {
                args: args.join(" "),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn candidates() -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(p) = std::env::var_os("MUSUBIKO_GIT") {
        list.push(PathBuf::from(p));
    }
    if let Some(path) = std::env::var_os("PATH") {
        list.extend(std::env::split_paths(&path).map(|d| d.join("git")));
    }
    for p in [
        "/opt/homebrew/bin/git",
        "/usr/local/bin/git",
        "/usr/bin/git",
    ] {
        list.push(PathBuf::from(p));
    }
    list.into_iter().filter(|p| p.is_file()).collect()
}

fn version_of(bin: &Path) -> Option<(u32, u32, u32)> {
    let out = Command::new(bin)
        .arg("--version")
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// Parses `git version 2.39.5 (Apple Git-154)` and `git version 2.47.0.windows.1`.
pub(crate) fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let v = s
        .trim()
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut it = v.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

pub(crate) fn fmt_version(v: (u32, u32, u32)) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version("git version 2.39.5 (Apple Git-154)\n"),
            Some((2, 39, 5))
        );
        assert_eq!(
            parse_version("git version 2.47.0.windows.1"),
            Some((2, 47, 0))
        );
        assert_eq!(parse_version("git version 2.40"), Some((2, 40, 0)));
        assert_eq!(parse_version("nope"), None);
    }
}
