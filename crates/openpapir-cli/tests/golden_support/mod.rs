//! The synthetic world the golden cases run against, and the normalisation
//! that makes one run comparable with the next.
//!
//! Nothing here reads a fixture from disk. Every byte an archive holds comes
//! from the constants below, so a golden file describes bytes this repository
//! can reproduce from its own source and from nothing else.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub mod compare;

/// The three synthetic payloads every archive is built from.
const ALPHA: &[u8] = b"synthetic submission alpha\n";
const BETA: &[u8] = b"synthetic receipt beta\n";
const GAMMA: &[u8] = b"synthetic attachment gamma\n";

/// The bytes one stored object is replaced with to damage an archive.
///
/// They are exactly as long as [`ALPHA`], so the archive check reports a
/// digest mismatch and nothing else, which is the condition being pinned.
const DAMAGED: &[u8] = b"synthetic submission ALPHA\n";

/// The number of inputs one import refuses, which is the cap plus one.
const OVER_THE_IMPORT_FILE_CAP: usize = 1001;

/// The synthetic payloads, in the order they are imported.
#[must_use]
pub fn payloads() -> [&'static [u8]; 3] {
    [ALPHA, BETA, GAMMA]
}

/// How far a case's archive is built before the pinned invocation runs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    /// A directory that is not an archive yet.
    Empty,
    /// An archive with nothing in it.
    Initialised,
    /// The three payloads imported.
    Imported,
    /// One case recorded.
    Cased,
    /// One submission recorded against the case, and optionally a second.
    Submitted,
    /// One receipt recorded.
    Receipted,
    /// Two associations recorded, the second superseding the first.
    Associated,
}

/// One run's two streams and its exit code, already normalised.
pub struct Captured {
    /// Normalised stdout: the pretty-printed envelope, or the human lines.
    pub stdout: String,
    /// Normalised stderr.
    pub stderr: String,
    /// The exit code, on its own line.
    pub exit: String,
}

/// A synthetic archive and everything a case needs to name what is in it.
pub struct World {
    directory: TempDir,
    archive: PathBuf,
    inputs: Vec<PathBuf>,
    /// The stored artefacts, in the order they were imported.
    pub digests: Vec<String>,
    /// The one case every record hangs from.
    pub case_id: String,
    /// The submissions recorded against the case, in the order they were made.
    pub submission_ids: Vec<String>,
    /// The one receipt the associations are about.
    pub receipt_id: String,
    /// The associations recorded against it, oldest first.
    pub association_ids: Vec<String>,
}

impl World {
    /// Build a synthetic archive up to `stage`.
    #[must_use]
    pub fn build(stage: Stage, second_submission: bool) -> Self {
        let directory = tempfile::tempdir().expect("create the temporary root");
        let archive = directory.path().join("archive");
        fs::create_dir(&archive).expect("create the archive directory");
        let inputs = write_inputs(directory.path());
        let mut world = Self {
            directory,
            archive,
            inputs,
            digests: Vec::new(),
            case_id: String::new(),
            submission_ids: Vec::new(),
            receipt_id: String::new(),
            association_ids: Vec::new(),
        };
        world.fill(stage, second_submission);
        world
    }

    /// Run every setup step the stage asks for, in a fixed order.
    fn fill(&mut self, stage: Stage, second_submission: bool) {
        if stage < Stage::Initialised {
            return;
        }
        self.json(&[
            "archive".to_owned(),
            "init".to_owned(),
            self.archive_string(),
        ]);
        if stage < Stage::Imported {
            return;
        }
        self.import_inputs();
        if stage < Stage::Cased {
            return;
        }
        let created = self.json(&self.case_create_arguments());
        self.case_id = text(&created["data"]["case"]["id"]);
        if stage < Stage::Submitted {
            return;
        }
        self.add_submission(0);
        if second_submission {
            self.add_submission(1);
        }
        if stage < Stage::Receipted {
            return;
        }
        let added = self.json(&self.receipt_arguments());
        self.receipt_id = text(&added["data"]["receipt"]["id"]);
        if stage < Stage::Associated {
            return;
        }
        self.add_associations();
    }

