use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub struct InstallGitDifftool {
    pub executable: PathBuf,
}

pub struct InstallGitDifftoolContext;

impl InstallGitDifftool {
    pub fn run(self, _context: &mut InstallGitDifftoolContext) -> io::Result<()> {
        let config_path = global_config_path()?;

        BackupGlobalGitConfig { path: &config_path }.run()?;

        let command = difftool_command(&self.executable)?;
        SetGlobalGitConfig {
            key: "difftool.hdiff.cmd",
            value: &command,
        }
        .run()?;
        SetGlobalGitConfig {
            key: "diff.tool",
            value: "hdiff",
        }
        .run()
    }
}

struct BackupGlobalGitConfig<'a> {
    path: &'a Path,
}

impl BackupGlobalGitConfig<'_> {
    fn run(self) -> io::Result<()> {
        if self.path.exists() {
            fs::copy(self.path, self.path.with_extension("bak"))?;
        }

        Ok(())
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

fn difftool_command(executable: &Path) -> io::Result<String> {
    let executable = executable
        .to_str()
        .ok_or_else(|| io::Error::other("hdiff executable path is not valid UTF-8"))?;
    let quoted_executable = executable.replace('\'', "'\"'\"'");

    Ok(format!("'{quoted_executable}' \"$LOCAL\" \"$REMOTE\""))
}
