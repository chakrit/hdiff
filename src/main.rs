#![deny(warnings)]

use hdiff::{actions, document, input, prepared, render, terminal};

fn main() -> Result<(), String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let stdin = input::StdinState::inspect().map_err(|error| error.to_string())?;
    let command = input::select_command(&arguments, stdin).map_err(|error| error.0)?;

    match command {
        input::Command::Help => write_stdout(input::HELP.as_bytes())?,
        input::Command::Install => {
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            let mut context = actions::git_config::InstallGitDiffPagerContext {
                environment: actions::git_config::GitConfigEnvironment::of_process(),
            };

            let report = actions::git_config::InstallGitDiffPager { executable }
                .run(&mut context)
                .map_err(|error| error.to_string())?;
            let output = format!("{}\n", report.completed_commands.join("\n"));
            write_stdout(output.as_bytes())?;
        }
        input::Command::Version => write_stdout(input::VERSION.as_bytes())?,
        input::Command::Bench { driver, source } => {
            let report = driver.run(source)?;
            write_stdout(report.as_bytes())?;
        }
        input::Command::View { source } => view(source)?,
    }

    Ok(())
}

fn write_stdout(bytes: &[u8]) -> Result<(), String> {
    let mut stdout = std::io::stdout();
    let mut context = actions::write_output::OutputContext {
        writer: &mut stdout,
    };

    actions::write_output::WriteOutput { bytes }
        .run(&mut context)
        .map_err(|error| error.to_string())
}

fn view(source: input::InputSource) -> Result<(), String> {
    let input = source.read()?;
    if input.bytes().is_empty() {
        return Ok(());
    }

    let document = input.parse()?;
    render_document(&document)
}

fn render_document(document: &document::DiffDocument) -> Result<(), String> {
    let output_mode =
        terminal::mode_for_output(std::io::IsTerminal::is_terminal(&std::io::stdout()));

    match output_mode {
        terminal::OutputMode::Finite => write_stdout(&render::render_unified(document)),
        terminal::OutputMode::Interactive => {
            let Some(controlling_terminal) = terminal::open_controlling_terminal() else {
                return write_stdout(&render::render_unified(document));
            };
            let prepared = prepared::PreparedDocument::prepare(document);
            terminal::run_interactive(controlling_terminal, prepared)
                .map_err(|error| error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use hdiff::{parser::parse_unified_diff, render::render_unified};

    #[test]
    fn turns_diff_input_into_finite_safe_output() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input).expect("valid diff");
        let output = render_unified(&document);

        assert_eq!(output, input);
    }
}
