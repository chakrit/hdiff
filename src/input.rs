use std::{ffi::OsString, path::PathBuf};

use crate::{actions, bench::Driver};

pub const HELP: &str = concat!(
    "Usage: hdiff [OPTIONS] [FILE ...]\n",
    "\n",
    "Arguments:\n",
    "  [FILE ...]  Read one patch file, compare two files, or read standard input\n",
    "              when omitted\n",
    "\n",
    "Options:\n",
    "  --bench <DRIVER>  Prepare the diff and report measurements from DRIVER:\n",
    "                    prepare, stages, or movement\n",
    "  --install         Register hdiff as the user-level Git diff pager\n",
    "  --help            Print help\n",
    "  --version         Print version\n",
);

pub const VERSION: &str = concat!(
    "hdiff ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("HDIFF_GIT_HASH"),
    ")\n"
);

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Install,
    Version,
    Bench { driver: Driver, source: InputSource },
    View { source: InputSource },
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    DiffFile(PathBuf),
    Operands([PathBuf; 2]),
}

impl InputSource {
    pub fn read(self) -> Result<Vec<u8>, String> {
        match self {
            InputSource::Stdin => {
                let mut input = Vec::new();
                std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
                    .map_err(|error| error.to_string())?;
                Ok(input)
            }
            InputSource::DiffFile(path) => std::fs::read(path).map_err(|error| error.to_string()),
            InputSource::Operands([before, after]) => {
                let mut context = actions::compare_files::CompareFilesContext;

                actions::compare_files::CompareFiles {
                    before: &before,
                    after: &after,
                }
                .run(&mut context)
                .map_err(|error| error.to_string())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InputError(pub String);

pub fn select_command(arguments: &[OsString]) -> Result<Command, InputError> {
    if arguments.iter().any(|argument| argument == "--install") {
        return match arguments {
            [_] => Ok(Command::Install),
            _ => Err(InputError(
                "--install cannot be combined with diff input operands".to_owned(),
            )),
        };
    }

    match arguments {
        [argument] if argument == "--help" => Ok(Command::Help),
        [argument] if argument == "--version" => Ok(Command::Version),
        [flag, rest @ ..] if flag == "--bench" => select_bench(rest),
        operands => select_input(&paths(operands)).map(|source| Command::View { source }),
    }
}

fn select_bench(arguments: &[OsString]) -> Result<Command, InputError> {
    let Some((name, operands)) = arguments.split_first() else {
        return Err(InputError("--bench requires a driver".to_owned()));
    };
    let Some(driver) = Driver::from_name(name) else {
        return Err(InputError(format!(
            "unknown --bench driver {}; expected prepare, stages, or movement",
            name.to_string_lossy()
        )));
    };

    select_input(&paths(operands)).map(|source| Command::Bench { driver, source })
}

fn paths(arguments: &[OsString]) -> Vec<PathBuf> {
    arguments.iter().map(PathBuf::from).collect()
}

pub fn select_input(arguments: &[PathBuf]) -> Result<InputSource, InputError> {
    match arguments {
        [] => Ok(InputSource::Stdin),
        [path] => Ok(InputSource::DiffFile(path.clone())),
        [first, second] => Ok(InputSource::Operands([first.clone(), second.clone()])),
        _ => Err(InputError("expected zero, one, or two operands".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, Driver, InputSource, select_command, select_input};
    use std::{ffi::OsString, path::PathBuf};

    #[test]
    fn selects_install_without_input_operands() {
        assert_eq!(
            select_command(&[OsString::from("--install")]).expect("install command"),
            Command::Install
        );
    }

    #[test]
    fn rejects_install_with_input_operands() {
        let arguments = [OsString::from("--install"), OsString::from("change.patch")];

        assert!(select_command(&arguments).is_err());
    }

    #[test]
    fn selects_bench_driver_before_input_operands() {
        let arguments = [OsString::from("--bench"), OsString::from("stages")];
        assert_eq!(
            select_command(&arguments).expect("bench command"),
            Command::Bench {
                driver: Driver::Stages,
                source: InputSource::Stdin,
            }
        );

        let arguments = ["--bench", "movement", "change.patch"].map(OsString::from);
        assert_eq!(
            select_command(&arguments).expect("bench command"),
            Command::Bench {
                driver: Driver::Movement,
                source: InputSource::DiffFile(PathBuf::from("change.patch")),
            }
        );
    }

    #[test]
    fn rejects_bench_without_a_known_driver() {
        assert!(select_command(&[OsString::from("--bench")]).is_err());
        assert!(select_command(&["--bench", "a.patch"].map(OsString::from)).is_err());
    }

    #[test]
    fn treats_profile_as_an_operand() {
        assert_eq!(
            select_command(&[OsString::from("--profile")]).expect("view command"),
            Command::View {
                source: InputSource::DiffFile(PathBuf::from("--profile")),
            }
        );
    }

    #[test]
    fn selects_stdin_without_operands() {
        assert_eq!(select_input(&[]).expect("stdin"), InputSource::Stdin);
    }

    #[test]
    fn rejects_more_than_two_operands() {
        let paths = vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")];
        assert!(select_input(&paths).is_err());
    }
}
