//! `cargo artic`: project tooling for artic apps.

const USAGE: &str = "\
Usage: cargo artic <command>

Commands:
    check     Report XAML diagnostics without building
    new       Create a new artic app
    schema    Generate an XSD for editor completion
    inspect   Attach to a running debug app
    ios       Generate the iOS shell project
    android   Generate the Android shell project
    lsp       Start the language server

No commands are implemented yet.
";

// Cargo runs a subcommand as `cargo-artic artic <args>`, so the first command to read its arguments
// skips `artic`.
fn main() {
  print!("{USAGE}");
}
