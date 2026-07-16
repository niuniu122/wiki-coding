use std::collections::{BTreeMap, BTreeSet};

use minimax_protocol::RetrievalMode;

use crate::domain::DomainIdentity;
use crate::exact::ExactIndex;
use crate::{SearchDocument, tokenize_query};

const K1: f64 = 1.2;
const B: f64 = 0.75;
const MINIMUM_USEFUL_SCORE: f64 = 0.35;

#[derive(Clone, Debug, PartialEq)]
pub struct Bm25Contribution {
    pub term: String,
    pub score: f64,
}

#[derive(Clone, Debug)]
pub struct LexicalHit<D: SearchDocument> {
    pub document: D,
    pub score: f64,
    pub mode: RetrievalMode,
    pub contributions: Vec<Bm25Contribution>,
}

#[derive(Clone, Debug)]
struct IndexedDocument<D: SearchDocument> {
    document: D,
    token_count: usize,
    counts: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct LexicalIndex<D: SearchDocument> {
    documents: Vec<IndexedDocument<D>>,
    exact: ExactIndex<D>,
    document_frequency: BTreeMap<String, usize>,
    average_length: f64,
    _domain: DomainIdentity<D::Marker>,
}

impl<D: SearchDocument> LexicalIndex<D> {
    #[must_use]
    pub fn new(documents: Vec<D>) -> Self {
        let searchable = documents
            .into_iter()
            .filter(SearchDocument::is_searchable)
            .collect::<Vec<_>>();
        let exact = ExactIndex::new(&searchable);
        let mut document_frequency = BTreeMap::new();
        let indexed = searchable
            .into_iter()
            .map(|document| {
                let tokens = tokenize_query(&document.search_text());
                let token_count = tokens.len();
                let mut counts = BTreeMap::new();
                for token in tokens {
                    *counts.entry(token).or_insert(0) += 1;
                }
                for token in counts.keys() {
                    *document_frequency.entry(token.clone()).or_insert(0) += 1;
                }
                IndexedDocument {
                    document,
                    token_count,
                    counts,
                }
            })
            .collect::<Vec<_>>();
        let average_length = if indexed.is_empty() {
            1.0
        } else {
            indexed
                .iter()
                .map(|document| document.token_count as f64)
                .sum::<f64>()
                / indexed.len() as f64
        };
        Self {
            documents: indexed,
            exact,
            document_frequency,
            average_length,
            _domain: DomainIdentity::default(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<LexicalHit<D>> {
        if limit == 0 {
            return Vec::new();
        }
        if let Some(index) = self.exact.resolve(query) {
            let document = &self.documents[index].document;
            return vec![LexicalHit {
                document: document.clone(),
                score: f64::INFINITY,
                mode: RetrievalMode::Exact,
                contributions: tokenize_query(query)
                    .into_iter()
                    .map(|term| Bm25Contribution { term, score: 1.0 })
                    .collect(),
            }];
        }

        let query_tokens = tokenize_query(query).into_iter().collect::<BTreeSet<_>>();
        let total = self.documents.len();
        if query_tokens.is_empty() || total == 0 {
            return Vec::new();
        }

        let mut results = Vec::new();
        for document in &self.documents {
            let mut contributions = Vec::new();
            let mut score = 0.0;
            for token in &query_tokens {
                let frequency = *document.counts.get(token).unwrap_or(&0);
                if frequency == 0 {
                    continue;
                }
                let docs_with_token = *self.document_frequency.get(token).unwrap_or(&0);
                let idf = (1.0
                    + (total as f64 - docs_with_token as f64 + 0.5)
                        / (docs_with_token as f64 + 0.5))
                    .ln();
                let denominator = frequency as f64
                    + K1 * (1.0 - B + B * document.token_count as f64 / self.average_length);
                let contribution = idf * frequency as f64 * (K1 + 1.0) / denominator;
                if contribution.is_finite() && contribution > 0.0 {
                    score += contribution;
                    contributions.push(Bm25Contribution {
                        term: token.clone(),
                        score: contribution,
                    });
                }
            }
            let has_meaningful_term = contributions
                .iter()
                .any(|item| item.term.chars().count() > 1);
            if score.is_finite() && score >= MINIMUM_USEFUL_SCORE && has_meaningful_term {
                contributions.sort_by(|left, right| {
                    right
                        .score
                        .total_cmp(&left.score)
                        .then_with(|| left.term.cmp(&right.term))
                });
                results.push(LexicalHit {
                    document: document.document.clone(),
                    score,
                    mode: RetrievalMode::Bm25,
                    contributions,
                });
            }
        }
        results.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.document.id().cmp(right.document.id()))
        });
        results.truncate(limit);
        results
    }
}
