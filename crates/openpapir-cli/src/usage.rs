//! What an argument-parser failure looks like to a machine caller.
//!
//! Without `--json` the parser prints its own usage text. With `--json` the
//! same failure is one `usage.arguments` envelope on stdout and nothing on
//! stderr, so a caller that asked for JSON never has to parse usage text to
//! learn why an invocation was refused. Both forms exit `2`, the bucket's
//! exit code.
//!
//! One rejection is answered rather than only reported: a long flag written
//! before the subcommand that takes it, `openpapir --archive <root> case
//! list` for one, where the parser can say no more than that the flag was
//! unexpected. Both forms then add the sentence that says where the flag
//! belongs, and the JSON form carries it as `details.placement` as well. Only
//! the flag's own name is named; the value beside it is text the caller
//! typed and is never echoed.
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
/// The `details.placement` value for a flag written before its subcommand.
const AFTER_SUBCOMMAND: &str = "after_subcommand";
/// The sentence both forms use when a flag was written too early.
const MISPLACED_MESSAGE: &str = "The argument belongs after the subcommand, not before it. Write the \
     subcommand first and the flag after it.";
/// The sentence every other parser rejection carries.
const MALFORMED_MESSAGE: &str = "The command line is malformed or a value is unusable.";

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
    let command = Parser::command();
    let recognised = recognise(&command, arguments);
    let rejected = classify(error, &recognised, arguments);
    if !json_requested(arguments) {
        let _ = error.print();
        if let Rejected::Misplaced(name) = &rejected {
            eprintln!("note: --{name}: {MISPLACED_MESSAGE}");
        }
        return 2;
    }
    let refusal = Diagnostic::new(
        codes::USAGE_ARGUMENTS,
        match rejected {
            Rejected::Misplaced(_) => MISPLACED_MESSAGE,
            _ => MALFORMED_MESSAGE,
        },
        rejected.into_details(),
    );
    println!(
        "{}",
        envelope::failure(&recognised.command_path(), &refusal, &[])
    );
    2
}

/// What the parser's rejected argument turned out to be.
enum Rejected {
    /// The parser named nothing, or named text the caller invented.
    Unnamed,
    /// A name the recognised command or one of its parents defines.
    Known(String),
    /// A long flag no recognised command defines but one below them does,
    /// which is the flag-order mistake rather than an unknown flag.
    Misplaced(String),
}

impl Rejected {
    /// The `details` the refusal carries.
    ///
    /// Every name here is read from the command definition, so `details`
    /// holds no text the caller typed even when the caller's spelling is what
    /// found it.
    fn into_details(self) -> Details {
        match self {
            Self::Unnamed => Details::new(),
            Self::Known(name) => Details::new().text("argument", name),
            Self::Misplaced(name) => Details::new()
                .text("argument", name)
                .text("placement", AFTER_SUBCOMMAND),
        }
    }
}

/// Decide what the parser rejected, using only names this build defines.
///
/// An unrecognised token is text the user typed, which may be a path, so it is
/// never echoed. A token that spells a long flag defined below the recognised
/// command is not such text: it matched a name in the definition exactly, and
/// naming it is the whole point of the steer.
fn classify(error: &clap::Error, recognised: &Recognised, arguments: &[OsString]) -> Rejected {
    let Some(candidate) = invalid_argument(error) else {
        return Rejected::Unnamed;
    };
    let name = normalise(&candidate);
    if misplaced(error, recognised, arguments, &candidate, &name) {
        return Rejected::Misplaced(name);
    }
    if known_names(&recognised.chain).contains(&name) {
        return Rejected::Known(name);
    }
    Rejected::Unnamed
}

/// Whether the rejection is a long flag written ahead of its subcommand.
///
/// Two shapes count, and both need the parser to have called the token
/// unexpected rather than missing or unusable. Either the flag was written
/// before the subcommand the walk went on to recognise, `--json capabilities`
/// for one, and that command defines it; or no recognised command defines it
/// while a command below the deepest one does, which is `--archive <root>
/// case list`. A flag written in the right place and still refused is not
/// this: it stays the unknown argument it always was.
fn misplaced(
    error: &clap::Error,
    recognised: &Recognised,
    arguments: &[OsString],
    candidate: &str,
    name: &str,
) -> bool {
    if error.kind() != ErrorKind::UnknownArgument || !candidate.starts_with("--") {
        return false;
    }
    let Some(deepest) = recognised.chain.last().copied() else {
        return false;
    };
    if declared_below(deepest, name) {
        return true;
    }
    written_before_the_subcommand(arguments, recognised.boundary, name)
        && deepest.get_arguments().any(|argument| {
            argument
                .get_long()
                .is_some_and(|long| normalise(long) == name)
        })
}

