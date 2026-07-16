use std::collections::BTreeMap;

use crate::{SearchDocument, normalize_query};

#[derive(Clone, Debug)]
pub(crate) struct ExactIndex<D: SearchDocument> {
    values: BTreeMap<String, usize>,
    _document: std::marker::PhantomData<D>,
}

impl<D: SearchDocument> ExactIndex<D> {
    pub(crate) fn new(documents: &[D]) -> Self {
        let mut values = BTreeMap::new();
        for (index, document) in documents.iter().enumerate() {
            for key in document.exact_keys() {
                let normalized = normalize_query(key);
                if !normalized.is_empty() {
                    values.entry(normalized).or_insert(index);
                }
            }
        }
        Self {
            values,
            _document: std::marker::PhantomData,
        }
    }

    pub(crate) fn resolve(&self, query: &str) -> Option<usize> {
        self.values.get(&normalize_query(query)).copied()
    }
}
