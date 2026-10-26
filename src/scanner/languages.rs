//! The languages the scanner reads, each with the file names and extensions that select
//! it. Rules: docs/spec/architecture.md#technology-direction.

use super::{Spec, c::C, go::GO, javascript::JAVASCRIPT, python::PYTHON, rust::RUST};

/// A language the scanner reads, with what selects it: a whole file name, or else a file
/// name's extension.
struct Language {
    file_names: &'static [&'static str],
    extensions: &'static [&'static str],
    spec: &'static Spec,
}

/// Every language the scanner reads. An extension two languages share selects the one
/// listed first.
static LANGUAGES: &[Language] = &[
    Language {
        file_names: &[],
        extensions: &["c", "h"],
        spec: &C,
    },
    Language {
        file_names: &[],
        extensions: &["go"],
        spec: &GO,
    },
    Language {
        file_names: &[],
        extensions: &["js", "jsx"],
        spec: &JAVASCRIPT,
    },
    Language {
        file_names: &[],
        extensions: &["py"],
        spec: &PYTHON,
    },
    Language {
        file_names: &[],
        extensions: &["rs"],
        spec: &RUST,
    },
    Language {
        file_names: &[],
        extensions: &["s", "S", "asm", "nasm"],
        spec: &super::asm::ASM,
    },
    Language {
        file_names: &[],
        extensions: &["clj", "cljs", "cljc", "edn"],
        spec: &super::clojure::CLOJURE,
    },
    Language {
        file_names: &["CMakeLists.txt"],
        extensions: &["cmake"],
        spec: &super::cmake::CMAKE,
    },
    Language {
        file_names: &[],
        extensions: &[
            "cc", "cpp", "cxx", "c++", "hh", "hpp", "hxx", "h++", "ipp", "tpp", "inl",
        ],
        spec: &super::cpp::CPP,
    },
    Language {
        file_names: &[],
        extensions: &["cr"],
        spec: &super::crystal::CRYSTAL,
    },
    Language {
        file_names: &[],
        extensions: &["cs", "csx"],
        spec: &super::csharp::CSHARP,
    },
    Language {
        file_names: &[],
        extensions: &["css"],
        spec: &super::css::CSS,
    },
    Language {
        file_names: &[],
        extensions: &["cu", "cuh"],
        spec: &super::cuda::CUDA,
    },
    Language {
        file_names: &[],
        extensions: &["dart"],
        spec: &super::dart::DART,
    },
    Language {
        file_names: &["Dockerfile", "Containerfile"],
        extensions: &["dockerfile", "containerfile"],
        spec: &super::dockerfile::DOCKERFILE,
    },
    Language {
        file_names: &["mix.lock"],
        extensions: &["ex", "exs"],
        spec: &super::elixir::ELIXIR,
    },
    Language {
        file_names: &["rebar.config"],
        extensions: &["erl", "hrl"],
        spec: &super::erlang::ERLANG,
    },
    Language {
        file_names: &[],
        extensions: &["fs", "fsi", "fsx"],
        spec: &super::fsharp::FSHARP,
    },
    Language {
        file_names: &[],
        extensions: &["gleam"],
        spec: &super::gleam::GLEAM,
    },
    Language {
        file_names: &[],
        extensions: &["graphql", "gql", "graphqls"],
        spec: &super::graphql::GRAPHQL,
    },
    Language {
        file_names: &["Jenkinsfile"],
        extensions: &["groovy", "gradle", "gvy", "gy", "gsh"],
        spec: &super::groovy::GROOVY,
    },
    Language {
        file_names: &[],
        extensions: &["hs", "lhs"],
        spec: &super::haskell::HASKELL,
    },
    Language {
        file_names: &[".terraformrc", "terraform.rc"],
        extensions: &["tf", "tfvars", "hcl", "nomad"],
        spec: &super::hcl::HCL,
    },
    Language {
        file_names: &[],
        extensions: &["html", "htm", "xhtml", "vue", "svelte"],
        spec: &super::html::HTML,
    },
    Language {
        file_names: &[
            ".editorconfig",
            ".gitconfig",
            ".gitmodules",
            ".npmrc",
            ".pylintrc",
            "pylintrc",
            ".flake8",
        ],
        extensions: &[
            "ini", "cfg", "inf", "desktop", "service", "socket", "timer", "mount", "target",
            "slice",
        ],
        spec: &super::ini::INI,
    },
    Language {
        file_names: &[],
        extensions: &["java"],
        spec: &super::java::JAVA,
    },
    Language {
        file_names: &[
            ".babelrc",
            ".jshintrc",
            ".swcrc",
            "composer.lock",
            "flake.lock",
        ],
        extensions: &["json", "jsonc", "json5", "geojson", "webmanifest"],
        spec: &super::json::JSON,
    },
    Language {
        file_names: &[],
        extensions: &["jl"],
        spec: &super::julia::JULIA,
    },
    Language {
        file_names: &[],
        extensions: &["kt", "kts"],
        spec: &super::kotlin::KOTLIN,
    },
    Language {
        file_names: &[],
        extensions: &["less"],
        spec: &super::less::LESS,
    },
    Language {
        file_names: &[],
        extensions: &["lua"],
        spec: &super::lua::LUA,
    },
    Language {
        file_names: &["Makefile", "makefile", "GNUmakefile", "BSDmakefile"],
        extensions: &["mk", "mak", "make"],
        spec: &super::makefile::MAKEFILE,
    },
    Language {
        file_names: &[],
        extensions: &["md", "markdown", "mdown", "mkd", "mdx"],
        spec: &super::markdown::MARKDOWN,
    },
    Language {
        file_names: &[],
        extensions: &["nim", "nims", "nimble"],
        spec: &super::nim::NIM,
    },
    Language {
        file_names: &[],
        extensions: &["m", "mm"],
        spec: &super::objc::OBJC,
    },
    Language {
        file_names: &[],
        extensions: &["ml", "mli"],
        spec: &super::ocaml::OCAML,
    },
    Language {
        file_names: &[],
        extensions: &["pl", "pm", "t"],
        spec: &super::perl::PERL,
    },
    Language {
        file_names: &[],
        extensions: &["php", "phtml", "php3", "php4", "php5", "php7", "phps"],
        spec: &super::php::PHP,
    },
    Language {
        file_names: &[],
        extensions: &["ps1", "psm1", "psd1"],
        spec: &super::powershell::POWERSHELL,
    },
    Language {
        file_names: &[],
        extensions: &["proto"],
        spec: &super::proto::PROTO,
    },
    Language {
        file_names: &[".Rprofile"],
        extensions: &["r", "R"],
        spec: &super::r::R,
    },
    Language {
        file_names: &[
            "Gemfile",
            "Rakefile",
            "Guardfile",
            "Podfile",
            "Vagrantfile",
            "Brewfile",
            "Fastfile",
            "Capfile",
        ],
        extensions: &["rb", "rake", "gemspec", "ru", "rbw"],
        spec: &super::ruby::RUBY,
    },
    Language {
        file_names: &[],
        extensions: &["scala", "sc", "sbt"],
        spec: &super::scala::SCALA,
    },
    Language {
        file_names: &[],
        extensions: &["scss", "sass"],
        spec: &super::scss::SCSS,
    },
    Language {
        file_names: &[
            ".bashrc",
            ".bash_profile",
            ".bash_logout",
            ".bash_aliases",
            ".profile",
            ".zshrc",
            ".zprofile",
            ".zshenv",
            ".envrc",
            "PKGBUILD",
        ],
        extensions: &["sh", "bash", "zsh", "ksh"],
        spec: &super::shell::SHELL,
    },
    Language {
        file_names: &[],
        extensions: &["sql"],
        spec: &super::sql::SQL,
    },
    Language {
        file_names: &[],
        extensions: &["swift"],
        spec: &super::swift::SWIFT,
    },
    Language {
        file_names: &["Cargo.lock", "Pipfile", "poetry.lock", "uv.lock"],
        extensions: &["toml"],
        spec: &super::toml::TOML,
    },
    Language {
        file_names: &[],
        extensions: &["vb", "vbs", "bas"],
        spec: &super::vb::VB,
    },
    Language {
        file_names: &[],
        extensions: &[
            "xml", "xsd", "xsl", "xslt", "svg", "plist", "rss", "atom", "wsdl", "xaml", "csproj",
            "fsproj", "vbproj", "props", "targets", "resx",
        ],
        spec: &super::xml::XML,
    },
    Language {
        file_names: &[".clang-format", ".clang-tidy", ".clangd"],
        extensions: &["yaml", "yml"],
        spec: &super::yaml::YAML,
    },
    Language {
        file_names: &[],
        extensions: &["zig", "zon"],
        spec: &super::zig::ZIG,
    },
];

/// The spec of the language `path` selects: by its whole file name, or else by its
/// extension; none for any other path.
pub fn of_path(path: &str) -> Option<&'static Spec> {
    let file_name = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let by_name = LANGUAGES
        .iter()
        .find(|language| language.file_names.contains(&file_name));
    let by_extension = || {
        let (_, extension) = file_name.rsplit_once('.')?;
        LANGUAGES
            .iter()
            .find(|language| language.extensions.contains(&extension))
    };

    by_name.or_else(by_extension).map(|language| language.spec)
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use super::{super::go::GO, LANGUAGES, of_path};

    #[test]
    fn selects_a_language_by_file_name_then_by_extension() {
        assert!(of_path("cmd/main.go").is_some_and(|spec| ptr::eq(spec, &GO)));
        assert!(of_path("README").is_none());
        assert!(of_path("archive.tar").is_none());
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
