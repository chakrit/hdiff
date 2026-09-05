use std::{ffi::OsString, path::PathBuf};

pub const HELP: &str = concat!(
    "Usage: hdiff [OPTIONS] [FILE ...]\n",
    "\n",
    "Arguments:\n",
    "  [FILE ...]  Read one patch file, compare two files, or read standard input\n",
    "              when omitted\n",
    "\n",
    "Options:\n",
    "  --bench    Prepare the diff and report benchmark measurements\n",
    "  --profile  Prepare the diff and report diagnostic stage measurements\n",
    "  --install  Register hdiff as the user-level Git diff pager\n",
    "  --help     Print help\n",
    "  --version  Print version\n",
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
    View {
        source: InputSource,
        mode: ViewerMode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerMode {
    Benchmark,
    Profile,
    Render,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    DiffFile(PathBuf),
    Operands([PathBuf; 2]),
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
        arguments
            if arguments
                .first()
                .is_some_and(|argument| argument == "--profile") =>
        {
            let operands = arguments[1..].iter().map(PathBuf::from).collect::<Vec<_>>();
            select_input(&operands).map(|source| Command::View {
                source,
                mode: ViewerMode::Profile,
            })
        }
        [argument] if argument == "--help" => Ok(Command::Help),
        [argument] if argument == "--version" => Ok(Command::Version),
        [argument] if argument == "--bench" => Ok(Command::View {
            source: InputSource::Stdin,
            mode: ViewerMode::Benchmark,
        }),
        arguments if arguments.first() == Some(&OsString::from("--bench")) => {
            let operands = arguments[1..].iter().map(PathBuf::from).collect::<Vec<_>>();
            select_input(&operands).map(|source| Command::View {
                source,
                mode: ViewerMode::Benchmark,
            })
        }
        _ => {
            let operands = arguments.iter().map(PathBuf::from).collect::<Vec<_>>();
            select_input(&operands).map(|source| Command::View {
                source,
                mode: ViewerMode::Render,
            })
        }
    }
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
    use super::{Command, InputSource, ViewerMode, select_command, select_input};
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
    fn selects_benchmark_with_standard_input() {
        assert_eq!(
            select_command(&[OsString::from("--bench")]).expect("benchmark command"),
            Command::View {
                source: InputSource::Stdin,
                mode: ViewerMode::Benchmark,
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
