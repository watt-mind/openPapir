//! The `manpage` command: write the man page for the whole command tree to
//! stdout.
//!
//! The page is generated from the same command definition the parser uses, so
//! it can never describe a command this build does not have. It is written to
//! stdout alone, like `skill`: no archive is opened, no file is written, no
//! directory is taken, and there is no `--json` form.
//!
//! One invocation writes one roff stream holding the whole tree: the page for
//! `openpapir` first, then one page for each subcommand at any depth, each
//! with its own title line and named the way a manual page for a subcommand
//! is named, `openpapir-archive-init` for one. A reader pages through the
//! stream with `openpapir manpage | man -l -`, and an installer that wants
//! separate files splits the stream on its title lines.

use std::io::{self, Write};

use clap::CommandFactory;
use clap_mangen::Man;

/// The kind of document a failing stdout names on stderr.
const DOCUMENT: &str = "man page";

/// The separator between the command words in a page's own name.
const NAME_SEPARATOR: &str = "-";

use crate::stdout;

/// Write the man page for the whole command tree and return the exit code.
pub fn emit<Parser: CommandFactory>() -> i32 {
    stdout::emit(DOCUMENT, |out| page::<Parser>(out))
}

/// Render the whole tree as one roff stream.
fn page<Parser: CommandFactory>(out: &mut dyn Write) -> io::Result<()> {
    let root = Parser::command();
    Man::new(root.clone()).render(out)?;
    for command in tree(&root, root.get_name()) {
        Man::new(command).render(out)?;
    }
    Ok(())
}

/// Every subcommand below `command`, in the order the definition declares
/// them, each renamed to the full path a manual page for it carries.
///
/// `invocation` is how the parent is invoked, `openpapir archive` for one, so
/// a page's synopsis reads as the command a user types while its own name is
/// the dashed form a manual page is filed under. Both are built from names
/// this build defines and never from a value the caller supplied.
fn tree(command: &clap::Command, invocation: &str) -> Vec<clap::Command> {
    let mut pages = Vec::new();
    for sub in command.get_subcommands().filter(|sub| !sub.is_hide_set()) {
        let invoked = format!("{invocation} {}", sub.get_name());
        let filed = invoked.replace(' ', NAME_SEPARATOR);
        pages.push(
            sub.clone()
                .name(filed.clone())
                .display_name(filed)
                .bin_name(invoked.clone()),
        );
        pages.extend(tree(sub, &invoked));
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::{page, tree};
    use clap::CommandFactory;

    /// The command definition this crate's parser derives.
    type Parser = crate::Args;

    /// Render the whole stream as text.
    fn rendered() -> String {
        let mut written = Vec::new();
        page::<Parser>(&mut written).expect("a vector takes every byte");
        String::from_utf8(written).expect("a generated man page is UTF-8")
    }

    /// Every command in the tree, as the dashed name its page is filed under.
    fn filed_names() -> Vec<String> {
        tree(&Parser::command(), "openpapir")
            .iter()
            .map(|command| command.get_name().to_owned())
            .collect()
    }

    /// The stream names every subcommand at every depth, so a command added
    /// later cannot quietly stay undocumented.
    #[test]
    fn the_stream_holds_a_page_for_every_command_in_the_tree() {
        let text = rendered();
        let names = filed_names();
        assert!(
            names.iter().any(|name| name == "openpapir-archive-init"),
            "a nested command is filed under its whole path"
        );
        for name in &names {
            assert!(text.contains(name), "the stream omits {name}");
        }
        assert_eq!(
            text.matches("\n.TH").count() + usize::from(text.starts_with(".TH")),
            names.len() + 1,
            "one title line for the root page and one for each command"
        );
    }

    /// The stream is a manual page: it starts with a title line, and it
    /// carries the binary's own description rather than a build path.
    #[test]
    fn the_stream_is_roff_and_carries_no_build_path() {
        let text = rendered();
        assert!(
            text.contains("\n.TH openpapir 1"),
            "the first page is the binary's own"
        );
        assert!(text.contains(".SH SYNOPSIS"), "a page carries its sections");
        assert!(
            !text.contains(env!("CARGO_MANIFEST_DIR")),
            "the page carries a build path"
        );
    }
}
