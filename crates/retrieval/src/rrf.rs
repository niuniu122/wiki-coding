use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct RankedId {
    pub id: String,
    pub score: f64,
}

#[must_use]
pub fn reciprocal_rank_fusion(rankings: &[Vec<String>], constant: usize) -> Vec<RankedId> {
    let mut scores = BTreeMap::<String, f64>::new();
    for ranking in rankings {
        for (index, id) in ranking.iter().enumerate() {
            *scores.entry(id.clone()).or_default() += 1.0 / (constant + index + 1) as f64;
        }
    }
    let mut results = scores
        .into_iter()
        .map(|(id, score)| RankedId { id, score })
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.id.cmp(&right.id))
    });
    results
}

#[must_use]
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.is_empty() || left.len() != right.len() {
        return None;
    }
    let mut dot = 0.0_f64;
    let mut left_norm = 0.0_f64;
    let mut right_norm = 0.0_f64;
    for (left, right) in left.iter().zip(right) {
        if !left.is_finite() || !right.is_finite() {
            return None;
        }
        let left = f64::from(*left);
        let right = f64::from(*right);
        dot += left * right;
        left_norm += left * left;
        right_norm += right * right;
    }
    let denominator = left_norm.sqrt() * right_norm.sqrt();
    if denominator == 0.0 || !denominator.is_finite() {
        None
    } else {
        Some(dot / denominator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_and_cosine_are_stable() {
        let fused = reciprocal_rank_fusion(
            &[vec!["b".into(), "a".into()], vec!["a".into(), "b".into()]],
            60,
        );
        assert_eq!(
            fused
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]), Some(1.0));
        assert_eq!(cosine_similarity(&[f32::NAN], &[1.0]), None);
    }
}
