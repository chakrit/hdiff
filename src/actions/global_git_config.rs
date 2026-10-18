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
    pub fn of_process() -> Self {
        Self {
            git_config_global: env::var_os(GIT_CONFIG_GLOBAL),
            home: env::var_os(HOME),
            xdg_config_home: env::var_os(XDG_CONFIG_HOME),
        }
    }

    /// Every variable by name, so a child process can receive exactly this environment.
    pub fn variables(&self) -> [(&'static str, Option<&OsStr>); 3] {
        [
            (GIT_CONFIG_GLOBAL, self.git_config_global.as_deref()),
            (HOME, self.home.as_deref()),
            (XDG_CONFIG_HOME, self.xdg_config_home.as_deref()),
        ]
    }
}
