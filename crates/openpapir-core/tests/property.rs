//! Generative tests over the library's untrusted-input boundaries.
//!
//! Every test in this target is a property: a strategy generates the input,
//! proptest shrinks a counterexample to its smallest form, and the assertion
//! states an invariant rather than an expected value. The invariants are the
//! ones `SECURITY.md` asks for at a boundary that accepts input openPapir did
//! not mint: never panic, never follow a link, never read past a cap, never
//! echo a value the user supplied, and never answer with a code outside the
//! documented table.
//!
//! The case count is bounded so that the whole target stays well inside a
//! minute on continuous integration; `PROPTEST_CASES` raises it locally.
//! `docs/testing.md` says how.

#[path = "property/mod.rs"]
mod property;
