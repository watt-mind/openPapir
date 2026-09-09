//! The atomic write procedure every file in the archive goes through.
//!
//! `docs/archive-layout.md` fixes the procedure: create the temporary file in
//! the destination directory, write the whole content, flush the file, put it
//! in place, and flush the destination directory. A leftover staging file is
//! never adopted, so an interrupted write leaves the complete file or nothing.
//!
//! The publish step is a hard link followed by the removal of the staging
//! name. That is atomic, stays on one filesystem exactly as the design
//! requires, and, unlike a rename, cannot silently replace a file openPapir
//! did not create: the destination is refused as `path.overwrite` instead.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::archive::paths;
use crate::error::{Details, Diagnostic, Warning};
use crate::ident;

/// A staging file inside the archive root, removed unless it is published.
#[derive(Debug)]
pub struct Staging {
    path: PathBuf,
    file: Option<File>,
}

impl Staging {
    /// Create a staging file in `directory`, which must be inside the root.
    ///
    /// # Errors
    ///
    /// Returns `write.interrupted` when the staging file cannot be created.
    pub fn create(directory: &Path, stage: &'static str) -> Result<Self, Diagnostic> {
        let path = directory.join(format!(".papir-staging-{}", ident::new_id()?));
        let file = paths::create_file_owner_only(&path)
            .map_err(|error| paths::publish_refusal(&error, "", stage))?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    /// The staging file's path, for streaming into it.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open staging file.
    pub fn file(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("the staging file stays open until it is published")
    }

    /// Flush the staged content, then close the file.
    ///
    /// # Errors
    ///
    /// Returns `write.interrupted` when the content cannot be made durable.
    pub fn finish(&mut self, stage: &'static str) -> Result<(), Diagnostic> {
        let Some(mut file) = self.file.take() else {
            return Ok(());
        };
        file.flush()
            .and_then(|()| file.sync_all())
            .map_err(|error| paths::publish_refusal(&error, "", stage))
    }

    /// Put the staged file at `destination` without replacing anything.
    ///
    /// # Errors
    ///
    /// Returns `path.symlink`, `path.overwrite`, `path.cross_device`, or
    /// `write.interrupted`, as the condition requires.
    pub fn publish(
        mut self,
        destination: &Path,
        archive_path: &str,
        stage: &'static str,
    ) -> Result<Vec<Warning>, Diagnostic> {
        self.finish(stage)?;
        if paths::is_symlink(destination) {
            return Err(paths::symlink_refusal(
                Details::new()
                    .text("scope", "archive")
                    .text("archive_path", archive_path),
            ));
        }
        fs::hard_link(&self.path, destination)
            .map_err(|error| paths::publish_refusal(&error, archive_path, stage))?;
        let _ = fs::remove_file(&self.path);
        self.file = None;
        let mut warnings = Vec::new();
        if let Some(parent) = destination.parent()
            && let Some(warning) = paths::sync_directory(parent, stage)
        {
            warnings.push(warning);
        }
        Ok(warnings)
    }
}

impl Drop for Staging {
    /// An abandoned staging file is removed, never adopted.
    fn drop(&mut self) {
        self.file = None;
        let _ = fs::remove_file(&self.path);
    }
}

/// Write one document into `directory` under a name openPapir derived itself.
///
/// # Errors
///
/// Returns the same refusals as [`Staging::publish`].
pub fn write_document(
    directory: &Path,
    file_name: &str,
    archive_path: &str,
    content: &[u8],
    stage: &'static str,
) -> Result<Vec<Warning>, Diagnostic> {
    let mut staging = Staging::create(directory, stage)?;
    staging
        .file()
        .write_all(content)
        .map_err(|error| paths::publish_refusal(&error, archive_path, stage))?;
    staging.publish(&directory.join(file_name), archive_path, stage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::codes;

    #[test]
    fn a_document_is_written_whole_and_never_replaces_one() {
        let directory = tempfile::tempdir().unwrap();
        let warnings = write_document(
            directory.path(),
            "marker.json",
            "marker.json",
            b"{}\n",
            "marker_write",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("marker.json")).unwrap(),
            "{}\n"
        );
        assert!(
            warnings
                .iter()
                .all(|warning| warning.bucket() == crate::error::Bucket::Platform),
            "only platform degradations are reported"
        );
        let refusal = write_document(
            directory.path(),
            "marker.json",
            "marker.json",
            b"{}\n",
            "marker_write",
        )
        .unwrap_err();
        assert_eq!(refusal.code, codes::PATH_OVERWRITE);
    }

    #[test]
    fn an_abandoned_staging_file_is_removed_and_never_adopted() {
        let directory = tempfile::tempdir().unwrap();
        {
            let mut staging = Staging::create(directory.path(), "object").unwrap();
            staging.file().write_all(b"partial").unwrap();
        }
        let remaining: Vec<_> = fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(remaining.is_empty(), "staging files are never left behind");
    }

    #[test]
    fn publishing_onto_a_symbolic_link_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        if cfg!(unix) {
            #[cfg(unix)]
            std::os::unix::fs::symlink("/etc/hostname", directory.path().join("linked.json"))
                .unwrap();
            let refusal = write_document(
                directory.path(),
                "linked.json",
                "records/imports/linked.json",
                b"{}\n",
                "record",
            )
            .unwrap_err();
            assert_eq!(refusal.code, codes::PATH_SYMLINK);
        }
    }
}
