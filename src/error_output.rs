//! How hdiff states an error on standard error. Rules:
//! docs/spec/architecture.md#error-output.

/// `message` with every line naming the program, each line ended.
pub fn prefixed(message: &str) -> String {
    message
        .lines()
        .map(|line| format!("hdiff: {line}\n"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::prefixed;

    #[test]
    fn prefixes_every_line_of_the_message() {
        let message = "cannot read input\ncleanup failed: raw mode: denied";

        assert_eq!(
            prefixed(message),
            "hdiff: cannot read input\nhdiff: cleanup failed: raw mode: denied\n"
        );
    }
}
