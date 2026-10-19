use std::{
    ffi::OsStr,
    fs::{self, Metadata},
    io,
    os::unix::fs::MetadataExt,
    path::{self, Component, Path, PathBuf},
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
        } = InstallPlan::of(&context.environment, &self.executable)?;
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
    /// The plan for `environment` and `executable`, which registers the executable's
    /// absolute path without its `.` and `..` steps. Links stay as the user reached them,
    /// so the command still resolves after the directory `--install` was run from, or a
    /// versioned directory a package manager's link points into, is gone.
    fn of(environment: &GitConfigEnvironment, executable: &Path) -> Result<Self, Error> {
        let registered = registered_path(executable)?;

        Self::of_registered(environment, &registered)
    }

    /// The plan for `environment` and an `executable` already in its registered form;
    /// any path that is not UTF-8 fails it.
    fn of_registered(environment: &GitConfigEnvironment, executable: &Path) -> Result<Self, Error> {
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

/// `executable` as an absolute path with its `.` and `..` steps removed by spelling,
/// keeping every link. The result must still name the same file: a `..` after a linked
/// directory leaves the link's target, which spelling alone cannot follow.
fn registered_path(executable: &Path) -> Result<PathBuf, Error> {
    let resolve_failed = |source| Error::ResolveExecutable {
        path: executable.to_owned(),
        source,
    };
    let absolute = path::absolute(executable).map_err(resolve_failed)?;

    let registered = without_dot_segments(&absolute);

    let reached = fs::metadata(executable).map_err(resolve_failed)?;
    let reaches = names_file(&registered, &reached).map_err(|source| Error::ResolveExecutable {
        path: registered.clone(),
        source,
    })?;

    match reaches {
        true => Ok(registered),
        false => Err(Error::ExecutableNotReached {
            executable: executable.to_owned(),
            registered,
        }),
    }
}

/// Whether `path` names the file `reached` describes; a path that names nothing does
/// not.
fn names_file(path: &Path, reached: &Metadata) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(named) => Ok((named.dev(), named.ino()) == (reached.dev(), reached.ino())),
        Err(error) => match error.kind() {
            io::ErrorKind::NotFound => Ok(false),
            _ => Err(error),
        },
    }
}

/// `absolute` with each `.` step dropped and each `..` step removing the step before it;
/// `..` at the root stays at the root.
fn without_dot_segments(absolute: &Path) -> PathBuf {
    absolute
        .components()
        .fold(PathBuf::new(), |mut path, component| {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    path.pop();
                }
                Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                    path.push(component)
                }
            }
            path
        })
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
        if let Some(path) = environment.global_config_override() {
            return Self::probed(configured_global_path(path)?);
        }

        let home_config = environment
            .under_home(".gitconfig")
            .ok_or(Error::MissingHome)?;
        if exists(&home_config)? {
            return Ok(Self::Existing(home_config));
        }

        let xdg_config = environment
            .under_xdg_config_home("git/config")
            .ok_or(Error::MissingHome)?;
        Self::probed(xdg_config)
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
        GitConfigEnvironment::of(Some(config_path.as_os_str().to_owned()), None, None)
    }

    #[test]
    fn rejects_a_non_utf8_executable_before_planning_any_write() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let config_path = directory.path().join("gitconfig");
        fs::write(&config_path, "").expect("write global config");
        let executable = Path::new(OsStr::from_bytes(b"/bin/hdiff-\xff"));

        let plan = InstallPlan::of_registered(&environment_for(&config_path), executable);

        assert!(
            matches!(plan, Err(Error::NonUtf8Path(PathRole::Executable))),
            "a non-UTF-8 executable cannot form a pager command: {plan:?}"
        );
    }

    #[test]
    fn rejects_a_disabled_global_config() {
        let plan = InstallPlan::of_registered(
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

        let absent = InstallPlan::of_registered(&environment_for(&config_path), executable)
            .expect("plan without a config");
        fs::write(&config_path, "").expect("write global config");
        let present = InstallPlan::of_registered(&environment_for(&config_path), executable)
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

    /// Git reads an empty `XDG_CONFIG_HOME` as unset and writes
    /// `$HOME/.config/git/config`, so that is the file to back up.
    #[test]
    fn backs_up_the_home_xdg_config_when_xdg_config_home_is_empty() {
        let home = tempfile::tempdir().expect("temporary home");
        let xdg_config = home.path().join(".config/git/config");
        fs::create_dir_all(xdg_config.parent().expect("config directory"))
            .expect("create config directory");
        fs::write(&xdg_config, "").expect("write xdg config");
        let environment = GitConfigEnvironment::of(
            None,
            Some(home.path().as_os_str().to_owned()),
            Some("".into()),
        );

        let plan = InstallPlan::of_registered(&environment, Path::new("/bin/hdiff")).expect("plan");

        let backed_up = plan.backup.as_ref().map(|backup| backup.config.as_str());
        assert_eq!(backed_up, xdg_config.to_str(), "{plan:?}");
    }

    /// docs/spec/architecture.md#initial-cli-surface: the registered command names the
    /// executable as the user reached it, without the `.` and `..` steps of the
    /// directory `--install` was run from.
    #[test]
    fn registers_the_executable_without_dot_segments() {
        let directory = tempfile::tempdir().expect("temporary directory");
        fs::write(directory.path().join("hdiff"), "").expect("write executable");
        fs::create_dir(directory.path().join("run")).expect("create run directory");
        let through_run = directory.path().join("run/./../hdiff");
        let config_path = directory.path().join("gitconfig");

        let plan = InstallPlan::of(&environment_for(&config_path), &through_run)
            .expect("plan for an existing executable");

        let expected = format!("{}/hdiff", directory.path().display());
        assert_eq!(plan.pager_command, expected, "{plan:?}");
    }

    /// docs/spec/architecture.md#initial-cli-surface: a package manager's stable link
    /// stays registered, so the pager survives the versioned directory it points into
    /// being replaced.
    #[test]
    fn registers_a_symlinked_executable_by_its_link() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let versioned = directory.path().join("cellar/1.0/bin");
        fs::create_dir_all(&versioned).expect("create versioned directory");
        fs::write(versioned.join("hdiff"), "").expect("write executable");
        fs::create_dir(directory.path().join("bin")).expect("create link directory");
        std::os::unix::fs::symlink(
            "../cellar/1.0/bin/hdiff",
            directory.path().join("bin/hdiff"),
        )
        .expect("link executable");
        let config_path = directory.path().join("gitconfig");

        let plan = InstallPlan::of(
            &environment_for(&config_path),
            &directory.path().join("bin/../bin/hdiff"),
        )
        .expect("plan for a linked executable");

        let expected = format!("{}/bin/hdiff", directory.path().display());
        assert_eq!(plan.pager_command, expected, "{plan:?}");
    }

    /// A `..` after a linked directory leaves the link's target, so removing it by
    /// spelling alone would name a different file; that executable is not registered.
    #[test]
    fn rejects_an_executable_whose_dot_dot_leaves_a_linked_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("elsewhere/bin");
        fs::create_dir_all(&target).expect("create link target");
        fs::write(directory.path().join("elsewhere/hdiff"), "").expect("write executable");
        std::os::unix::fs::symlink(&target, directory.path().join("tools")).expect("link");
        let config_path = directory.path().join("gitconfig");

        let plan = InstallPlan::of(
            &environment_for(&config_path),
            &directory.path().join("tools/../hdiff"),
        );

        assert!(
            matches!(plan, Err(Error::ExecutableNotReached { .. })),
            "the spelled path names no executable: {plan:?}"
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
