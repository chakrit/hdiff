use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use super::{
    Error,
    backup_global_git_config::BackupGlobalGitConfig,
    global_git_config::{GitConfigEnvironment, GlobalGitConfigContext},
    set_global_diff_pager::SetGlobalDiffPager,
};
use crate::{shell_words, utf8_path::Utf8Path};

/// Registers `executable` as the global Git diff pager, backing up the global config
/// first when it exists.
pub struct InstallGitDiffPager {
    pub executable: PathBuf,
}

/// The shell commands `--install` applied, as it prints them.
pub struct InstallGitDiffPagerReport {
    pub completed_commands: Vec<String>,
}

impl InstallGitDiffPager {
    /// Plans the whole install before changing anything, then backs up the global config
    /// when it exists and sets the pager, reporting each command once it has applied.
    pub fn run(
        self,
        context: &mut GlobalGitConfigContext,
    ) -> Result<InstallGitDiffPagerReport, Error> {
        let InstallPlan {
            backup,
            pager_command,
        } = InstallPlan::new(&context.environment, &self.executable)?;
        let mut completed_commands = Vec::new();

        if let Some(backup) = backup {
            let backup_command = backup.shell_command();
            backup.run(context)?;
            completed_commands.push(backup_command);
        }

        let set_pager = SetGlobalDiffPager {
            command: &pager_command,
        };
        let config_command = set_pager.shell_command();
        set_pager.run(context)?;
        completed_commands.push(config_command);

        Ok(InstallGitDiffPagerReport { completed_commands })
    }
}

/// Which path `--install` could not spell as UTF-8.
#[derive(Clone, Copy, Debug)]
pub enum PathRole {
    Executable,
    Config,
    Backup,
}

impl PathRole {
    /// The path's name in error messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Executable => "hdiff executable path",
            Self::Config => "Git config path",
            Self::Backup => "Git config backup path",
        }
    }
}

/// Every path and value `--install` needs, validated as UTF-8 before anything is written.
#[derive(Debug)]
struct InstallPlan {
    backup: Option<BackupGlobalGitConfig>,
    /// The `pager.diff` value: the executable as one shell word.
    pager_command: String,
}

impl InstallPlan {
    /// The plan for `environment` and `executable`; any path that is not UTF-8 fails it.
    fn new(environment: &GitConfigEnvironment, executable: &Path) -> Result<Self, Error> {
        let config = GlobalConfig::of(environment)?;
        let executable = utf8(executable, PathRole::Executable)?;
        let pager_command = shell_words::quote(executable.as_str()).into_owned();

        let backup = match config {
            GlobalConfig::Existing(path) => Some(backup_of(&path)?),
            GlobalConfig::Absent => None,
        };

        Ok(Self {
            backup,
            pager_command,
        })
    }
}

/// The copy of `config` to its backup path, both spelled as UTF-8.
fn backup_of(config: &Path) -> Result<BackupGlobalGitConfig, Error> {
    let copy = backup_path(config);

    Ok(BackupGlobalGitConfig {
        config: utf8(config, PathRole::Config)?,
        copy: utf8(&copy, PathRole::Backup)?,
    })
}

/// The file `git config --global` writes, as resolution found it on disk.
enum GlobalConfig {
    /// The config file exists at this path, so `--install` backs it up.
    Existing(PathBuf),
    /// No config file exists yet; Git creates it, so there is nothing to back up.
    Absent,
}

impl GlobalConfig {
    /// Resolves the file `git config --global` writes, in the order of git-config(1)
    /// FILES, probing each candidate's existence once.
    fn of(environment: &GitConfigEnvironment) -> Result<Self, Error> {
        if let Some(path) = &environment.git_config_global {
            return Self::probed(configured_global_path(path)?);
        }

        let home = environment
            .home
            .as_ref()
            .map(PathBuf::from)
            .ok_or(Error::MissingHome)?;
        let home_config = home.join(".gitconfig");
        if exists(&home_config)? {
            return Ok(Self::Existing(home_config));
        }

        let xdg_config_home = match &environment.xdg_config_home {
            Some(path) => PathBuf::from(path),
            None => home.join(".config"),
        };
        Self::probed(xdg_config_home.join("git/config"))
    }

    /// `path` as existing or absent.
    fn probed(path: PathBuf) -> Result<Self, Error> {
        Ok(match exists(&path)? {
            true => Self::Existing(path),
            false => Self::Absent,
        })
    }
}

/// The config file `GIT_CONFIG_GLOBAL` names; `/dev/null` disables the global config.
fn configured_global_path(path: &OsStr) -> Result<PathBuf, Error> {
    let path = PathBuf::from(path);
    if path == Path::new("/dev/null") {
        return Err(Error::GlobalConfigDisabled);
    }

    Ok(path)
}

/// Whether `path` exists; failing to tell is an error, not an absence.
fn exists(path: &Path) -> Result<bool, Error> {
    fs::exists(path).map_err(|source| Error::InspectConfig {
        path: path.to_owned(),
        source,
    })
}

/// The sibling backup file: the config's own name with `.bak` appended.
fn backup_path(config_path: &Path) -> PathBuf {
    let mut name = config_path.as_os_str().to_owned();
    name.push(".bak");

    PathBuf::from(name)
}

/// `path` as UTF-8, or an error naming its `role`.
fn utf8(path: &Path, role: PathRole) -> Result<Utf8Path, Error> {
    Utf8Path::of(path).ok_or(Error::NonUtf8Path(role))
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
            "a non-UTF-8 executable cannot form a pager command: {plan:?}"
        );
    }

    #[test]
    fn rejects_a_disabled_global_config() {
        let plan = InstallPlan::new(
            &environment_for(Path::new("/dev/null")),
            Path::new("/bin/hdiff"),
        );

        assert!(
            matches!(plan, Err(Error::GlobalConfigDisabled)),
            "/dev/null disables the global config: {plan:?}"
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

        assert!(
            absent.backup.is_none(),
            "no backup without a config: {absent:?}"
        );
        assert!(
            present.backup.is_some(),
            "a backup of an existing config: {present:?}"
        );
    }

    #[test]
    fn backs_up_to_the_config_name_with_bak_appended() {
        assert_eq!(
            backup_path(Path::new("/home/user/my.gitconfig")),
            Path::new("/home/user/my.gitconfig.bak")
        );
    }
}
