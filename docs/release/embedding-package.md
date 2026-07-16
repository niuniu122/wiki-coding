# Optional Embedding Package

The base Windows/Linux release contains no embedding weights, vector bundle, tokenizer package, dynamic model runtime, or downloader. Exact and BM25 retrieval remain fully available without it.

Semantic reranking is a separate, user-installed Granite multilingual qint8 x64-AVX2 resource. Activation requires the complete resource manifest, package/model/revision identity, runtime ABI, CPU/platform health, license, tokenizer, dimensions, catalog/vector fingerprints, and every file SHA-256 to match. The fixed local helper receives only the query plus BM25 candidates and has no shell, network, or credential input.

Missing, corrupt, stale, incompatible, slow, or malformed resources leave the BM25 order intact and report one explicit degradation reason. Installing or upgrading the base binary never installs, upgrades, deletes, or repairs this optional package.
