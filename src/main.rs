#![deny(warnings)]

pub mod actions;
pub mod document;
pub mod input;
pub mod interaction;
pub mod parser;
pub mod render;
pub mod terminal;

fn render_input(input: &[u8]) -> Result<Vec<u8>, String> {
    let document = parser::parse_unified_diff(input).map_err(|error| error.message)?;

    Ok(render::render_unified(&document))
}

fn main() -> Result<(), String> {
    let operands = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let source = input::select_input(&operands).map_err(|error| error.0)?;
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
        input::InputSource::Operands(_) => {
            return Err("file comparison is not implemented yet".to_owned());
        }
    };
    let output = render_input(&input)?;

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
            terminal::run_interactive(&output).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_input;

    #[test]
    fn turns_diff_input_into_finite_safe_output() {
        let input = b"--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";

        let output = render_input(input).expect("valid diff");

        assert_eq!(output, input);
    }
}
