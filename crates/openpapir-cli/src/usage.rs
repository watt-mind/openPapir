//! What an argument-parser failure looks like to a machine caller.
//!
//! Without `--json` the parser prints its own usage text, exactly as it did
//! before. With `--json` the same failure is one `usage.arguments` envelope on
//! stdout and nothing on stderr, so a caller that asked for JSON never has to
//! parse usage text to learn why an invocation was refused. Both forms exit
//! `2`, the bucket's exit code.
//!
//! `--json` is found in the raw arguments, because the parse that would have
//! reported the flag is the parse that failed. `--help` and `--version` are
//! not failures: the parser renders them itself and the process exits `0`.

use std::collections::BTreeSet;
use std::ffi::OsString;

use clap::CommandFactory;
use clap::error::{ContextKind, ContextValue, ErrorKind};
use openpapir_core::error::{Details, Diagnostic, codes};

use crate::envelope;

/// The token that asks for the JSON form, and the separator that ends flags.
const JSON_FLAG: &str = "--json";
/// Everything after this token is a positional value, never a flag.
const END_OF_FLAGS: &str = "--";
/// The envelope's `command` when no subcommand was recognised at all.
const PROGRAM: &str = "openpapir";

/// Print an argument-parser failure and return the process exit code.
///
/// `Parser` is the argument parser whose `try_parse` failed; its command
/// definition supplies the only names this function is willing to echo.
pub fn report<Parser: CommandFactory>(error: &clap::Error, arguments: &[OsString]) -> i32 {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    ) {
        let _ = error.print();
        return 0;
    }
    if !json_requested(arguments) {
        let _ = error.print();
        return 2;
    }
    let command = Parser::command();
    let recognised = recognise(&command, arguments);
    let refusal = Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The command line is malformed or a value is unusable.",
        argument_detail(error, &known_names(&recognised.chain)),
    );
    println!(
        "{}",
        envelope::failure(&recognised.command_path(), &refusal, &[])
    );
    2
}

/// Whether the raw arguments asked for the JSON form.
///
/// The scan stops at `--`, after which a token is a positional value the user
/// supplied and never a flag openPapir defines.
fn json_requested(arguments: &[OsString]) -> bool {
    arguments
        .iter()
        .take_while(|argument| argument.as_os_str() != END_OF_FLAGS)
        .any(|argument| argument.as_os_str() == JSON_FLAG)
}

/// The command the arguments named, as far as the walk recognised it.
struct Recognised<'a> {
    /// The commands walked through, the program first and the deepest last.
    chain: Vec<&'a clap::Command>,
}

impl Recognised<'_> {
    /// The dotted path of the recognised subcommands, or the program itself.
    ///
    /// Only names this build defines are used, so the value is never
    /// user-supplied text.
    fn command_path(&self) -> String {
        let path: Vec<&str> = self
            .chain
            .iter()
            .skip(1)
            .map(|command| command.get_name())
            .collect();
        if path.is_empty() {
            PROGRAM.to_owned()
        } else {
            path.join(".")
        }
    }
}

/// Walk the raw arguments as the parser would, as far as they are recognised.
///
/// A flag's value is consumed with the flag, so a value that happens to spell
/// a subcommand name, `--title list` for one, is a value and never a
/// subcommand. The walk stops at the first token that is neither a flag this
/// command defines nor one of its subcommands, and at `--`, after which every
/// token is a positional value the user supplied.
fn recognise<'a>(command: &'a clap::Command, arguments: &[OsString]) -> Recognised<'a> {
    let mut chain = vec![command];
    let mut current = command;
    let mut tokens = arguments.iter();
    while let Some(argument) = tokens.next() {
        let Some(token) = argument.to_str() else {
            break;
        };
        if token == END_OF_FLAGS {
            break;
        }
        if let Some(long) = token.strip_prefix("--") {
            if !long.contains('=') && long_takes_a_value(current, long) {
                tokens.next();
            }
            continue;
        }
        if let Some(shorts) = token.strip_prefix('-') {
            if !shorts.is_empty() && short_takes_the_next_token(current, shorts) {
                tokens.next();
            }
            continue;
        }
        let Some(next) = current
            .get_subcommands()
            .find(|candidate| candidate.get_name() == token)
        else {
            break;
        };
        chain.push(next);
        current = next;
    }
    Recognised { chain }
}

/// Whether a long flag this command defines takes a separate value.
fn long_takes_a_value(command: &clap::Command, long: &str) -> bool {
    command
        .get_arguments()
        .find(|argument| argument.get_long() == Some(long))
        .is_some_and(|argument| argument.get_action().takes_values())
}

/// Whether a cluster of short flags ends in one whose value is the next token.
///
/// A value written inside the cluster, `-nvalue`, is already the value, so
/// only a cluster whose value-taking flag is its last character reaches
/// forward. This build defines no short flag, so the rule exists to keep the
/// walk correct rather than to serve one today.
fn short_takes_the_next_token(command: &clap::Command, shorts: &str) -> bool {
    let mut remaining = shorts.chars();
    while let Some(short) = remaining.next() {
        let takes_value = command
            .get_arguments()
            .find(|argument| argument.get_short() == Some(short))
            .is_some_and(|argument| argument.get_action().takes_values());
        if takes_value {
            return remaining.next().is_none();
        }
    }
    false
}

/// Every flag and value name the recognised command and its parents define.
///
/// The allowlist is the walked chain and nothing else, so a flag another
/// subcommand defines is as unknown here as a token the user invented.
fn known_names(chain: &[&clap::Command]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for command in chain {
        for argument in command.get_arguments() {
            if let Some(long) = argument.get_long() {
                names.insert(normalise(long));
            }
            names.insert(normalise(argument.get_id().as_str()));
            for value_name in argument.get_value_names().unwrap_or_default() {
                names.insert(normalise(value_name.as_str()));
            }
        }
    }
    names
}