    /// Import the three payloads and keep their digests in input order.
    fn import_inputs(&mut self) {
        let mut arguments = self.import_arguments();
        arguments.extend(self.input_strings());
        let imported = self.json(&arguments);
        self.digests = imported["data"]["artefacts"]
            .as_array()
            .expect("the import reports its artefacts")
            .iter()
            .map(|artefact| text(&artefact["digest"]))
            .collect();
    }

    /// Record one submission and keep its identifier.
    fn add_submission(&mut self, index: usize) {
        let added = self.json(&self.submission_arguments(index));
        self.submission_ids
            .push(text(&added["data"]["submission"]["id"]));
    }

    /// Record the two associations the history case lists.
    ///
    /// The second supersedes the first, so the history has one fixed order
    /// whether or not the two records share a recorded second.
    fn add_associations(&mut self) {
        let first = self.json(&self.association_arguments("unassociated", &[]));
        let first_id = text(&first["data"]["association"]["id"]);
        let mut arguments = self.association_arguments(
            "candidate",
            &[(0, "moderate", "The reference matches the submission.")],
        );
        arguments.extend(["--supersedes".to_owned(), first_id.clone()]);
        let second = self.json(&arguments);
        self.association_ids = vec![first_id, text(&second["data"]["association"]["id"])];
    }

    /// Run one invocation in its JSON form and return the parsed envelope.
    fn json(&self, arguments: &[String]) -> serde_json::Value {
        let mut arguments = arguments.to_vec();
        arguments.push("--json".to_owned());
        let output = self.run(&arguments);
        serde_json::from_slice(&output.stdout).expect("a setup step prints one envelope")
    }

    /// Run the binary under test with the supplied arguments.
    fn run(&self, arguments: &[String]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_openpapir"))
            .args(arguments)
            .output()
            .expect("run the openpapir binary under test")
    }

