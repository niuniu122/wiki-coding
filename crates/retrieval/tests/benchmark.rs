use std::time::Instant;

use minimax_retrieval::{WikiDocument, WikiIndex};

#[test]
fn ten_thousand_current_wiki_pages_search_within_the_phase_five_p95_budget() {
    let documents = (0..10_000)
        .map(|index| WikiDocument {
            id: format!("page-{index:05}"),
            title: format!("Project note {index}"),
            body: if index == 7_777 {
                "unique retrieval needle architecture decision".into()
            } else {
                format!("ordinary project knowledge note topic-{index}")
            },
            aliases: Vec::new(),
            current: true,
        })
        .collect();
    let index = WikiIndex::new(documents);
    assert_eq!(index.len(), 10_000);

    let mut elapsed = Vec::new();
    for _ in 0..30 {
        let started = Instant::now();
        let result = index.search("unique retrieval needle", 5);
        elapsed.push(started.elapsed());
        assert_eq!(result[0].document.id, "page-07777");
    }
    elapsed.sort_unstable();
    let p95 = elapsed[(elapsed.len() * 95).div_ceil(100).saturating_sub(1)];
    eprintln!("10k Wiki BM25 p95: {:.3} ms", p95.as_secs_f64() * 1_000.0);
    assert!(
        p95.as_millis() <= 100,
        "10k Wiki BM25 p95 exceeded 100 ms: {p95:?}"
    );
}
