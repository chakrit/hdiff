use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum InputSource {
    Stdin,
    DiffFile(PathBuf),
    Operands(Vec<PathBuf>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct InputError(pub String);

pub fn select_input(arguments: &[PathBuf]) -> Result<InputSource, InputError> {
    match arguments {
        [] => Ok(InputSource::Stdin),
        [path] => Ok(InputSource::DiffFile(path.clone())),
        [first, second] => Ok(InputSource::Operands(vec![first.clone(), second.clone()])),
        _ => Err(InputError("expected zero, one, or two operands".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::{InputSource, select_input};
    use std::path::PathBuf;

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
