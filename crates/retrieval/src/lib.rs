//! Typed, deterministic retrieval pipelines for the Rust rewrite.
//!
//! Project discovery always recalls candidates with lexical search first.
//! Optional embeddings may only rerank that bounded candidate set.

mod bm25;
mod domain;
mod exact;
mod normalize;
mod rrf;
mod snapshot;

pub use bm25::{Bm25Contribution, LexicalHit, LexicalIndex};
pub use domain::{
    CapabilityDocument, CapabilityMarker, ProjectDocument, ProjectMarker, SearchDocument,
    WikiDocument, WikiMarker,
};
pub use normalize::{QUERY_TOKENIZER_VERSION, normalize_query, tokenize_query};
pub use rrf::{RankedId, cosine_similarity, reciprocal_rank_fusion};
pub use snapshot::{
    IndexSnapshot, SnapshotError, load_snapshot, publish_snapshot, snapshot_file_hash,
};

pub type CapabilityIndex = LexicalIndex<CapabilityDocument>;
pub type ProjectIndex = LexicalIndex<ProjectDocument>;
pub type WikiIndex = LexicalIndex<WikiDocument>;

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "BM25-first candidate recall followed by bounded embedding matching";
