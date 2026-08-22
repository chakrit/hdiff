pub mod document;
pub mod input;
pub mod parser;

fn main() -> Result<(), String> {
    let operands = std::env::args_os()
        .skip(1)
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    let source = input::select_input(&operands).map_err(|error| error.0)?;
    let text = match source {
        input::InputSource::Stdin => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .map_err(|error| error.to_string())?;
            text
        }
        input::InputSource::DiffFile(path) => {
            std::fs::read_to_string(path).map_err(|error| error.to_string())?
        }
        input::InputSource::Operands(_) => {
            return Err("file comparison is not implemented yet".to_owned());
        }
    };
    parser::parse_unified_diff(&text).map_err(|error| error.message)?;
    Ok(())
}
