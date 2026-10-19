//! Command-line selection. Rules: docs/spec/architecture.md#current-argument-selection.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

use crate::{
    bench::Driver,
    comparison::{Comparison, Depth, Operand},
    input::{Source, StdinState},
    side::BySide,
};

/// The `--help` text; its driver list comes from [`Driver::choices`].
pub fn help() -> String {
    format!(
        concat!(
            "Usage: hdiff [OPTIONS] [FILE ...]\n",
            "\n",
            "Arguments:\n",
            "  [FILE ...]  Read one patch file, compare two files or directories, or read\n",
            "              standard input when omitted; - names standard input\n",
            "\n",
            "Options:\n",
            "  -r                Compare directories recursively\n",
            "  --bench <DRIVER>  Prepare the diff and report measurements from DRIVER:\n",
            "                    {drivers}\n",
            "  --install         Register hdiff as the user-level Git diff pager\n",
            "  --help            Print help\n",
            "  --version         Print version\n",
        ),
        drivers = Driver::choices()
    )
}

/// What `--version` prints: the package version and the commit it was built from.
pub const VERSION: &str = concat!(
    "hdiff ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("HDIFF_GIT_HASH"),
    ")\n"
);

/// What the command line asks hdiff to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    Help,
    Install,
    Version,
    Bench { driver: Driver, source: Source },
    View { source: Source },
}

/// Why the command line was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InstallWithOperands,
    MissingBenchDriver,
    UnknownBenchDriver(OsString),
    PipedStdinIgnored,
    RecursionWithoutComparison,
    TooManyOperands,
}

impl fmt::Display for Error {
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

impl std::error::Error for Error {}

/// The invocation `arguments` select, given whether standard input is piped.
pub fn select_invocation(arguments: &[OsString], stdin: StdinState) -> Result<Invocation, Error> {
    if arguments.iter().any(|argument| argument == "--install") {
        return match arguments {
            [_] => Ok(Invocation::Install),
            _ => Err(Error::InstallWithOperands),
        };
    }

    match arguments {
        [argument] if argument == "--help" => Ok(Invocation::Help),
        [argument] if argument == "--version" => Ok(Invocation::Version),
        [flag, rest @ ..] if flag == "--bench" => select_bench(rest, stdin),
        operands => select_input(operands, stdin).map(|source| Invocation::View { source }),
    }
}

/// A benchmark run: the driver name, then the input operands.
fn select_bench(arguments: &[OsString], stdin: StdinState) -> Result<Invocation, Error> {
    let Some((name, operands)) = arguments.split_first() else {
        return Err(Error::MissingBenchDriver);
    };
    let Some(driver) = Driver::of_name(name) else {
        return Err(Error::UnknownBenchDriver(name.clone()));
    };

    select_input(operands, stdin).map(|source| Invocation::Bench { driver, source })
}

/// The input the operands select; piped standard input must be what they read.
fn select_input(arguments: &[OsString], stdin: StdinState) -> Result<Source, Error> {
    let source = select_source(arguments)?;

    match (stdin, source.reads_stdin()) {
        (StdinState::Piped, false) => Err(Error::PipedStdinIgnored),
        (StdinState::Piped, true) | (StdinState::Unpiped, _) => Ok(source),
    }
}

/// The input operands and `-r` flags name: two operands compare, fewer read a patch.
fn select_source(arguments: &[OsString]) -> Result<Source, Error> {
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

    match <[Operand; 2]>::try_from(operands) {
        Ok([old, new]) => {
            let operands = BySide { old, new };
            Ok(Source::Comparison(Comparison { operands, depth }))
        }
        Err(operands) => single_source(operands, depth),
    }
}

/// The source of any number of operands but two: a second one means more than two.
fn single_source(operands: Vec<Operand>, depth: Depth) -> Result<Source, Error> {
    let mut operands = operands.into_iter();
    match (operands.next(), operands.next(), depth) {
        (_, Some(_), _) => Err(Error::TooManyOperands),
        (_, None, Depth::Recursive) => Err(Error::RecursionWithoutComparison),
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
    use std::{ffi::OsString, path::PathBuf};

    use super::{
        BySide, Comparison, Depth, Driver, Error, Invocation, Operand, Source, StdinState,
        select_input, select_invocation,
    };

    fn select_invocation_unpiped(words: &[&str]) -> Result<Invocation, Error> {
        select_invocation(&arguments(words), StdinState::Unpiped)
    }

    fn select_input_unpiped(words: &[&str]) -> Result<Source, Error> {
        select_input(&arguments(words), StdinState::Unpiped)
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
                Err(Error::PipedStdinIgnored),
                "{arguments:?}"
            );
        }
    }

