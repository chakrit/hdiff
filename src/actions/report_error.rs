//! Rules: docs/spec/architecture.md#error-output.

use std::{fmt::Display, io};

use super::{
    Error,
    write_output::{OutputContext, WriteOutput},
};
use crate::error_output;

/// States `error` on standard error, every line naming the program.
pub struct ReportError<'a, E> {
    pub error: &'a E,
}

impl<E: Display> ReportError<'_, E> {
    pub fn run(self) -> Result<(), Error> {
        let message = error_output::prefixed(&self.error.to_string());

        WriteOutput {
            bytes: message.as_bytes(),
        }
        .run(&mut OutputContext {
            writer: &mut io::stderr(),
        })
    }
}
