# Optional Granite embedding resource package

MiniMax Codex does not bundle or download embedding weights. Semantic retrieval is enabled only when the user explicitly installs a validated `@minimax-codex/embedding-granite-97m-r2-avx2` resource directory.

Its `manifest.json` must identify `ibm-granite/granite-embedding-97m-multilingual-r2`, a pinned upstream revision, runtime ABI, `x64-avx2`, `qint8`, license, tokenizer version, and SHA-256 for every resource file. The core validates those fields and file hashes before loading a provider factory. Missing, incompatible, or corrupt resources fall back to exact + BM25 without network access.

The repository and CI contain only tiny deterministic fake vectors. Real package revision/hash acceptance and performance testing are explicit local release steps; they are never guessed or fetched during startup or search.
