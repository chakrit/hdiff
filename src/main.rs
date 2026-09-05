#![deny(warnings)]

pub mod actions;
pub mod detail;
pub mod document;
pub mod input;
pub mod interaction;
pub mod layout;
pub mod parser;
pub mod prepared;
pub mod render;
pub mod syntax;
pub mod terminal;
pub mod theme;

fn main() -> Result<(), String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let command = input::select_command(&arguments).map_err(|error| error.0)?;

    match command {
        input::Command::Help => write_stdout(input::HELP.as_bytes())?,
        input::Command::Install => {
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            let mut context = actions::git_config::InstallGitDiffPagerContext;

            let report = actions::git_config::InstallGitDiffPager { executable }
                .run(&mut context)
                .map_err(|error| error.to_string())?;
            let output = format!("{}\n", report.completed_commands.join("\n"));
            write_stdout(output.as_bytes())?;
            return Ok(());
        }
        input::Command::Version => write_stdout(input::VERSION.as_bytes())?,
        input::Command::View { source, mode } => run_viewer(source, mode)?,
    }

    Ok(())
}

fn read_input(source: input::InputSource) -> Result<Vec<u8>, String> {
    match source {
        input::InputSource::Stdin => {
            let mut input = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
                .map_err(|error| error.to_string())?;
            Ok(input)
        }
        input::InputSource::DiffFile(path) => {
            std::fs::read(path).map_err(|error| error.to_string())
        }
        input::InputSource::Operands([before, after]) => {
            let mut context = actions::compare_files::CompareFilesContext;

            actions::compare_files::CompareFiles {
                before: &before,
                after: &after,
            }
            .run(&mut context)
            .map_err(|error| error.to_string())
        }
    }
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

fn run_viewer(source: input::InputSource, mode: input::ViewerMode) -> Result<(), String> {
    let input = read_input(source)?;
    let document = parser::parse_unified_diff(&input).map_err(|error| error.message)?;

    match mode {
        input::ViewerMode::Benchmark => run_benchmark(&document),
        input::ViewerMode::Render => run_render(&document),
    }
}

fn run_benchmark(document: &document::DiffDocument) -> Result<(), String> {
    let preparation_started = std::time::Instant::now();
    let prepared = prepared::PreparedDocument::prepare(document);
    let elapsed = preparation_started.elapsed();
    let output = format!(
        "preparation duration_ns={} files={} records={}\n",
        elapsed.as_nanos(),
        prepared.file_count(),
        prepared.record_count()
    );
    write_stdout(output.as_bytes())
}

fn run_render(document: &document::DiffDocument) -> Result<(), String> {
    let output_mode =
        terminal::mode_for_output(std::io::IsTerminal::is_terminal(&std::io::stdout()));

    match output_mode {
        terminal::OutputMode::Finite => {
            let output = render::render_unified(document);
            write_stdout(&output)?;
        }
        terminal::OutputMode::Interactive => {
            let prepared = prepared::PreparedDocument::prepare(document);

            terminal::run_interactive(&prepared).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{parser::parse_unified_diff, render::render_unified};

    #[test]
    fn turns_diff_input_into_finite_safe_output() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";

        let document = parse_unified_diff(input).expect("valid diff");
        let output = render_unified(&document);

        assert_eq!(output, input);
    }
}
