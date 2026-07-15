//! Provider adapters for the Rust rewrite.
//!
//! This crate translates provider-specific streams into protocol events. It
//! depends inward on core policy and protocol contracts; core never imports it.

mod chat_completions;
mod client;
mod fixture_protocol;
mod responses;
mod sse;

pub use chat_completions::ChatCompletionsAdapter;
pub use client::HttpProviderClient;
pub use fixture_protocol::{
    CompatibilityEvent, FixtureReplay, parse_chat_completions_event, parse_responses_event,
    replay_fixture,
};
pub use responses::ResponsesAdapter;
pub use sse::{MAX_SSE_EVENT_BYTES, SseDecoder};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "provider-specific translation into stable protocol events";
