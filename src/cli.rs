//! Command-line selection. Contract: docs/spec/architecture.md#current-argument-selection.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

use crate::{
    actions::compare_files::{Comparison, Depth, Operand},
    bench::Driver,
    input::{Source, StdinState},
};

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
pub enum Invocation {
    Help,
    Install,
    Version,
    Bench { driver: Driver, source: Source },
    View { source: Source },
}

#[derive(Debug, PartialEq, Eq)]
pub enum UsageError {
    InstallWithOperands,
    MissingBenchDriver,
    UnknownBenchDriver(OsString),
    PipedStdinIgnored,
    RecursionWithoutComparison,
    TooManyOperands,
}

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InstallWithOperands => {
                formatter.write_str("--install cannot be combined with diff input operands")
            }
            Self::MissingBenchDriver => formatter.write_str("--bench requires a driver"),
            Self::UnknownBenchDriver(name) => write!(
                formatter,
                "unknown --bench driver {}; expected {}",
                name.to_string_lossy(),
                Driver::choices()
            ),
            Self::PipedStdinIgnored => formatter.write_str(
                "piped standard input cannot be combined with file operands; name it with -",
            ),
            Self::RecursionWithoutComparison => {
                formatter.write_str("-r requires two comparison operands")
            }
            Self::TooManyOperands => formatter.write_str("expected zero, one, or two operands"),
        }
    }
}

impl std::error::Error for UsageError {}

pub fn select_invocation(
    arguments: &[OsString],
    stdin: StdinState,
) -> Result<Invocation, UsageError> {
    if arguments.iter().any(|argument| argument == "--install") {
        return match arguments {
            [_] => Ok(Invocation::Install),
            _ => Err(UsageError::InstallWithOperands),
        };
    }

    match arguments {
        [argument] if argument == "--help" => Ok(Invocation::Help),
        [argument] if argument == "--version" => Ok(Invocation::Version),
        [flag, rest @ ..] if flag == "--bench" => select_bench(rest, stdin),
        operands => select_input(operands, stdin).map(|source| Invocation::View { source }),
    }
}

fn select_bench(arguments: &[OsString], stdin: StdinState) -> Result<Invocation, UsageError> {
    let Some((name, operands)) = arguments.split_first() else {
        return Err(UsageError::MissingBenchDriver);
    };
    let Some(driver) = Driver::from_name(name) else {
        return Err(UsageError::UnknownBenchDriver(name.clone()));
    };

    select_input(operands, stdin).map(|source| Invocation::Bench { driver, source })
}

fn select_input(arguments: &[OsString], stdin: StdinState) -> Result<Source, UsageError> {
    let source = select_source(arguments)?;

    match (stdin, source.reads_stdin()) {
        (StdinState::Piped, false) => Err(UsageError::PipedStdinIgnored),
        _ => Ok(source),
    }
}

fn select_source(arguments: &[OsString]) -> Result<Source, UsageError> {
    let (recursion_flags, operand_arguments): (Vec<_>, Vec<_>) =
        arguments.iter().partition(|argument| *argument == "-r");
    let depth = match recursion_flags.is_empty() {
        true => Depth::TopLevel,
        false => Depth::Recursive,
    };
    let operands = operand_arguments
        .into_iter()
        .map(|argument| operand(argument))
        .collect::<Vec<_>>();
    let operands = match <[Operand; 2]>::try_from(operands) {
        Ok(operands) => return Ok(Source::Comparison(Comparison { operands, depth })),
        Err(operands) => operands,
    };

    // Any number but two operands remains, so a second one means more than two.
    let mut operands = operands.into_iter();
    match (operands.next(), operands.next(), depth) {
        (_, Some(_), _) => Err(UsageError::TooManyOperands),
        (_, None, Depth::Recursive) => Err(UsageError::RecursionWithoutComparison),
        (None | Some(Operand::Stdin), None, Depth::TopLevel) => Ok(Source::Stdin),
        (Some(Operand::Path(path)), None, Depth::TopLevel) => Ok(Source::PatchFile(path)),
    }
}

