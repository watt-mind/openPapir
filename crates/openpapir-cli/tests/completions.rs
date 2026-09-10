//! `openpapir completions <shell>` and `openpapir manpage` against the
//! binary itself.
//!
//! Both write a generated document to stdout and nothing else. Neither is
//! pinned byte for byte: the exact script and the exact roff move with the
//! generator's version, and pinning them would turn a dependency bump into a
//! contract change. What is pinned is what a user relies on: every subcommand
//! this build defines is completable in every shell offered, and every
//! subcommand has a page in the man stream.

use std::process::Command;

/// Every shell `completions` offers, in the order the parser lists them.
const SHELLS: [&str; 5] = ["bash", "elvish", "fish", "powershell", "zsh"];

/// Run the binary under test and return its output.
fn run(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_openpapir"))
        .args(arguments)
        .output()
        .expect("run the openpapir binary under test")
}

/// The stdout of a successful run, as text.
fn document(arguments: &[&str]) -> String {
    let output = run(arguments);
    assert_eq!(output.status.code(), Some(0), "{arguments:?} succeeds");
    assert!(output.stderr.is_empty(), "the document is the whole output");
    String::from_utf8(output.stdout).expect("a generated document is UTF-8")
}

/// Every command path this build defines, deepest last, as the words a user
/// types after `openpapir`.
///
/// The list is read from the binary's own help output rather than written
/// out here, so a command added later is checked without this file changing.
fn command_paths() -> Vec<Vec<String>> {
    let mut paths = Vec::new();
    let mut pending = vec![Vec::new()];
    while let Some(path) = pending.pop() {
        let mut arguments: Vec<&str> = path.iter().map(String::as_str).collect();
        arguments.push("--help");
        let help = run(&arguments);
        assert_eq!(help.status.code(), Some(0), "{path:?} has help");
        let text = String::from_utf8(help.stdout).expect("help is UTF-8");
        for name in subcommand_names(&text) {
            let mut child = path.clone();
            child.push(name);
            paths.push(child.clone());
            pending.push(child);
        }
    }
    assert!(!paths.is_empty(), "the binary defines subcommands");
    paths
}

/// The subcommand names one help text lists, which are the first word of each
/// line of its Commands section.
fn subcommand_names(help: &str) -> Vec<String> {
    let Some(section) = help.split_once("Commands:\n") else {
        return Vec::new();
    };
    section
        .1
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
        .filter(|word| *word != "help")
        .map(str::to_owned)
        .collect()
}

/// Every shell's script names every command this build defines, so a user who
/// installed the script can complete the whole tool.
#[test]
fn every_shell_script_completes_every_command() {
    let paths = command_paths();
    for shell in SHELLS {
        let script = document(&["completions", shell]);
        assert!(!script.is_empty(), "{shell} produced nothing");
        assert!(script.contains("openpapir"), "{shell} omits the binary");
        for path in &paths {
            let name = path.last().expect("a path has a last word");
            assert!(script.contains(name.as_str()), "{shell} omits {name}");
        }
    }
}

/// The man stream holds a page for the binary and one for every command,
/// filed under the dashed name a manual page for it carries.
#[test]
fn the_man_stream_holds_a_page_for_every_command() {
    let page = document(&["manpage"]);
    assert!(page.contains(".TH openpapir 1"), "the binary's own page");
    for path in command_paths() {
        let filed = format!("openpapir-{}", path.join("-"));
        assert!(page.contains(&filed), "the stream omits {filed}");
    }
}

/// Neither command takes a `--json` form, and a shell this build cannot
/// generate for is a usage refusal rather than an empty script.
#[test]
fn neither_command_takes_a_json_form_and_an_unknown_shell_is_refused() {
    for arguments in [
        &["completions", "bash", "--json"][..],
        &["manpage", "--json"][..],
    ] {
        let output = run(arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?} is refused");
        assert!(output.stderr.is_empty(), "a JSON caller reads an envelope");
        let envelope: serde_json::Value =
            serde_json::from_str(String::from_utf8(output.stdout).unwrap().trim())
                .expect("the refusal is one JSON object");
        assert_eq!(envelope["error"]["code"], "usage.arguments");
        assert_eq!(envelope["ok"], false);
        assert_eq!(envelope["data"], serde_json::json!({}));
    }

    let unknown = run(&["completions", "korn"]);
    assert_eq!(unknown.status.code(), Some(2), "an unknown shell is usage");
    assert!(unknown.stdout.is_empty(), "no script is written");
    let text = String::from_utf8(unknown.stderr).expect("the parser explains itself");
    assert!(text.contains("SHELL"), "the refusal names the argument");
    for shell in SHELLS {
        assert!(text.contains(shell), "the refusal lists {shell}");
    }
}

/// A reader that closed the pipe asked the command to stop, so the run still
/// reports success, exactly as `skill` does.
#[test]
#[cfg(unix)]
fn a_closed_reader_leaves_the_exit_code_at_zero() {
    use std::io::Read;
    use std::process::Stdio;

    for arguments in [&["completions", "bash"][..], &["manpage"][..]] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_openpapir"))
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run the openpapir binary under test");
        let mut stdout = child.stdout.take().expect("the child's stdout is piped");
        let mut first = [0_u8; 1];
        stdout.read_exact(&mut first).expect("read the first byte");
        drop(stdout);
        let output = child.wait_with_output().expect("wait for the child");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{arguments:?} treats a closed reader as the reader's own choice"
        );
        assert!(output.stderr.is_empty(), "nothing is explained on stderr");
    }
}
