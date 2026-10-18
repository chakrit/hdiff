use std::process::ExitStatus;

/// `diff`'s exit status convention, which hdiff's own errors follow. Contract:
/// docs/spec/architecture.md#error-output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Same,
    Different,
    Trouble,
}

impl DiffStatus {
    /// Reads a finished `diff`: any status other than same or different is trouble.
    pub fn of_exit(status: ExitStatus) -> Self {
        let code = status.code();

        [Self::Same, Self::Different]
            .into_iter()
            .find(|candidate| code == Some(i32::from(candidate.code())))
            .unwrap_or(Self::Trouble)
    }

    pub fn code(self) -> u8 {
        match self {
            Self::Same => 0,
            Self::Different => 1,
            Self::Trouble => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{os::unix::process::ExitStatusExt, process::ExitStatus};

    use super::DiffStatus;

    fn exited(code: i32) -> ExitStatus {
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn reads_diff_exit_codes() {
        assert_eq!(DiffStatus::of_exit(exited(0)), DiffStatus::Same);
        assert_eq!(DiffStatus::of_exit(exited(1)), DiffStatus::Different);
        assert_eq!(DiffStatus::of_exit(exited(2)), DiffStatus::Trouble);
        assert_eq!(DiffStatus::of_exit(exited(3)), DiffStatus::Trouble);
    }

    #[test]
    fn treats_a_signalled_diff_as_trouble() {
        assert_eq!(
            DiffStatus::of_exit(ExitStatus::from_raw(9)),
            DiffStatus::Trouble
        );
    }
}
