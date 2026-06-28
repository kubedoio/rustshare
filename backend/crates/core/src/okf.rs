//! Open Knowledge Format (OKF) domain types and utilities.

pub mod frontmatter;

pub use frontmatter::{
    default_note_frontmatter, merge_required_okf_keys, parse_frontmatter, serialize_frontmatter,
    split_frontmatter, to_document, FrontmatterError, OkfNoteFrontmatter, RustshareFrontmatter,
};