/// An operand argument, where `-` names standard input and other dash-leading arguments
/// are paths.
fn operand(argument: &OsStr) -> Operand {
    match argument == "-" {
        true => Operand::Stdin,
        false => Operand::Path(PathBuf::from(argument)),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Comparison, Depth, Driver, HELP, Invocation, Operand, Source, StdinState, UsageError,
        select_input, select_invocation,
    };
    use std::{ffi::OsString, path::PathBuf};

    fn invocation_unpiped(arguments: &[OsString]) -> Result<Invocation, UsageError> {
        select_invocation(arguments, StdinState::Unpiped)
    }

    fn select_unpiped(arguments: &[OsString]) -> Result<Source, UsageError> {
        select_input(arguments, StdinState::Unpiped)
    }

    fn arguments(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn piped_stdin_is_accepted_only_where_an_operand_reads_it() {
        let reading: [&[&str]; 4] = [&[], &["-"], &["-", "after"], &["before", "-"]];
        for words in reading {
            let arguments = arguments(words);
            assert!(
                select_input(&arguments, StdinState::Piped).is_ok(),
                "{arguments:?}"
            );
        }

        let ignoring: [&[&str]; 2] = [&["change.patch"], &["before", "after"]];
        for words in ignoring {
            let arguments = arguments(words);
            assert_eq!(
                select_input(&arguments, StdinState::Piped),
                Err(UsageError::PipedStdinIgnored),
                "{arguments:?}"
            );
        }
    }

    #[test]
    fn piped_stdin_rejects_bench_operands_that_ignore_it() {
        let arguments = ["--bench", "prepare", "change.patch"].map(OsString::from);

        assert_eq!(
            select_invocation(&arguments, StdinState::Piped),
            Err(UsageError::PipedStdinIgnored)
        );
    }

    #[test]
    fn selects_install_without_input_operands() {
        assert_eq!(
            invocation_unpiped(&[OsString::from("--install")]).expect("install invocation"),
            Invocation::Install
        );
    }

    #[test]
    fn rejects_install_with_input_operands() {
        let arguments = [OsString::from("--install"), OsString::from("change.patch")];

        assert_eq!(
            invocation_unpiped(&arguments),
            Err(UsageError::InstallWithOperands)
        );
    }

    #[test]
    fn selects_bench_driver_before_input_operands() {
        let arguments = [OsString::from("--bench"), OsString::from("stages")];
        assert_eq!(
            invocation_unpiped(&arguments).expect("bench invocation"),
            Invocation::Bench {
                driver: Driver::Stages,
                source: Source::Stdin,
            }
        );

        let arguments = ["--bench", "movement", "change.patch"].map(OsString::from);
        assert_eq!(
            invocation_unpiped(&arguments).expect("bench invocation"),
            Invocation::Bench {
                driver: Driver::Movement,
                source: Source::PatchFile(PathBuf::from("change.patch")),
            }
        );
    }

    #[test]
    fn rejects_bench_without_a_known_driver() {
        assert_eq!(
            invocation_unpiped(&[OsString::from("--bench")]),
            Err(UsageError::MissingBenchDriver)
        );
        assert_eq!(
            invocation_unpiped(&["--bench", "a.patch"].map(OsString::from)),
            Err(UsageError::UnknownBenchDriver(OsString::from("a.patch")))
        );
    }

    #[test]
    fn help_names_every_bench_driver() {
        for driver in Driver::ALL {
            assert!(HELP.contains(driver.label()), "{}", driver.label());
        }
    }

    #[test]
    fn treats_unknown_dash_arguments_as_paths() {
        assert_eq!(
            invocation_unpiped(&[OsString::from("--unknown")]).expect("view invocation"),
            Invocation::View {
                source: Source::PatchFile(PathBuf::from("--unknown")),
            }
        );
    }

    #[test]
    fn selects_stdin_without_operands() {
        assert_eq!(select_unpiped(&[]).expect("stdin"), Source::Stdin);
    }

    #[test]
    fn rejects_more_than_two_operands() {
        assert_eq!(
            select_unpiped(&["a", "b", "c"].map(OsString::from)),
            Err(UsageError::TooManyOperands)
        );
    }

    #[test]
    fn reads_a_lone_dash_operand_from_stdin() {
        assert_eq!(
            select_unpiped(&[OsString::from("-")]).expect("stdin"),
            Source::Stdin
        );
    }

    #[test]
    fn compares_a_dash_operand_as_stdin() {
        assert_eq!(
            select_unpiped(&["-", "after"].map(OsString::from)).expect("comparison"),
            Source::Comparison(Comparison {
                operands: [Operand::Stdin, Operand::Path(PathBuf::from("after"))],
                depth: Depth::TopLevel,
            })
        );
    }

    #[test]
    fn selects_recursive_comparison_wherever_the_flag_appears() {
        let recursive = Source::Comparison(Comparison {
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
        assert_eq!(
            select_unpiped(&[OsString::from("-r")]),
            Err(UsageError::RecursionWithoutComparison)
        );
        assert_eq!(
            select_unpiped(&["-r", "change.patch"].map(OsString::from)),
            Err(UsageError::RecursionWithoutComparison)
        );
    }
}
