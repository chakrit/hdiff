use std::io::{self, Write};

pub struct WriteOutput<'a> {
    pub bytes: &'a [u8],
}

pub struct OutputContext<'a, Writer> {
    pub writer: &'a mut Writer,
}

impl WriteOutput<'_> {
    pub fn run(self, context: &mut OutputContext<'_, impl Write>) -> io::Result<()> {
        match context.writer.write_all(self.bytes) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use super::{OutputContext, WriteOutput};

    struct LimitedWriter {
        bytes: Vec<u8>,
        write_limit: usize,
    }

    impl Write for LimitedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let accepted = bytes.len().min(self.write_limit);
            self.bytes.extend_from_slice(&bytes[..accepted]);
            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingWriter {
        kind: io::ErrorKind,
    }

    impl Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(self.kind))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn writes_complete_output_across_partial_writes() {
        let mut writer = LimitedWriter {
            bytes: Vec::new(),
            write_limit: 3,
        };

        let mut context = OutputContext {
            writer: &mut writer,
        };

        WriteOutput { bytes: b"complete" }
            .run(&mut context)
            .expect("output write should succeed");

        assert_eq!(writer.bytes, b"complete");
    }

    #[test]
    fn treats_broken_pipe_as_a_quiet_success() {
        let mut writer = FailingWriter {
            kind: io::ErrorKind::BrokenPipe,
        };

        let mut context = OutputContext {
            writer: &mut writer,
        };

        let result = WriteOutput { bytes: b"output" }.run(&mut context);

        assert!(result.is_ok(), "broken pipe should end output quietly");
    }

    #[test]
    fn propagates_other_write_failures() {
        let mut writer = FailingWriter {
            kind: io::ErrorKind::PermissionDenied,
        };

        let mut context = OutputContext {
            writer: &mut writer,
        };

        let error = WriteOutput { bytes: b"output" }
            .run(&mut context)
            .expect_err("non-broken-pipe failures should propagate");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    }
}
