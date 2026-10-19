use std::{
    env,
    ffi::{OsStr, OsString},
};

/// The context every unit that reads or writes the global Git config runs in.
pub struct GlobalGitConfigContext {
    pub environment: GitConfigEnvironment,
}

/// The environment variables that decide which file `git config --global` writes.
pub struct GitConfigEnvironment {
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
        Self {
            git_config_global: env::var_os(GIT_CONFIG_GLOBAL),
            home: env::var_os(HOME),
            xdg_config_home: env::var_os(XDG_CONFIG_HOME),
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
