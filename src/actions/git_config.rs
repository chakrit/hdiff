use std::{
    env,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

pub struct InstallGitDiffPager {
    pub executable: PathBuf,
}

pub struct InstallGitDiffPagerContext {
    pub environment: GitConfigEnvironment,
}

/// The environment variables that decide which file `git config --global` writes.
pub struct GitConfigEnvironment {
    pub git_config_global: Option<OsString>,
    pub home: Option<OsString>,
    pub xdg_config_home: Option<OsString>,
}

pub struct InstallGitDiffPagerReport {
    pub completed_commands: Vec<String>,
}

impl GitConfigEnvironment {
    pub fn of_process() -> Self {
        Self {
            git_config_global: env::var_os("GIT_CONFIG_GLOBAL"),
            home: env::var_os("HOME"),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME"),
        }
    }
}

impl InstallGitDiffPager {
    pub fn run(
        self,
        context: &mut InstallGitDiffPagerContext,
    ) -> io::Result<InstallGitDiffPagerReport> {
        let plan = InstallPlan::new(&context.environment, &self.executable)?;

        if let Some(backup) = &plan.backup {
            BackupGlobalGitConfig { backup }.run(context)?;
        }
        SetGlobalGitConfig {
            key: "pager.diff",
            value: &plan.pager_command,
        }
        .run(context)?;

        Ok(plan.report())
    }
}

/// Every path and command `--install` needs, validated before anything is written.
struct InstallPlan {
    backup: Option<Backup>,
    pager_command: String,
}

struct Backup {
    source: PathBuf,
    destination: PathBuf,
    copy_command: String,
}

impl InstallPlan {
    fn new(environment: &GitConfigEnvironment, executable: &Path) -> io::Result<Self> {
        let config_path = global_config_path(environment)?;
        let pager_command = shell_quote(utf8(executable, "hdiff executable path")?);

        let backup = match fs::exists(&config_path)? {
            true => Some(Backup::of(config_path)?),
            false => None,
        };

        Ok(Self {
            backup,
            pager_command,
        })
    }

    fn report(self) -> InstallGitDiffPagerReport {
        let copy_command = self.backup.map(|backup| backup.copy_command);
        let config_command = format!("git config --global pager.diff {}", self.pager_command);

        InstallGitDiffPagerReport {
            completed_commands: copy_command.into_iter().chain([config_command]).collect(),
        }
    }
}

impl Backup {
    fn of(source: PathBuf) -> io::Result<Self> {
        let destination = backup_path(&source);
        let copy_command = format!(
            "cp {} {}",
            shell_quote(utf8(&source, "Git config path")?),
            shell_quote(utf8(&destination, "Git config backup path")?)
        );

        Ok(Self {
            source,
            destination,
            copy_command,
        })
    }
}

struct BackupGlobalGitConfig<'a> {
    backup: &'a Backup,
}

impl BackupGlobalGitConfig<'_> {
    fn run(self, _context: &mut InstallGitDiffPagerContext) -> io::Result<()> {
        fs::copy(&self.backup.source, &self.backup.destination).map(|_| ())
    }
}

struct SetGlobalGitConfig<'a> {
    key: &'a str,
    value: &'a str,
}

impl SetGlobalGitConfig<'_> {
    fn run(self, context: &mut InstallGitDiffPagerContext) -> io::Result<()> {
        let environment = &context.environment;
        let mut command = Command::new("git");
        command.args(["config", "--global", self.key, self.value]);
        set_or_remove(
            &mut command,
            "GIT_CONFIG_GLOBAL",
            &environment.git_config_global,
        );
        set_or_remove(&mut command, "HOME", &environment.home);
        set_or_remove(
            &mut command,
            "XDG_CONFIG_HOME",
            &environment.xdg_config_home,
        );

        let output = command.output()?;

        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(String::from_utf8_lossy(&output.stderr)))
    }
}

/// Gives Git exactly the environment the config path was resolved from.
fn set_or_remove(command: &mut Command, key: &str, value: &Option<OsString>) {
    match value {
        Some(value) => command.env(key, value),
        None => command.env_remove(key),
    };
}

fn global_config_path(environment: &GitConfigEnvironment) -> io::Result<PathBuf> {
    if let Some(path) = &environment.git_config_global {
        return configured_global_path(path);
    }

    let home = environment
        .home
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME is required to locate the global Git config"))?;
    let home_config = home.join(".gitconfig");
    if fs::exists(&home_config)? {
        return Ok(home_config);
    }

    let xdg_config_home = match &environment.xdg_config_home {
        Some(path) => PathBuf::from(path),
        None => home.join(".config"),
    };
    let xdg_config = xdg_config_home.join("git/config");
    if fs::exists(&xdg_config)? {
        return Ok(xdg_config);
    }

    Ok(home_config)
}

fn configured_global_path(path: &OsStr) -> io::Result<PathBuf> {
    let path = PathBuf::from(path);
    if path == Path::new("/dev/null") {
        return Err(io::Error::other(
            "GIT_CONFIG_GLOBAL=/dev/null disables global Git configuration",
        ));
    }

    Ok(path)
}

/// The sibling backup file: the config's own name with `.bak` appended.
fn backup_path(config_path: &Path) -> PathBuf {
    let mut name = config_path.as_os_str().to_owned();
    name.push(".bak");

    PathBuf::from(name)
}

fn utf8<'a>(path: &'a Path, description: &str) -> io::Result<&'a str> {
    path.to_str()
        .ok_or_else(|| io::Error::other(format!("{description} is not valid UTF-8")))
}

fn shell_quote(value: &str) -> String {
    let escaped_value = value.replace('\'', "'\"'\"'");

    format!("'{escaped_value}'")
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, fs, os::unix::ffi::OsStrExt, path::Path};

    use super::{GitConfigEnvironment, InstallPlan, backup_path};

    fn environment_for(config_path: &Path) -> GitConfigEnvironment {
        GitConfigEnvironment {
            git_config_global: Some(config_path.as_os_str().to_owned()),
            home: None,
            xdg_config_home: None,
        }
    }

    #[test]
    fn rejects_a_non_utf8_executable_before_planning_any_write() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let config_path = directory.path().join("gitconfig");
        fs::write(&config_path, "").expect("write global config");
        let executable = Path::new(OsStr::from_bytes(b"/bin/hdiff-\xff"));

        let plan = InstallPlan::new(&environment_for(&config_path), executable);

        assert!(
            plan.is_err(),
            "a non-UTF-8 executable cannot form a pager command"
        );
    }

    #[test]
    fn plans_a_backup_only_for_an_existing_config() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let config_path = directory.path().join("gitconfig");
        let executable = Path::new("/bin/hdiff");

        let absent = InstallPlan::new(&environment_for(&config_path), executable)
            .expect("plan without a config");
        fs::write(&config_path, "").expect("write global config");
        let present = InstallPlan::new(&environment_for(&config_path), executable)
            .expect("plan with a config");

        assert!(absent.backup.is_none());
        assert!(present.backup.is_some());
    }

    #[test]
    fn backs_up_to_the_config_name_with_bak_appended() {
        assert_eq!(
            backup_path(Path::new("/home/user/my.gitconfig")),
            Path::new("/home/user/my.gitconfig.bak")
        );
    }
}
