use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io,
    os::{fd::AsFd, unix::fs::FileTypeExt},
    path::PathBuf,
};

use crate::{actions, bench::Driver};

pub const HELP: &str = concat!(
    "Usage: hdiff [OPTIONS] [FILE ...]\n",
    "\n",
    "Arguments:\n",
    "  [FILE ...]  Read one patch file, compare two files or directories, or read\n",
    "              standard input when omitted; - names standard input\n",
    "\n",
    "Options:\n",
    "  -r                Compare directories recursively\n",
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
    Comparison(Comparison),
}

/// Two operands for `diff -u`. Contract: docs/spec/architecture.md#input-contract.
#[derive(Debug, PartialEq, Eq)]
pub struct Comparison {
    pub operands: [Operand; 2],
    pub depth: Depth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Stdin,
    Path(PathBuf),
}

impl Operand {
    fn from_argument(argument: &OsStr) -> Self {
        match argument == "-" {
            true => Self::Stdin,
            false => Self::Path(PathBuf::from(argument)),
        }
    }

    /// The operand as `diff` reads it, where `-` names standard input.
    pub fn argument(&self) -> &OsStr {
        match self {
            Self::Stdin => OsStr::new("-"),
            Self::Path(path) => path.as_os_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    TopLevel,
    Recursive,
}

/// Whether standard input is a pipe or socket, as when another process feeds hdiff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinState {
    Piped,
    Unpiped,
}

impl StdinState {
    pub fn inspect() -> io::Result<Self> {
        let handle = io::stdin().as_fd().try_clone_to_owned()?;
        let file_type = File::from(handle).metadata()?.file_type();

        match file_type.is_fifo() || file_type.is_socket() {
            true => Ok(Self::Piped),
            false => Ok(Self::Unpiped),
        }
    }
}

impl InputSource {
    fn reads_stdin(&self) -> bool {
        match self {
            InputSource::Stdin => true,
            InputSource::DiffFile(_) => false,
            InputSource::Comparison(comparison) => comparison.operands.contains(&Operand::Stdin),
        }
    }

