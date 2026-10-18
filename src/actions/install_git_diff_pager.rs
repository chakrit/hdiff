use std::{
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use super::{
    backup_global_git_config::BackupGlobalGitConfig,
    global_git_config::{GitConfigEnvironment, GlobalGitConfigContext},
    set_global_diff_pager::{self, SetGlobalDiffPager},
};
use crate::shell_words;

pub struct InstallGitDiffPager {
    pub executable: PathBuf,
}

pub struct InstallGitDiffPagerReport {
    pub completed_commands: Vec<String>,
}

#[derive(Debug)]
pub enum Error {
    MissingHome,
    GlobalConfigDisabled,
    NonUtf8Path(PathRole),
    InspectConfig(io::Error),
    Backup(io::Error),
    SetPager(set_global_diff_pager::Error),
}

/// Which path `--install` could not spell as a shell command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathRole {
    Executable,
    Config,
    Backup,
}

impl InstallGitDiffPager {
    pub fn run(
        self,
        context: &mut GlobalGitConfigContext,
    ) -> Result<InstallGitDiffPagerReport, Error> {
        let plan = InstallPlan::new(&context.environment, &self.executable)?;
        let mut completed_commands = Vec::new();

        if let Some(backup) = &plan.backup {
            let copy = BackupGlobalGitConfig {
                source: &backup.source,
                destination: &backup.destination,
            };
            let copy_command = copy.shell_command();
            copy.run(context).map_err(Error::Backup)?;
            completed_commands.push(copy_command);
        }

        let set_pager = SetGlobalDiffPager {
            command: &plan.pager_command,
        };
        let config_command = set_pager.shell_command();
        set_pager.run(context).map_err(Error::SetPager)?;
        completed_commands.push(config_command);

        Ok(InstallGitDiffPagerReport { completed_commands })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHome => {
                formatter.write_str("HOME is required to locate the global Git config")
            }
            Self::GlobalConfigDisabled => {
                formatter.write_str("GIT_CONFIG_GLOBAL=/dev/null disables global Git configuration")
            }
            Self::NonUtf8Path(role) => write!(formatter, "{} is not valid UTF-8", role.label()),
            Self::InspectConfig(error) => write!(formatter, "Git config: {error}"),
            Self::Backup(error) => write!(formatter, "Git config backup: {error}"),
            Self::SetPager(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl PathRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Executable => "hdiff executable path",
            Self::Config => "Git config path",
            Self::Backup => "Git config backup path",
        }
    }
}

/// Every path and value `--install` needs, validated as UTF-8 before anything is written.
struct InstallPlan {
    backup: Option<Backup>,
    /// The `pager.diff` value: the executable as one shell word.
    pager_command: String,
}

struct Backup {
    source: String,
    destination: String,
}

impl InstallPlan {
    fn new(environment: &GitConfigEnvironment, executable: &Path) -> Result<Self, Error> {
        let config_path = global_config_path(environment)?;
        let pager_command =
            shell_words::quote(utf8(executable, PathRole::Executable)?).into_owned();

        let backup = match exists(&config_path)? {
            true => Some(Backup::of(config_path)?),
            false => None,
        };

        Ok(Self {
            backup,
            pager_command,
        })
    }
}

impl Backup {
    fn of(source: PathBuf) -> Result<Self, Error> {
        let destination = backup_path(&source);

        Ok(Self {
            source: utf8(&source, PathRole::Config)?.to_owned(),
            destination: utf8(&destination, PathRole::Backup)?.to_owned(),
        })
    }
}

/// Resolves the file `git config --global` writes, in the order of git-config(1) FILES.
fn global_config_path(environment: &GitConfigEnvironment) -> Result<PathBuf, Error> {
    if let Some(path) = &environment.git_config_global {
        return configured_global_path(path);
    }

    let home = environment
        .home
        .as_ref()
        .map(PathBuf::from)
        .ok_or(Error::MissingHome)?;
    let home_config = home.join(".gitconfig");
    if exists(&home_config)? {
        return Ok(home_config);
    }

    let xdg_config_home = match &environment.xdg_config_home {
        Some(path) => PathBuf::from(path),
        None => home.join(".config"),
    };
    let xdg_config = xdg_config_home.join("git/config");
    if exists(&xdg_config)? {
        return Ok(xdg_config);
    }

    Ok(home_config)
}

fn configured_global_path(path: &OsStr) -> Result<PathBuf, Error> {
    let path = PathBuf::from(path);
    if path == Path::new("/dev/null") {
        return Err(Error::GlobalConfigDisabled);
    }

    Ok(path)
}

fn exists(path: &Path) -> Result<bool, Error> {
    fs::exists(path).map_err(Error::InspectConfig)
}

/// The sibling backup file: the config's own name with `.bak` appended.
fn backup_path(config_path: &Path) -> PathBuf {
    let mut name = config_path.as_os_str().to_owned();
    name.push(".bak");

    PathBuf::from(name)
}

fn utf8(path: &Path, role: PathRole) -> Result<&str, Error> {
    path.to_str().ok_or(Error::NonUtf8Path(role))
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, fs, os::unix::ffi::OsStrExt, path::Path};

    use super::{Error, InstallPlan, PathRole, backup_path};
    use crate::actions::global_git_config::GitConfigEnvironment;

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
            matches!(plan, Err(Error::NonUtf8Path(PathRole::Executable))),
            "a non-UTF-8 executable cannot form a pager command"
        );
    }

    #[test]
    fn rejects_a_disabled_global_config() {
        let plan = InstallPlan::new(
            &environment_for(Path::new("/dev/null")),
            Path::new("/bin/hdiff"),
        );

        assert!(matches!(plan, Err(Error::GlobalConfigDisabled)));
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
