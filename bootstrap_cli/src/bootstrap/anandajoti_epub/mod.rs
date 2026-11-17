//! What the Ānandajoti epub imports have in common: reading the epub, the
//! block stream and notes of its content files, the HTML writer for inline
//! text and notes, the page frame, the defect report, and the import helpers.
//!
//! The structure of a book (vaggas and stories, Jātakas and sections) and its
//! class tables stay in `dhammapada_commentary_anandajoti` and
//! `jataka_commentary_anandajoti`. A new Ānandajoti epub gets its own module
//! with a [`page::ClassMap`] for its span and paragraph classes, and uses
//! [`page::Writer`] to render, [`page::frame`] to wrap the page, and
//! [`report::Report`] for the defects.

pub mod epub;
pub mod import;
pub mod page;
pub mod report;
pub mod source;

#[cfg(test)]
mod tests;