    pub fn read(self) -> Result<Vec<u8>, String> {
        match self {
            InputSource::Stdin => {
                let mut input = Vec::new();
                std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
                    .map_err(|error| error.to_string())?;
                Ok(input)
            }
            InputSource::DiffFile(path) => std::fs::read(path).map_err(|error| error.to_string()),
            InputSource::Comparison(comparison) => {
                let mut context = actions::compare_files::CompareFilesContext;

                actions::compare_files::CompareFiles {
                    comparison: &comparison,
                }
                .run(&mut context)
                .map_err(|error| error.to_string())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InputError(pub String);

pub fn select_command(arguments: &[OsString], stdin: StdinState) -> Result<Command, InputError> {
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
        [flag, rest @ ..] if flag == "--bench" => select_bench(rest, stdin),
        operands => select_input(operands, stdin).map(|source| Command::View { source }),
    }
}

fn select_bench(arguments: &[OsString], stdin: StdinState) -> Result<Command, InputError> {
    let Some((name, operands)) = arguments.split_first() else {
        return Err(InputError("--bench requires a driver".to_owned()));
    };
    let Some(driver) = Driver::from_name(name) else {
        return Err(InputError(format!(
            "unknown --bench driver {}; expected prepare, stages, or movement",
            name.to_string_lossy()
        )));
    };

    select_input(operands, stdin).map(|source| Command::Bench { driver, source })
}

fn select_input(arguments: &[OsString], stdin: StdinState) -> Result<InputSource, InputError> {
    let source = select_source(arguments)?;

    match (stdin, source.reads_stdin()) {
        (StdinState::Piped, false) => Err(InputError(
            "piped standard input cannot be combined with file operands; name it with -".to_owned(),
        )),
        _ => Ok(source),
    }
}

fn select_source(arguments: &[OsString]) -> Result<InputSource, InputError> {
    let (recursion_flags, operand_arguments): (Vec<_>, Vec<_>) =
        arguments.iter().partition(|argument| *argument == "-r");
    let depth = match recursion_flags.is_empty() {
        true => Depth::TopLevel,
        false => Depth::Recursive,
    };
    let operands = operand_arguments
        .into_iter()
        .map(|argument| Operand::from_argument(argument))
        .collect::<Vec<_>>();

    match (operands.as_slice(), depth) {
        ([] | [Operand::Stdin], Depth::TopLevel) => Ok(InputSource::Stdin),
        ([Operand::Path(path)], Depth::TopLevel) => Ok(InputSource::DiffFile(path.clone())),
        ([before, after], depth) => Ok(InputSource::Comparison(Comparison {
            operands: [before.clone(), after.clone()],
            depth,
        })),
        ([] | [_], Depth::Recursive) => {
            Err(InputError("-r requires two comparison operands".to_owned()))
        }
        _ => Err(InputError("expected zero, one, or two operands".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Command, Comparison, Depth, Driver, InputError, InputSource, Operand, StdinState,
        select_command, select_input,
    };
    use std::{ffi::OsString, path::PathBuf};

    fn command_unpiped(arguments: &[OsString]) -> Result<Command, InputError> {
        select_command(arguments, StdinState::Unpiped)
    }

    fn select_unpiped(arguments: &[OsString]) -> Result<InputSource, InputError> {
        select_input(arguments, StdinState::Unpiped)
    }

    #[test]
    fn piped_stdin_is_accepted_only_where_an_operand_reads_it() {
        let reading = [vec![], vec!["-"], vec!["-", "after"], vec!["before", "-"]];
        for arguments in reading {
            let arguments = arguments
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>();
            assert!(
                select_input(&arguments, StdinState::Piped).is_ok(),
                "{arguments:?}"
            );
        }

        let ignoring = [vec!["change.patch"], vec!["before", "after"]];
        for arguments in ignoring {
            let arguments = arguments
                .into_iter()
                .map(OsString::from)
                .collect::<Vec<_>>();
            assert!(
                select_input(&arguments, StdinState::Piped).is_err(),
                "{arguments:?}"
            );
        }
    }

    #[test]
    fn piped_stdin_rejects_bench_operands_that_ignore_it() {
        let arguments = ["--bench", "prepare", "change.patch"].map(OsString::from);

        assert!(select_command(&arguments, StdinState::Piped).is_err());
    }

    #[test]
    fn selects_install_without_input_operands() {
        assert_eq!(
            command_unpiped(&[OsString::from("--install")]).expect("install command"),
            Command::Install
        );
    }

    #[test]
    fn rejects_install_with_input_operands() {
        let arguments = [OsString::from("--install"), OsString::from("change.patch")];

        assert!(command_unpiped(&arguments).is_err());
    }

    #[test]
    fn selects_bench_driver_before_input_operands() {
        let arguments = [OsString::from("--bench"), OsString::from("stages")];
        assert_eq!(
            command_unpiped(&arguments).expect("bench command"),
            Command::Bench {
                driver: Driver::Stages,
                source: InputSource::Stdin,
            }
        );

        let arguments = ["--bench", "movement", "change.patch"].map(OsString::from);
        assert_eq!(
            command_unpiped(&arguments).expect("bench command"),
            Command::Bench {
                driver: Driver::Movement,
                source: InputSource::DiffFile(PathBuf::from("change.patch")),
            }
        );
    }

    #[test]
    fn rejects_bench_without_a_known_driver() {
        assert!(command_unpiped(&[OsString::from("--bench")]).is_err());
        assert!(command_unpiped(&["--bench", "a.patch"].map(OsString::from)).is_err());
    }

    #[test]
    fn treats_profile_as_an_operand() {
        assert_eq!(
            command_unpiped(&[OsString::from("--profile")]).expect("view command"),
            Command::View {
                source: InputSource::DiffFile(PathBuf::from("--profile")),
            }
        );
    }

    #[test]
    fn selects_stdin_without_operands() {
        assert_eq!(select_unpiped(&[]).expect("stdin"), InputSource::Stdin);
    }

    #[test]
    fn rejects_more_than_two_operands() {
        assert!(select_unpiped(&["a", "b", "c"].map(OsString::from)).is_err());
    }

    #[test]
    fn reads_a_lone_dash_operand_from_stdin() {
        assert_eq!(
            select_unpiped(&[OsString::from("-")]).expect("stdin"),
            InputSource::Stdin
        );
    }

    #[test]
    fn compares_a_dash_operand_as_stdin() {
        assert_eq!(
            select_unpiped(&["-", "after"].map(OsString::from)).expect("comparison"),
            InputSource::Comparison(Comparison {
                operands: [Operand::Stdin, Operand::Path(PathBuf::from("after"))],
                depth: Depth::TopLevel,
            })
        );
    }

    #[test]
    fn selects_recursive_comparison_wherever_the_flag_appears() {
        let recursive = InputSource::Comparison(Comparison {
            operands: [
                Operand::Path(PathBuf::from("a")),
                Operand::Path(PathBuf::from("b")),
            ],
            depth: Depth::Recursive,
        });

        for arguments in [["-r", "a", "b"], ["a", "-r", "b"], ["a", "b", "-r"]] {
            assert_eq!(
                select_unpiped(&arguments.map(OsString::from)).expect("comparison"),
                recursive
            );
        }
    }

    #[test]
    fn rejects_recursion_without_two_operands() {
        assert!(select_unpiped(&[OsString::from("-r")]).is_err());
        assert!(select_unpiped(&["-r", "change.patch"].map(OsString::from)).is_err());
    }
}
