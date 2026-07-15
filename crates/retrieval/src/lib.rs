//! Retrieval pipelines for the Rust rewrite.
//!
//! The open-source project finder keeps its user-facing order: BM25 derives
//! keywords and recalls candidates first; embeddings only match or rerank that
//! bounded candidate set.

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "BM25-first candidate recall followed by bounded embedding matching";
