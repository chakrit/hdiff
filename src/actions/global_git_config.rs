use std::{
    env,
    ffi::{OsStr, OsString},
    path::PathBuf,
};

/// The context every unit that reads or writes the global Git config runs in.
pub struct GlobalGitConfigContext {
    pub environment: GitConfigEnvironment,
}

/// The environment variables that decide which file `git config --global` writes, kept
/// as received so a child process sees them unchanged. Paths are read from them only
/// through the methods below, which follow Git's own reading.
pub struct GitConfigEnvironment {
    git_config_global: Option<OsString>,
    home: Option<OsString>,
    xdg_config_home: Option<OsString>,
}

/// The values of the variables that decide the global Git config, each by name so no two
/// can be passed in each other's place; `None` is unset.
pub struct GitConfigVariables {
    pub git_config_global: Option<OsString>,
    pub home: Option<OsString>,
    pub xdg_config_home: Option<OsString>,
}

const GIT_CONFIG_GLOBAL: &str = "GIT_CONFIG_GLOBAL";
const HOME: &str = "HOME";
const XDG_CONFIG_HOME: &str = "XDG_CONFIG_HOME";

impl GitConfigEnvironment {
    /// The variables as this process received them.
    pub fn of_process() -> Self {
        Self::of(GitConfigVariables {
            git_config_global: env::var_os(GIT_CONFIG_GLOBAL),
            home: env::var_os(HOME),
            xdg_config_home: env::var_os(XDG_CONFIG_HOME),
        })
    }

    /// The environment holding `variables`.
    pub fn of(variables: GitConfigVariables) -> Self {
        let GitConfigVariables {
            git_config_global,
            home,
            xdg_config_home,
        } = variables;

        Self {
            git_config_global,
            home,
            xdg_config_home,
        }
    }

    /// `GIT_CONFIG_GLOBAL`, which names the global config file outright when set.
    pub fn global_config_override(&self) -> Option<&OsStr> {
        self.git_config_global.as_deref()
    }

    /// `$HOME/<relative>`, joined as Git joins it, so an empty `HOME` names a path under
    /// the root; none when `HOME` is unset.
    pub fn under_home(&self, relative: &str) -> Option<PathBuf> {
        self.home.as_deref().map(|home| joined(home, relative))
    }

    /// `relative` under the XDG config directory as Git resolves it: `XDG_CONFIG_HOME`
    /// when it is set and not empty, otherwise `$HOME/.config`; none when neither is set.
    pub fn under_xdg_config_home(&self, relative: &str) -> Option<PathBuf> {
        let xdg_config_home = self
            .xdg_config_home
            .as_deref()
            .filter(|directory| !directory.is_empty());

        match xdg_config_home {
            Some(directory) => Some(joined(directory, relative)),
            None => self.under_home(&format!(".config/{relative}")),
        }
    }

    /// Every variable by name, so a child process can receive exactly this environment.
    pub fn variables(&self) -> [EnvironmentVariable<'_>; 3] {
        [
            EnvironmentVariable {
                name: GIT_CONFIG_GLOBAL,
                value: self.git_config_global.as_deref(),
            },
            EnvironmentVariable {
                name: HOME,
                value: self.home.as_deref(),
            },
            EnvironmentVariable {
                name: XDG_CONFIG_HOME,
                value: self.xdg_config_home.as_deref(),
            },
        ]
    }
}

/// One environment variable by name, with its value when it is set.
pub struct EnvironmentVariable<'a> {
    pub name: &'static str,
    pub value: Option<&'a OsStr>,
}

/// `<directory>/<relative>` by text, the way Git builds these paths.
fn joined(directory: &OsStr, relative: &str) -> PathBuf {
    let mut path = directory.to_owned();
    path.push("/");
    path.push(relative);

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{GitConfigEnvironment, GitConfigVariables};

    fn with_home(home: &str, xdg_config_home: Option<&str>) -> GitConfigEnvironment {
        GitConfigEnvironment::of(GitConfigVariables {
            git_config_global: None,
            home: Some(home.into()),
            xdg_config_home: xdg_config_home.map(Into::into),
        })
    }

    #[test]
    fn reads_an_empty_xdg_config_home_as_unset() {
        let paths = [None, Some("")]
            .map(|xdg| with_home("/home/user", xdg).under_xdg_config_home("git/config"));

        for path in paths {
            assert_eq!(
                path.as_deref(),
                Some(Path::new("/home/user/.config/git/config"))
            );
        }
    }

    #[test]
    fn joins_an_empty_home_under_the_root_as_git_does() {
        let path = with_home("", None).under_home(".gitconfig");

        assert_eq!(path.as_deref(), Some(Path::new("/.gitconfig")));
    }
}
