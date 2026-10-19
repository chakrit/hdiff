//! Rules: docs/spec/architecture.md#error-output.

use std::{fmt::Display, io::Write};

use super::{
    Error,
    write_output::{OutputContext, WriteOutput},
};
use crate::error_output;

/// States `error` on the context's writer, every line naming the program.
pub struct ReportError<'a, E> {
    pub error: &'a E,
}

impl<E: Display> ReportError<'_, E> {
    pub fn run(self, context: &mut OutputContext<'_, impl Write>) -> Result<(), Error> {
        let message = error_output::prefixed(&self.error.to_string());

        WriteOutput {
            bytes: message.as_bytes(),
        }
        .run(context)
    }
}

#[cfg(test)]
mod tests {
    use super::{OutputContext, ReportError};

    #[test]
    fn writes_the_prefixed_message_to_its_context() {
        let mut written = Vec::new();

        ReportError {
            error: &"first\nsecond",
        }
        .run(&mut OutputContext {
            writer: &mut written,
        })
        .expect("writing to a vector cannot fail");

        assert_eq!(
            String::from_utf8_lossy(&written),
            "hdiff: first\nhdiff: second\n"
        );
    }
}
