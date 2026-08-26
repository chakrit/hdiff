use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub struct InstallGitDiffPager {
    pub executable: PathBuf,
}

pub struct InstallGitDiffPagerContext;

pub struct InstallGitDiffPagerReport {
    pub completed_commands: Vec<String>,
}

impl InstallGitDiffPager {
    pub fn run(
        self,
        _context: &mut InstallGitDiffPagerContext,
    ) -> io::Result<InstallGitDiffPagerReport> {
        let config_path = global_config_path()?;

        let backup_path = BackupGlobalGitConfig { path: &config_path }.run()?;

        let command = pager_command(&self.executable)?;
        SetGlobalGitConfig {
            key: "pager.diff",
            value: &command,
        }
        .run()?;
        let mut completed_commands = Vec::new();
        if let Some(backup_path) = backup_path {
            completed_commands.push(copy_command(&config_path, &backup_path)?);
        }
        completed_commands.push(format!("git config --global pager.diff {command}"));

        Ok(InstallGitDiffPagerReport { completed_commands })
    }
}

struct BackupGlobalGitConfig<'a> {
    path: &'a Path,
}

impl BackupGlobalGitConfig<'_> {
    fn run(self) -> io::Result<Option<PathBuf>> {
        if !self.path.exists() {
            return Ok(None);
        }

        let backup_path = self.path.with_extension("bak");
        fs::copy(self.path, &backup_path)?;

        Ok(Some(backup_path))
    }
}

struct SetGlobalGitConfig<'a> {
    key: &'a str,
    value: &'a str,
}

impl SetGlobalGitConfig<'_> {
    fn run(self) -> io::Result<()> {
        let output = Command::new("git")
            .args(["config", "--global", self.key, self.value])
            .output()?;

        if output.status.success() {
            return Ok(());
        }

        Err(io::Error::other(String::from_utf8_lossy(&output.stderr)))
    }
}

fn global_config_path() -> io::Result<PathBuf> {
    let configured_path = env::var_os("GIT_CONFIG_GLOBAL");
    if let Some(path) = configured_path {
        return configured_global_path(path);
    }

    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME is required to locate the global Git config"))?;
    let home_config = home.join(".gitconfig");
    if home_config.exists() {
        return Ok(home_config);
    }

    let xdg_config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"))
        .join("git/config");
    if xdg_config.exists() {
        return Ok(xdg_config);
    }

    Ok(home_config)
}

fn configured_global_path(path: OsString) -> io::Result<PathBuf> {
    let path = PathBuf::from(path);
    if path == Path::new("/dev/null") {
        return Err(io::Error::other(
            "GIT_CONFIG_GLOBAL=/dev/null disables global Git configuration",
        ));
    }

    Ok(path)
}

fn pager_command(executable: &Path) -> io::Result<String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| io::Error::other("hdiff executable path is not valid UTF-8"))?;

    Ok(shell_quote(executable))
}

fn copy_command(source: &Path, destination: &Path) -> io::Result<String> {
    let source = source
        .to_str()
        .ok_or_else(|| io::Error::other("Git config path is not valid UTF-8"))?;
    let destination = destination
        .to_str()
        .ok_or_else(|| io::Error::other("Git config backup path is not valid UTF-8"))?;

    Ok(format!(
        "cp {} {}",
        shell_quote(source),
        shell_quote(destination)
    ))
}

fn shell_quote(value: &str) -> String {
    let escaped_value = value.replace('\'', "'\"'\"'");

    format!("'{escaped_value}'")
}