    /// The temporary root every path in this world lies under.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.directory.path()
    }

    /// The archive root as the command line spells it.
    #[must_use]
    pub fn archive_string(&self) -> String {
        self.archive.to_string_lossy().into_owned()
    }

    /// The three synthetic inputs as the command line spells them.
    #[must_use]
    pub fn input_strings(&self) -> Vec<String> {
        self.inputs
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    /// The destination `case export` writes to, which it creates itself.
    #[must_use]
    pub fn export_destination(&self) -> String {
        self.root().join("export").to_string_lossy().into_owned()
    }

    /// `import --archive <root>`, without the files.
    #[must_use]
    pub fn import_arguments(&self) -> Vec<String> {
        vec![
            "import".to_owned(),
            "--archive".to_owned(),
            self.archive_string(),
        ]
    }

    /// One command against this archive, with `--archive` already supplied.
    #[must_use]
    pub fn command(&self, parts: &[&str]) -> Vec<String> {
        let mut arguments: Vec<String> = parts.iter().map(|part| (*part).to_owned()).collect();
        arguments.extend(["--archive".to_owned(), self.archive_string()]);
        arguments
    }

    /// The one case every world above [`Stage::Cased`] holds.
    #[must_use]
    pub fn case_create_arguments(&self) -> Vec<String> {
        let mut arguments = self.command(&["case", "create"]);
        arguments.extend([
            "--title".to_owned(),
            "Tax matter".to_owned(),
            "--notes".to_owned(),
            "First contact with the office.".to_owned(),
        ]);
        arguments
    }

    /// The submission at `index`, which is fixed text over a fixed artefact.
    #[must_use]
    pub fn submission_arguments(&self, index: usize) -> Vec<String> {
        let mut arguments = self.command(&["submission", "add"]);
        arguments.extend(["--case".to_owned(), self.case_id.clone()]);
        if index == 0 {
            arguments.extend([
                "--description".to_owned(),
                "Posted the completed form.".to_owned(),
                "--date".to_owned(),
                "2026-01-13".to_owned(),
                "--artefact".to_owned(),
                format!("{}:cover letter", self.digests[0]),
            ]);
        } else {
            arguments.extend([
                "--description".to_owned(),
                "Sent the supporting evidence.".to_owned(),
                "--artefact".to_owned(),
                self.digests[2].clone(),
            ]);
        }
        arguments
    }

    /// Record one more submission with a date, for the summary's window
    /// cases. The date is the user's own text and is stored verbatim.
    pub fn add_dated_submission(&self, description: &str, date: &str) {
        let mut arguments = self.command(&["submission", "add"]);
        arguments.extend([
            "--case".to_owned(),
            self.case_id.clone(),
            "--description".to_owned(),
            description.to_owned(),
            "--date".to_owned(),
            date.to_owned(),
        ]);
        self.json(&arguments);
    }

    /// Record a second case and close it, so the summary's status breakdown
    /// pins a count on both sides rather than a zero on one.
    pub fn add_closed_case(&self) {
        let mut create = self.command(&["case", "create"]);
        create.extend(["--title".to_owned(), "Parking notice".to_owned()]);
        let created = self.json(&create);
        let id = text(&created["data"]["case"]["id"]);
        let mut update = self.command(&["case", "update"]);
        update.extend([id, "--status".to_owned(), "closed".to_owned()]);
        self.json(&update);
    }

    /// Retire the receipt's live association, withdrawing what it asserted.
    ///
    /// The retired record stays exactly where it is; only the head of the
    /// chain changes, which is how a user withdraws an assertion.
    pub fn retire_live_association(&self) {
        let mut listing = self.command(&["association", "list"]);
        listing.extend(["--receipt".to_owned(), self.receipt_id.clone()]);
        let history = self.json(&listing);
        let live = text(&history["data"]["associations"][0]["id"]);
        let mut arguments = self.command(&["association", "retire"]);
        arguments.extend([
            live,
            "--reason".to_owned(),
            "The reference turned out to name another case.".to_owned(),
        ]);
        self.json(&arguments);
    }

    /// The deletion of the one case, with or without a purge.
    #[must_use]
    pub fn delete_arguments(&self, purge: bool) -> Vec<String> {
        let mut arguments = self.command(&["case", "delete"]);
        arguments.extend(["--case".to_owned(), self.case_id.clone()]);
        if purge {
            arguments.push("--purge".to_owned());
        }
        arguments
    }

    /// The one receipt, over the second stored artefact.
    #[must_use]
    pub fn receipt_arguments(&self) -> Vec<String> {
        let mut arguments = self.command(&["receipt", "add"]);
        arguments.extend([
            "--artefact".to_owned(),
            self.digests[1].clone(),
            "--label".to_owned(),
            "Envelope from the post".to_owned(),
        ]);
        arguments
    }

    /// One association, with the candidates named by submission index.
    #[must_use]
    pub fn association_arguments(
        &self,
        outcome: &str,
        candidates: &[(usize, &str, &str)],
    ) -> Vec<String> {
        let mut arguments = self.command(&["association", "create"]);
        arguments.extend([
            "--receipt".to_owned(),
            self.receipt_id.clone(),
            "--outcome".to_owned(),
            outcome.to_owned(),
        ]);
        for (index, confidence, statement) in candidates {
            arguments.extend([
                "--candidate".to_owned(),
                format!("{}:{confidence}:{statement}", self.submission_ids[*index]),
            ]);
        }
        arguments
    }

    /// Normalise both streams and the exit code of one run.
    #[must_use]
    pub fn captured(&self, output: &Output, json: bool) -> Captured {
        let stdout = String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8");
        let stdout = if json { pretty(&stdout) } else { stdout };
        Captured {
            stdout: normalise(&stdout, self.root()),
            stderr: normalise(
                &String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8"),
                self.root(),
            ),
            exit: format!(
                "{}\n",
                output.status.code().expect("the run was not signalled")
            ),
        }
    }
}

