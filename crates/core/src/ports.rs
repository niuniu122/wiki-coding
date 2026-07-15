/// Supplies time to core workflows without consulting the system clock directly.
pub trait Clock {
    fn now_unix_ms(&self) -> u64;
}

/// Supplies stable identifiers without coupling core to an operating-system RNG.
pub trait IdGenerator {
    fn next_id(&self, prefix: &str) -> String;
}

/// Deterministic clock for fixtures and replay tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedClock {
    unix_ms: u64,
}

impl FixedClock {
    #[must_use]
    pub const fn new(unix_ms: u64) -> Self {
        Self { unix_ms }
    }
}

impl Clock for FixedClock {
    fn now_unix_ms(&self) -> u64 {
        self.unix_ms
    }
}

/// Deterministic ID generator for fixtures and replay tests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedIdGenerator {
    suffix: String,
}

impl FixedIdGenerator {
    #[must_use]
    pub fn new(suffix: impl Into<String>) -> Self {
        Self {
            suffix: suffix.into(),
        }
    }
}

impl IdGenerator for FixedIdGenerator {
    fn next_id(&self, prefix: &str) -> String {
        format!("{prefix}_{}", self.suffix)
    }
}