/// Whether a long flag of this bare name was written before the subcommand.
///
/// The scan stops at `--`, after which a token is a value the caller supplied,
/// and it compares bare names only, so `--archive=<root>` is the same flag as
/// `--archive <root>`.
fn written_before_the_subcommand(arguments: &[OsString], boundary: usize, name: &str) -> bool {
    arguments
        .iter()
        .take(boundary)
        .map_while(|argument| argument.to_str())
        .take_while(|token| *token != END_OF_FLAGS)
        .filter_map(|token| token.strip_prefix("--"))
        .any(|long| normalise(long.split('=').next().unwrap_or(long)) == name)
}

/// The argument the parser named, as the parser rendered it.
fn invalid_argument(error: &clap::Error) -> Option<String> {
    match error.get(ContextKind::InvalidArg)? {
        ContextValue::String(single) => Some(single.clone()),
        ContextValue::Strings(many) => many.first().cloned(),
        _ => None,
    }
}

/// Whether a command below this one defines a long flag of this bare name.
///
/// The search is the whole subtree, because `--archive` is defined by the
/// leaves of `archive` and `case` rather than by those groups themselves, and
/// a caller who wrote it too early is as likely to have written it before the
/// group as before the leaf.
fn declared_below(command: &clap::Command, name: &str) -> bool {
    command.get_subcommands().any(|subcommand| {
        subcommand.get_arguments().any(|argument| {
            argument
                .get_long()
                .is_some_and(|long| normalise(long) == name)
        }) || declared_below(subcommand, name)
    })
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
    /// Where the first subcommand was found, or the number of arguments when
    /// none was. Every token before it was written ahead of the subcommand.
    boundary: usize,
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
    let mut boundary = arguments.len();
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        let Some(token) = argument.to_str() else {
            break;
        };
        if token == END_OF_FLAGS {
            break;
        }
        index += 1;
        if let Some(long) = token.strip_prefix("--") {
            if !long.contains('=') && long_takes_a_value(current, long) {
                index += 1;
            }
            continue;
        }
        if let Some(shorts) = token.strip_prefix('-') {
            if !shorts.is_empty() && short_takes_the_next_token(current, shorts) {
                index += 1;
            }
            continue;
        }
        let Some(next) = current
            .get_subcommands()
            .find(|candidate| candidate.get_name() == token)
        else {
            break;
        };
        if chain.len() == 1 {
            boundary = index - 1;
        }
        chain.push(next);
        current = next;
    }
    Recognised { chain, boundary }
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

    /// A name a command below this one defines is a name this build declares,
    /// which is what makes it safe to name in the steer.
    #[test]
    fn a_flag_a_command_below_defines_is_found_and_an_invented_one_is_not() {
        let command = command();
        assert!(declared_below(&command, "archive"));
        assert!(declared_below(&command, "json"));
        assert!(declared_below(&command, "title"), "two levels down");
        assert!(!declared_below(&command, "qzmarker"));
        let import = command
            .get_subcommands()
            .find(|candidate| candidate.get_name() == "import")
            .unwrap();
        assert!(
            !declared_below(import, "archive"),
            "a leaf's own flag is not below it"
        );
    }

    /// The boundary is where the first subcommand was found, so a flag before
    /// it was written too early even when the walk went on to recognise the
    /// command that defines it.
    #[test]
    fn the_boundary_separates_what_was_written_before_the_subcommand() {
        let command = command();
        let boundary = |raw: &[&str]| recognise(&command, &tokens(raw)).boundary;
        assert_eq!(boundary(&["--json", "capabilities"]), 1);
        assert_eq!(boundary(&["capabilities", "--json"]), 0);
        assert_eq!(boundary(&["case", "list"]), 0);
        // No subcommand at all leaves every token ahead of the boundary.
        assert_eq!(boundary(&["--archive", "root"]), 2);

        let written = |raw: &[&str], name: &str| {
            let arguments = tokens(raw);
            written_before_the_subcommand(
                &arguments,
                recognise(&command, &arguments).boundary,
                name,
            )
        };
        assert!(written(&["--json", "capabilities"], "json"));
        assert!(!written(&["capabilities", "--json"], "json"));
        assert!(written(&["--archive=root", "case", "list"], "archive"));
        assert!(!written(&["--", "--json", "capabilities"], "json"));
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
