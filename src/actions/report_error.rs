use std::io::Write;

use super::{
    Error,
    write_output::{OutputContext, WriteOutput},
};

/// Writes an error message with every line naming the program. Rules:
/// docs/spec/architecture.md#error-output.
pub struct ReportError<'a> {
    pub message: &'a str,
}

impl ReportError<'_> {
    pub fn run(self, context: &mut OutputContext<'_, impl Write>) -> Result<(), Error> {
        let prefixed = self
            .message
            .lines()
            .map(|line| format!("hdiff: {line}\n"))
            .collect::<String>();

        WriteOutput {
            bytes: prefixed.as_bytes(),
        }
        .run(context)
    }
}

#[cfg(test)]
mod tests {
    use super::{OutputContext, ReportError};

    #[test]
    fn prefixes_every_line_of_the_message() {
        let mut written = Vec::new();

        ReportError {
            message: "cannot read input\ncleanup failed: raw mode: denied",
        }
        .run(&mut OutputContext {
            writer: &mut written,
        })
        .expect("writing to a Vec cannot fail");

        assert_eq!(
            String::from_utf8_lossy(&written),
            "hdiff: cannot read input\nhdiff: cleanup failed: raw mode: denied\n"
        );
    }
}
