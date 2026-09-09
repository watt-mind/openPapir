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
    let refusal = Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        "The command line is malformed or a value is unusable.",
        argument_detail(error, &known_names(&command)),
    );
    println!(
        "{}",
        envelope::failure(&command_path(&command, arguments), &refusal, &[])
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

/// The dotted command path the arguments name, as far as it was recognised.
///
/// Only names this build defines are used, so the value is never
/// user-supplied text. An invocation that named no subcommand at all reports
/// the program itself.
fn command_path(command: &clap::Command, arguments: &[OsString]) -> String {
    let mut current = command;
    let mut path: Vec<&str> = Vec::new();
    for argument in arguments {
        let Some(token) = argument.to_str() else {
            break;
        };
        if token.starts_with('-') {
            continue;
        }
        let Some(next) = current
            .get_subcommands()
            .find(|candidate| candidate.get_name() == token)
        else {
            break;
        };
        path.push(next.get_name());
        current = next;
    }
    if path.is_empty() {
        PROGRAM.to_owned()
    } else {
        path.join(".")
    }
}

/// Every flag and value name this build defines, normalised.
fn known_names(command: &clap::Command) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    collect_names(command, &mut names);
    names
}

fn collect_names(command: &clap::Command, names: &mut BTreeSet<String>) {
    for argument in command.get_arguments() {
        if let Some(long) = argument.get_long() {
            names.insert(normalise(long));
        }
        names.insert(normalise(argument.get_id().as_str()));
        for value_name in argument.get_value_names().unwrap_or_default() {
            names.insert(normalise(value_name.as_str()));
        }
    }
    for subcommand in command.get_subcommands() {
        collect_names(subcommand, names);
    }
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

    #[test]
    fn the_json_form_is_recognised_before_the_parse_and_not_after_a_separator() {
        let arguments =
            |tokens: &[&str]| -> Vec<OsString> { tokens.iter().map(OsString::from).collect() };
        assert!(json_requested(&arguments(&["import", "--json"])));
        assert!(!json_requested(&arguments(&["import"])));
        assert!(!json_requested(&arguments(&["import", "--", "--json"])));
    }
}
