#![deny(warnings)]

pub mod actions;
pub mod detail;
pub mod document;
pub mod input;
pub mod interaction;
pub mod layout;
pub mod parser;
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

            actions::git_config::InstallGitDiffPager { executable }
                .run(&mut context)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        input::Command::View(source) => run_viewer(source)?,
    }

    Ok(())
}

fn run_viewer(source: input::InputSource) -> Result<(), String> {
    let input = match source {
        input::InputSource::Stdin => {
            let mut input = Vec::new();
            std::io::Read::read_to_end(&mut std::io::stdin(), &mut input)
                .map_err(|error| error.to_string())?;
            input
        }
        input::InputSource::DiffFile(path) => {
            std::fs::read(path).map_err(|error| error.to_string())?
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
            .map_err(|error| error.to_string())?
        }
    };
    let document = parser::parse_unified_diff(&input).map_err(|error| error.message)?;
    let output = render::render_unified(&document);

    match terminal::mode_for_output(std::io::IsTerminal::is_terminal(&std::io::stdout())) {
        terminal::OutputMode::Finite => {
            let mut stdout = std::io::stdout();
            let mut output_context = actions::write_output::OutputContext {
                writer: &mut stdout,
            };
            actions::write_output::WriteOutput { bytes: &output }
                .run(&mut output_context)
                .map_err(|error| error.to_string())?;
        }
        terminal::OutputMode::Interactive => {
            terminal::run_interactive(&document).map_err(|error| error.to_string())?;
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
