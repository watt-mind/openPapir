//! The `completions` command: write one shell's completion script to stdout.
//!
//! The script is generated from the same command definition the parser uses,
//! so it can never describe a command this build does not have. It is written
//! to stdout alone, like `skill`: no archive is opened, no file is written,
//! and there is no `--json` form, because a completion script is not a
//! result to report in two shapes.

use std::io::{self, Write};

use clap::CommandFactory;
use clap_complete::{Generator, Shell};

use crate::stdout;

/// The binary name the generated script completes, which is this build's own
/// and never a value the caller supplied.
const BIN_NAME: &str = "openpapir";

/// The kind of document a failing stdout names on stderr.
const DOCUMENT: &str = "completion script";

/// Write the completion script for `shell` to stdout and return the exit code.
pub fn emit<Parser: CommandFactory>(shell: Shell) -> i32 {
    stdout::emit(DOCUMENT, |out| script::<Parser>(shell, out))
}

/// Generate one shell's script from the parser's own command definition.
///
/// The definition is built before it is handed over, which is what fills in
/// the help and version flags the script is expected to complete.
fn script<Parser: CommandFactory>(shell: Shell, out: &mut dyn Write) -> io::Result<()> {
    let mut command = Parser::command();
    command.set_bin_name(BIN_NAME);
    command.build();
    shell.try_generate(&command, out)
}

#[cfg(test)]
mod tests {
    use super::{BIN_NAME, script};
    use clap::CommandFactory;
    use clap_complete::Shell;

    /// The command definition this crate's parser derives.
    type Parser = crate::Args;

    /// Every shell openPapir offers, which is the whole `Shell` value set.
    fn shells() -> Vec<Shell> {
        vec![
            Shell::Bash,
            Shell::Elvish,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Zsh,
        ]
    }

    /// Render one shell's script as text.
    fn rendered(shell: Shell) -> String {
        let mut written = Vec::new();
        script::<Parser>(shell, &mut written).expect("a vector takes every byte");
        String::from_utf8(written).expect("a completion script is UTF-8")
    }

    /// Each script names the binary and every one of its subcommands, so a
    /// command added later cannot quietly stay uncompletable.
    #[test]
    fn every_shell_script_names_the_binary_and_every_subcommand() {
        let command = Parser::command();
        let names: Vec<String> = command
            .get_subcommands()
            .map(|sub| sub.get_name().to_owned())
            .collect();
        assert!(!names.is_empty(), "the parser defines subcommands");
        for shell in shells() {
            let text = rendered(shell);
            assert!(text.contains(BIN_NAME), "{shell} omits the binary name");
            for name in &names {
                assert!(text.contains(name), "{shell} omits {name}");
            }
        }
    }

    /// A shell this build cannot generate for is refused by the parser, and
    /// the argument the refusal names is this command's own `shell`. The
    /// parser's usage text repeats the value typed, as it does for every
    /// invocation it rejects; nothing openPapir prints about an archive
    /// does.
    #[test]
    fn an_unknown_shell_is_refused_and_names_the_shell_argument() {
        let error = <Parser as clap::Parser>::try_parse_from(["openpapir", "completions", "sh"])
            .err()
            .expect("an unknown shell is refused");
        assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
        let named = error
            .get(clap::error::ContextKind::InvalidArg)
            .expect("the parser names the argument it refused");
        assert_eq!(
            format!("{named}").trim_matches(|c| matches!(c, '<' | '>')),
            "SHELL",
            "the refusal names this command's own argument"
        );
        for shell in shells() {
            assert!(
                <Parser as clap::Parser>::try_parse_from([
                    "openpapir",
                    "completions",
                    &shell.to_string()
                ])
                .is_ok(),
                "{shell} is accepted by its own name"
            );
        }
    }

    /// The script is generated from the definition, not from a stored copy,
    /// so it is never empty and never carries a path from this machine.
    #[test]
    fn a_script_is_generated_and_carries_no_caller_supplied_path() {
        for shell in shells() {
            let text = rendered(shell);
            assert!(!text.is_empty(), "{shell} produced nothing");
            assert!(
                !text.contains(env!("CARGO_MANIFEST_DIR")),
                "{shell} carries a build path"
            );
        }
    }
}