/// Re-render one envelope with sorted keys and an indent, so a diff is
/// readable. The binary writes one compact line; the stored form is expanded.
fn pretty(line: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(line.trim()).expect("stdout is one object");
    let mut rendered = serde_json::to_string_pretty(&value).expect("an envelope re-renders");
    rendered.push('\n');
    rendered
}

/// The string a JSON value holds.
fn text(value: &serde_json::Value) -> String {
    value.as_str().expect("a reported identifier").to_owned()
}

/// Write the three synthetic inputs and return their paths in import order.
fn write_inputs(root: &Path) -> Vec<PathBuf> {
    let inputs = root.join("inputs");
    fs::create_dir(&inputs).expect("create the input directory");
    ["alpha.txt", "beta.txt", "gamma.txt"]
        .into_iter()
        .zip(payloads())
        .map(|(name, payload)| {
            let path = inputs.join(name);
            fs::write(&path, payload).expect("write a synthetic input");
            path
        })
        .collect()
}

/// Record the two extra submissions the archive summary's case needs: one
/// whose retention window closed before the pinned as-of date, and one whose
/// window is still open on it.
///
/// Together with the world's own two submissions, one dated and named by a
/// candidate association and one carrying no date at all, the archive then
/// holds exactly one submission of each kind the summary distinguishes.
pub fn record_window_submissions(world: &World) {
    world.add_dated_submission("Sent the first reminder.", ELAPSED_DATE);
    world.add_dated_submission("Sent the second reminder.", OPEN_DATE);
    world.add_closed_case();
}

/// The same world, with the receipt's live association retired.
///
/// The candidate record that named the first submission is superseded by the
/// retirement, so the chain's live head asserts nothing and the submission is
/// reminded of again. The retired record is untouched and `association list`
/// still shows it.
pub fn retire_the_live_association(world: &World) {
    record_window_submissions(world);
    world.retire_live_association();
}

/// The date every window in the summary case is measured against.
pub const STATUS_AS_OF: &str = "2026-02-10";
/// A stated date whose window closed before [`STATUS_AS_OF`].
const ELAPSED_DATE: &str = "2025-12-01";
/// A stated date whose window is still open on [`STATUS_AS_OF`].
const OPEN_DATE: &str = "2026-01-20";

/// Replace one stored object's bytes with others of the same length.
///
/// The corruption is deterministic: the same object, the same replacement
/// bytes, and therefore the same report on every run.
pub fn damage_one_object(world: &World) {
    let digest = world.digests[0]
        .strip_prefix("sha256:")
        .expect("a stored digest is algorithm qualified");
    let path = world
        .archive
        .join("objects/sha256")
        .join(&digest[0..2])
        .join(&digest[2..4])
        .join(digest);
    permissions(&path, 0o600);
    fs::write(&path, DAMAGED).expect("replace the stored bytes");
    permissions(&path, 0o400);
}

/// Widen one layout directory, so the permission repair has work to report.
pub fn widen_one_directory(world: &World) {
    permissions(&world.archive.join("records/cases"), 0o750);
}

/// Set one path's mode, which the golden cases only ever do deliberately.
fn permissions(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .expect("set a mode the case depends on");
}

/// Export the one case and delete it with a purge, so the archive is ready
/// for the import that restores it.
///
/// It is the round trip the design describes: an export is a plain copy
/// outward, and the import that follows puts the same bytes and the same
/// records back under their original identifiers.
pub fn export_and_purge(world: &World) {
    let mut arguments = world.command(&["case", "export"]);
    arguments.extend([
        "--case".to_owned(),
        world.case_id.clone(),
        "--to".to_owned(),
        world.export_destination(),
    ]);
    world.json(&arguments);
    world.json(&world.delete_arguments(true));
}

/// Create a directory that holds no export at all.
pub fn write_empty_source(world: &World) {
    fs::create_dir(world.root().join("empty")).expect("create the empty source");
}

