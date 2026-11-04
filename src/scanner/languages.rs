//! The languages the scanner reads, each with the file types that select it. Rules:
//! docs/spec/architecture.md#technology-direction.

use super::{Spec, c::C, go::GO, javascript::JAVASCRIPT, python::PYTHON, rust::RUST};

/// A language the scanner reads, with the file types that select it.
struct Language {
    file_types: &'static [&'static str],
    spec: &'static Spec,
}

/// Every language the scanner reads.
static LANGUAGES: &[Language] = &[
    Language {
        file_types: &["c"],
        spec: &C,
    },
    Language {
        file_types: &["go"],
        spec: &GO,
    },
    Language {
        file_types: &["javascript", "javascriptreact"],
        spec: &JAVASCRIPT,
    },
    Language {
        file_types: &["python"],
        spec: &PYTHON,
    },
    Language {
        file_types: &["rust"],
        spec: &RUST,
    },
    Language {
        file_types: &["asm", "nasm"],
        spec: &super::asm::ASM,
    },
    Language {
        file_types: &["clojure"],
        spec: &super::clojure::CLOJURE,
    },
    Language {
        file_types: &["cmake"],
        spec: &super::cmake::CMAKE,
    },
    Language {
        file_types: &["cpp"],
        spec: &super::cpp::CPP,
    },
    Language {
        file_types: &["crystal"],
        spec: &super::crystal::CRYSTAL,
    },
    Language {
        file_types: &["cs"],
        spec: &super::csharp::CSHARP,
    },
    Language {
        file_types: &["css"],
        spec: &super::css::CSS,
    },
    Language {
        file_types: &["cuda"],
        spec: &super::cuda::CUDA,
    },
    Language {
        file_types: &["dart"],
        spec: &super::dart::DART,
    },
    Language {
        file_types: &["dockerfile"],
        spec: &super::dockerfile::DOCKERFILE,
    },
    Language {
        file_types: &["elixir"],
        spec: &super::elixir::ELIXIR,
    },
    Language {
        file_types: &["erlang"],
        spec: &super::erlang::ERLANG,
    },
    Language {
        file_types: &["fsharp"],
        spec: &super::fsharp::FSHARP,
    },
    Language {
        file_types: &["gleam"],
        spec: &super::gleam::GLEAM,
    },
    Language {
        file_types: &["graphql"],
        spec: &super::graphql::GRAPHQL,
    },
    Language {
        file_types: &["groovy"],
        spec: &super::groovy::GROOVY,
    },
    Language {
        file_types: &["haskell", "lhaskell"],
        spec: &super::haskell::HASKELL,
    },
    Language {
        file_types: &["hcl", "terraform", "terraform-vars"],
        spec: &super::hcl::HCL,
    },
    Language {
        file_types: &["html", "svelte", "vue", "xhtml"],
        spec: &super::html::HTML,
    },
    Language {
        file_types: &[
            "cfg",
            "desktop",
            "dosini",
            "editorconfig",
            "gitconfig",
            "systemd",
        ],
        spec: &super::ini::INI,
    },
    Language {
        file_types: &["java"],
        spec: &super::java::JAVA,
    },
    Language {
        file_types: &["json", "json5", "jsonc"],
        spec: &super::json::JSON,
    },
    Language {
        file_types: &["julia"],
        spec: &super::julia::JULIA,
    },
    Language {
        file_types: &["kotlin"],
        spec: &super::kotlin::KOTLIN,
    },
    Language {
        file_types: &["less"],
        spec: &super::less::LESS,
    },
    Language {
        file_types: &["lua"],
        spec: &super::lua::LUA,
    },
    Language {
        file_types: &["make"],
        spec: &super::makefile::MAKEFILE,
    },
    Language {
        file_types: &["markdown"],
        spec: &super::markdown::MARKDOWN,
    },
    Language {
        file_types: &["nim"],
        spec: &super::nim::NIM,
    },
    Language {
        file_types: &["objc", "objcpp"],
        spec: &super::objc::OBJC,
    },
    Language {
        file_types: &["ocaml"],
        spec: &super::ocaml::OCAML,
    },
    Language {
        file_types: &["perl"],
        spec: &super::perl::PERL,
    },
    Language {
        file_types: &["php"],
        spec: &super::php::PHP,
    },
    Language {
        file_types: &["ps1"],
        spec: &super::powershell::POWERSHELL,
    },
    Language {
        file_types: &["proto"],
        spec: &super::proto::PROTO,
    },
    Language {
        file_types: &["r"],
        spec: &super::r::R,
    },
    Language {
        file_types: &["ruby"],
        spec: &super::ruby::RUBY,
    },
    Language {
        file_types: &["sbt", "scala"],
        spec: &super::scala::SCALA,
    },
    Language {
        file_types: &["sass", "scss"],
        spec: &super::scss::SCSS,
    },
    Language {
        file_types: &["sh", "zsh"],
        spec: &super::shell::SHELL,
    },
    Language {
        file_types: &["sql"],
        spec: &super::sql::SQL,
    },
    Language {
        file_types: &["swift"],
        spec: &super::swift::SWIFT,
    },
    Language {
        file_types: &["toml"],
        spec: &super::toml::TOML,
    },
    Language {
        file_types: &["basic", "vb"],
        spec: &super::vb::VB,
    },
    Language {
        file_types: &["svg", "xml", "xsd", "xslt"],
        spec: &super::xml::XML,
    },
    Language {
        file_types: &["yaml"],
        spec: &super::yaml::YAML,
    },
    Language {
        file_types: &["zig"],
        spec: &super::zig::ZIG,
    },
];

/// The spec of the language `file_type` names; none for a file type the scanner does not
/// read.
pub fn of_file_type(file_type: &str) -> Option<&'static Spec> {
    LANGUAGES
        .iter()
        .find(|language| language.file_types.contains(&file_type))
        .map(|language| language.spec)
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{super::go::GO, LANGUAGES, of_file_type};

    #[test]
    fn selects_a_language_by_file_type() {
        assert!(of_file_type("go").is_some_and(|spec| ptr::eq(spec, &GO)));
        assert!(of_file_type("text").is_none());
    }

    #[test]
    fn every_word_list_is_sorted_for_lookup() {
        for language in LANGUAGES {
            for (words, class) in language.spec.words {
                assert!(words.is_sorted(), "{class:?} words are sorted: {words:?}");
            }
        }
    }
}
