use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::{Error, Result};

/// `git merge-tree --write-tree` appeared in 2.38; merge status depends on it.
pub const MIN_VERSION: (u32, u32, u32) = (2, 38, 0);

/// How to get a newer git, for the "git not found" message.
pub(crate) const INSTALL_HINT: &str = if cfg!(windows) {
    "https://git-scm.com/download/win"
} else if cfg!(target_os = "macos") {
    "brew install git"
} else {
    "через пакетный менеджер дистрибутива"
};

/// A finished git command, as handed to the hook set with [`set_command_hook`].
pub struct Finished<'a> {
    pub dir: &'a Path,
    pub args: &'a [&'a str],
    /// `None` when git was killed by a signal.
    pub code: Option<i32>,
    pub took: Duration,
    pub stderr: &'a [u8],
}

type Hook = Box<dyn Fn(&Finished) + Send + Sync>;
static HOOK: OnceLock<Hook> = OnceLock::new();

/// Sees every git command Musubiko runs once it finishes; the app logs failures and slow ones.
/// Set once, before the first command; later calls are ignored.
pub fn set_command_hook(f: impl Fn(&Finished) + Send + Sync + 'static) {
    let _ = HOOK.set(Box::new(f));
}

fn report(dir: &Path, args: &[&str], started: Instant, out: &Output) {
    if let Some(hook) = HOOK.get() {
        hook(&Finished {
            dir,
            args,
            code: out.status.code(),
            took: started.elapsed(),
            stderr: &out.stderr,
        });
    }
}

/// A git binary new enough for Musubiko.
#[derive(Debug, Clone)]
pub struct Git {
    bin: PathBuf,
    version: (u32, u32, u32),
}

impl Git {
    /// The git named by `$MUSUBIKO_GIT` when it is set, otherwise the newest one among `$PATH`
    /// and the usual install locations. An app started from Finder sees a short PATH, so the
    /// fixed locations matter.
    pub fn discover() -> Result<Git> {
        match std::env::var_os("MUSUBIKO_GIT").filter(|p| !p.is_empty()) {
            Some(p) => Git::chosen(PathBuf::from(p)),
            None => Git::newest(),
        }
    }

    /// Exactly this git, as the user asked for it: an error if it does not run or is too old.
    pub fn chosen(bin: PathBuf) -> Result<Git> {
        let path = bin.display().to_string();
        match version_of(&bin) {
            Some(v) if v >= MIN_VERSION => Ok(Git { bin, version: v }),
            Some(v) => Err(Error::ChosenGit {
                path,
                problem: format!("это git {}", fmt_version(v)),
            }),
            None => Err(Error::ChosenGit {
                path,
                problem: "не запускается как git".into(),
            }),
        }
    }

    fn newest() -> Result<Git> {
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
        let mut cmd = new_command(&self.bin);
        cmd.current_dir(dir)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C");
        cmd
    }

    /// Runs git and returns its raw output whatever the exit code.
    pub(crate) fn output(&self, dir: &Path, args: &[&str]) -> Result<Output> {
        let started = Instant::now();
        let out = self.command(dir, args).output()?;
        report(dir, args, started, &out);
        Ok(out)
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
        let started = Instant::now();
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
        report(dir, args, started, &out);
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

/// A command for a console program. On Windows a GUI app would otherwise flash a console
/// window for every git call.
fn new_command(bin: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(bin);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn candidates() -> Vec<PathBuf> {
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    let mut list = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        list.extend(std::env::split_paths(&path).map(|d| d.join(exe)));
    }
    if cfg!(windows) {
        // Git for Windows, installed for everyone or for the current user.
        for (var, dir) in [
            ("ProgramFiles", r"Git\cmd"),
            ("ProgramW6432", r"Git\cmd"),
            ("LOCALAPPDATA", r"Programs\Git\cmd"),
        ] {
            if let Some(base) = std::env::var_os(var) {
                list.push(PathBuf::from(base).join(dir).join(exe));
            }
        }
    } else {
        for p in [
            "/opt/homebrew/bin/git",
            "/usr/local/bin/git",
            "/usr/bin/git",
        ] {
            list.push(PathBuf::from(p));
        }
    }
    list.into_iter().filter(|p| p.is_file()).collect()
}

fn version_of(bin: &Path) -> Option<(u32, u32, u32)> {
    let out = new_command(bin)
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

    #[test]
    fn a_chosen_git_is_used_as_is_or_refused() {
        let ok = Git::discover().expect("a git 2.38+ for the tests");
        let chosen = Git::chosen(ok.path().to_path_buf()).unwrap();
        assert_eq!(chosen.path(), ok.path());

        let missing = Git::chosen(PathBuf::from("/no/such/git")).unwrap_err();
        assert!(
            missing.to_string().contains("MUSUBIKO_GIT")
                && missing.to_string().contains("не запускается"),
            "{missing}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_chosen_git_that_is_too_old_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("musubiko-old-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("git");
        std::fs::write(&fake, "#!/bin/sh\necho 'git version 2.37.3'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = Git::chosen(fake).unwrap_err().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(e.contains("это git 2.37.3") && e.contains("2.38"), "{e}");
    }
}