/// Name the directory [`write_empty_source`] created.
#[must_use]
pub fn empty_source(world: &World) -> String {
    world.root().join("empty").to_string_lossy().into_owned()
}

/// Write one more input than an import accepts.
pub fn write_over_the_import_file_cap(world: &World) {
    let directory = world.root().join("many");
    fs::create_dir(&directory).expect("create the over-cap input directory");
    for index in 0..OVER_THE_IMPORT_FILE_CAP {
        fs::write(directory.join(format!("input-{index:04}.txt")), ALPHA)
            .expect("write an over-cap input");
    }
}

/// Name the inputs [`write_over_the_import_file_cap`] wrote.
#[must_use]
pub fn over_the_import_file_cap(world: &World) -> Vec<String> {
    let directory = world.root().join("many");
    (0..OVER_THE_IMPORT_FILE_CAP)
        .map(|index| {
            directory
                .join(format!("input-{index:04}.txt"))
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

/// Replace everything that legitimately moves between runs.
///
/// Four placeholders, and nothing else: the temporary root becomes `<root>`,
/// an RFC 3339 instant becomes `<time>`, a 64-character hexadecimal run
/// becomes `<digest>`, and a 32-character one becomes `<id>`. Byte counts,
/// the user's own text, a stated date, and the order of every array are part
/// of the contract and are compared exactly as they were produced.
#[must_use]
pub fn normalise(text: &str, root: &Path) -> String {
    let replaced = text.replace(&root.to_string_lossy().into_owned(), "<root>");
    let characters: Vec<char> = replaced.chars().collect();
    let mut out = String::with_capacity(replaced.len());
    let mut index = 0;
    while index < characters.len() {
        if is_timestamp(&characters, index) {
            out.push_str("<time>");
            index += TIMESTAMP_LENGTH;
            continue;
        }
        if characters[index].is_ascii_hexdigit() && !characters[index].is_ascii_uppercase() {
            let end = hex_run_end(&characters, index);
            out.push_str(&placeholder(&characters, index, end));
            index = end;
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// The length of the only instant shape openPapir records.
const TIMESTAMP_LENGTH: usize = 20;

/// Whether `YYYY-MM-DDTHH:MM:SSZ` starts at `index`, on a word boundary.
fn is_timestamp(characters: &[char], index: usize) -> bool {
    const SHAPE: [char; TIMESTAMP_LENGTH] = [
        'd', 'd', 'd', 'd', '-', 'd', 'd', '-', 'd', 'd', 'T', 'd', 'd', ':', 'd', 'd', ':', 'd',
        'd', 'Z',
    ];
    if index + TIMESTAMP_LENGTH > characters.len() || !starts_a_word(characters, index) {
        return false;
    }
    SHAPE.iter().enumerate().all(|(offset, expected)| {
        let actual = characters[index + offset];
        if *expected == 'd' {
            actual.is_ascii_digit()
        } else {
            actual == *expected
        }
    })
}

/// The end of the maximal lowercase hexadecimal run starting at `index`.
fn hex_run_end(characters: &[char], index: usize) -> usize {
    let mut end = index;
    while end < characters.len()
        && characters[end].is_ascii_hexdigit()
        && !characters[end].is_ascii_uppercase()
    {
        end += 1;
    }
    end
}

/// The placeholder a hexadecimal run stands for, or the run itself.
fn placeholder(characters: &[char], index: usize, end: usize) -> String {
    let bounded = starts_a_word(characters, index) && ends_a_word(characters, end);
    match end - index {
        64 if bounded => "<digest>".to_owned(),
        32 if bounded => "<id>".to_owned(),
        _ => characters[index..end].iter().collect(),
    }
}

/// Whether the character before `index` cannot be part of the same token.
fn starts_a_word(characters: &[char], index: usize) -> bool {
    index == 0 || !characters[index - 1].is_ascii_alphanumeric()
}

/// Whether the character at `end` cannot be part of the same token.
fn ends_a_word(characters: &[char], end: usize) -> bool {
    end == characters.len() || !characters[end].is_ascii_alphanumeric()
}
