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

fn main() -> Result<(), String> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    let command = input::select_command(&arguments).map_err(|error| error.0)?;

    match command {
        input::Command::Install => {
            let executable = std::env::current_exe().map_err(|error| error.to_string())?;
            let mut context = actions::git_config::InstallGitDiffPagerContext;

            let report = actions::git_config::InstallGitDiffPager { executable }
                .run(&mut context)
                .map_err(|error| error.to_string())?;
            let output = format!("{}\n", report.completed_commands.join("\n"));
            let mut stdout = std::io::stdout();
            let mut output_context = actions::write_output::OutputContext {
                writer: &mut stdout,
            };

            actions::write_output::WriteOutput {
                bytes: output.as_bytes(),
            }
            .run(&mut output_context)
            .map_err(|error| error.to_string())?;
            return Ok(());
        }
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
        input::InputSource::Operands(paths) => {
            let [before, after]: [std::path::PathBuf; 2] = paths
                .try_into()
                .map_err(|_| "expected two comparison operands".to_owned())?;
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

fn run_viewer(source: input::InputSource, mode: input::ViewerMode) -> Result<(), String> {
    let input = read_input(source)?;
    let document = parser::parse_unified_diff(&input).map_err(|error| error.message)?;
    let output_mode =
        terminal::mode_for_output(std::io::IsTerminal::is_terminal(&std::io::stdout()));
    let preparation_started = match mode {
        input::ViewerMode::Benchmark => Some(std::time::Instant::now()),
        input::ViewerMode::Render => None,
    };
    let prepared = match (mode, output_mode) {
        (input::ViewerMode::Benchmark, _)
        | (input::ViewerMode::Render, terminal::OutputMode::Interactive) => {
            Some(prepared::PreparedDocument::prepare(&document))
        }
        (input::ViewerMode::Render, terminal::OutputMode::Finite) => None,
    };

    match mode {
        input::ViewerMode::Benchmark => {
            let elapsed = preparation_started
                .expect("benchmark timing starts before preparation")
                .elapsed();
            let prepared = prepared.expect("benchmark preparation completes before reporting");
            let output = format!(
                "preparation duration_ns={} files={} records={}\n",
                elapsed.as_nanos(),
                prepared.file_count(),
                prepared.record_count()
            );
            let mut stdout = std::io::stdout();
            let mut output_context = actions::write_output::OutputContext {
                writer: &mut stdout,
            };
            actions::write_output::WriteOutput {
                bytes: output.as_bytes(),
            }
            .run(&mut output_context)
            .map_err(|error| error.to_string())?;
        }
        input::ViewerMode::Render => match output_mode {
            terminal::OutputMode::Finite => {
                let output = render::render_unified(&document);
                let mut stdout = std::io::stdout();
                let mut output_context = actions::write_output::OutputContext {
                    writer: &mut stdout,
                };
                actions::write_output::WriteOutput { bytes: &output }
                    .run(&mut output_context)
                    .map_err(|error| error.to_string())?;
            }
            terminal::OutputMode::Interactive => {
                let prepared = prepared.expect("interactive rendering needs prepared data");

                terminal::run_interactive(&prepared).map_err(|error| error.to_string())?;
            }
        },
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