/// The `argument` key, when the parser named one this build defines.
///
/// An unrecognised token is text the user typed, which may be a path, so it is
/// never echoed: the refusal then carries its bucket alone. The name is
/// reported without its dashes or angle brackets, the same form the library's
/// own `usage.arguments` refusals use.
fn argument_detail(error: &clap::Error, known: &BTreeSet<String>) -> Details {
    let Some(value) = error.get(ContextKind::InvalidArg) else {
        return Details::new();
    };
    let candidate = match value {
        ContextValue::String(single) => Some(single.clone()),
        ContextValue::Strings(many) => many.first().cloned(),
        _ => None,
    };
    candidate
        .as_deref()
        .map(normalise)
        .filter(|name| known.contains(name))
        .map_or_else(Details::new, |name| Details::new().text("argument", name))
}

/// Reduce a parser's rendering of an argument to its bare name.
///
/// The parser writes an argument as `--archive <ROOT>`, `<FILE>...`, or
/// `--json`, so the first whitespace-separated token is taken and its dashes,
/// angle brackets, and trailing dots removed.
fn normalise(rendered: &str) -> String {
    rendered
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|character: char| matches!(character, '-' | '<' | '>' | '.' | '='))
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rendered_argument_is_reduced_to_its_bare_name() {
        assert_eq!(normalise("--archive <ROOT>"), "archive");
        assert_eq!(normalise("<FILE>..."), "file");
        assert_eq!(normalise("--json"), "json");
        assert_eq!(normalise(""), "");
    }

    /// A flag's value is consumed with the flag, so a value that spells a
    /// subcommand name is a value and never the command that gets named.
    #[test]
    fn a_flag_value_that_spells_a_subcommand_is_not_taken_for_one() {
        let command = command();
        assert_eq!(
            path(&command, &["case", "create", "--title", "list"]),
            "case.create"
        );
        assert_eq!(path(&command, &["case", "list"]), "case.list");
        assert_eq!(
            path(&command, &["case", "create", "--title=list"]),
            "case.create"
        );
        assert_eq!(path(&command, &["import"]), "import");
        assert_eq!(path(&command, &[]), PROGRAM);
        assert_eq!(path(&command, &["bogus", "case"]), PROGRAM);
        // Everything after the separator is a positional the user supplied.
        assert_eq!(path(&command, &["--", "case", "list"]), PROGRAM);
    }

    /// The allowlist is the recognised command and its parents, so a flag
    /// another subcommand defines is as unknown as an invented token.
    #[test]
    fn the_allowlist_holds_only_the_recognised_commands_own_names() {
        let command = command();
        let capabilities = known_names(&recognise(&command, &tokens(&["capabilities"])).chain);
        assert!(capabilities.contains("json"));
        assert!(
            !capabilities.contains("archive"),
            "a flag only `import` defines is not `capabilities`' to echo"
        );
        let import = known_names(&recognise(&command, &tokens(&["import"])).chain);
        assert!(import.contains("archive") && import.contains("file"));
        assert!(
            !import.contains("title"),
            "`--title` belongs to `case create`"
        );
        let root = known_names(&recognise(&command, &tokens(&["bogus"])).chain);
        assert!(!root.contains("archive") && !root.contains("json"));
    }

    /// No short flag is defined today, so the cluster rule is checked against
    /// the definition itself rather than against an invocation.
    #[test]
    fn a_short_flag_cluster_reaches_forward_only_for_a_trailing_value() {
        let command = command();
        assert!(!short_takes_the_next_token(&command, "x"));
        assert!(!long_takes_a_value(&command, "nonexistent"));
        let import = command
            .get_subcommands()
            .find(|candidate| candidate.get_name() == "import")
            .unwrap();
        assert!(long_takes_a_value(import, "archive"));
        assert!(!long_takes_a_value(import, "json"), "a flag takes no value");
    }

    fn command() -> clap::Command {
        clap::Command::new(PROGRAM)
            .subcommand(clap::Command::new("capabilities").arg(json_flag()))
            .subcommand(
                clap::Command::new("case")
                    .subcommand(
                        clap::Command::new("create")
                            .arg(clap::Arg::new("title").long("title").value_name("TITLE"))
                            .arg(json_flag()),
                    )
                    .subcommand(clap::Command::new("list").arg(json_flag())),
            )
            .subcommand(
                clap::Command::new("import")
                    .arg(clap::Arg::new("archive").long("archive").value_name("ROOT"))
                    .arg(clap::Arg::new("files").value_name("FILE"))
                    .arg(json_flag()),
            )
    }

    fn json_flag() -> clap::Arg {
        clap::Arg::new("json")
            .long("json")
            .action(clap::ArgAction::SetTrue)
    }

    fn tokens(raw: &[&str]) -> Vec<OsString> {
        raw.iter().map(OsString::from).collect()
    }

    fn path(command: &clap::Command, raw: &[&str]) -> String {
        recognise(command, &tokens(raw)).command_path()
    }

    #[test]
    fn the_json_form_is_recognised_before_the_parse_and_not_after_a_separator() {
        let arguments =
            |tokens: &[&str]| -> Vec<OsString> { tokens.iter().map(OsString::from).collect() };
        assert!(json_requested(&arguments(&["import", "--json"])));
        assert!(!json_requested(&arguments(&["import"])));
        assert!(!json_requested(&arguments(&["import", "--", "--json"])));
    }
}