    #[test]
    fn piped_stdin_rejects_bench_operands_that_ignore_it() {
        let arguments = arguments(&["--bench", "prepare", "change.patch"]);

        assert_eq!(
            select_invocation(&arguments, StdinState::Piped),
            Err(Error::PipedStdinIgnored)
        );
    }

    #[test]
    fn selects_install_without_input_operands() {
        assert_eq!(
            select_invocation_unpiped(&["--install"]).expect("install invocation"),
            Invocation::Install
        );
    }

    #[test]
    fn rejects_install_with_input_operands() {
        assert_eq!(
            select_invocation_unpiped(&["--install", "change.patch"]),
            Err(Error::InstallWithOperands)
        );
    }

    #[test]
    fn selects_bench_driver_before_input_operands() {
        assert_eq!(
            select_invocation_unpiped(&["--bench", "stages"]).expect("bench invocation"),
            Invocation::Bench {
                driver: Driver::Stages,
                source: Source::Stdin,
            }
        );

        assert_eq!(
            select_invocation_unpiped(&["--bench", "movement", "change.patch"])
                .expect("bench invocation"),
            Invocation::Bench {
                driver: Driver::Movement,
                source: Source::PatchFile(PathBuf::from("change.patch")),
            }
        );
    }

    #[test]
    fn rejects_bench_without_a_known_driver() {
        assert_eq!(
            select_invocation_unpiped(&["--bench"]),
            Err(Error::MissingBenchDriver)
        );
        assert_eq!(
            select_invocation_unpiped(&["--bench", "a.patch"]),
            Err(Error::UnknownBenchDriver(OsString::from("a.patch")))
        );
    }

    #[test]
    fn treats_unknown_dash_arguments_as_paths() {
        assert_eq!(
            select_invocation_unpiped(&["--unknown"]).expect("view invocation"),
            Invocation::View {
                source: Source::PatchFile(PathBuf::from("--unknown")),
            }
        );
    }

    #[test]
    fn selects_stdin_without_operands() {
        assert_eq!(select_input_unpiped(&[]).expect("stdin"), Source::Stdin);
    }

    #[test]
    fn rejects_more_than_two_operands() {
        assert_eq!(
            select_input_unpiped(&["a", "b", "c"]),
            Err(Error::TooManyOperands)
        );
    }

    #[test]
    fn reads_a_lone_dash_operand_from_stdin() {
        assert_eq!(select_input_unpiped(&["-"]).expect("stdin"), Source::Stdin);
    }

    #[test]
    fn compares_a_dash_operand_as_stdin() {
        assert_eq!(
            select_input_unpiped(&["-", "after"]).expect("comparison"),
            Source::Comparison(Comparison {
                operands: BySide {
                    old: Operand::Stdin,
                    new: Operand::Path(PathBuf::from("after")),
                },
                depth: Depth::TopLevel,
            })
        );
    }

    #[test]
    fn selects_recursive_comparison_wherever_the_flag_appears() {
        let recursive = Source::Comparison(Comparison {
            operands: BySide {
                old: Operand::Path(PathBuf::from("a")),
                new: Operand::Path(PathBuf::from("b")),
            },
            depth: Depth::Recursive,
        });

        for arguments in [["-r", "a", "b"], ["a", "-r", "b"], ["a", "b", "-r"]] {
            assert_eq!(
                select_input_unpiped(&arguments).expect("comparison"),
                recursive
            );
        }
    }

    #[test]
    fn rejects_recursion_without_two_operands() {
        assert_eq!(
            select_input_unpiped(&["-r"]),
            Err(Error::RecursionWithoutComparison)
        );
        assert_eq!(
            select_input_unpiped(&["-r", "change.patch"]),
            Err(Error::RecursionWithoutComparison)
        );
    }
}
