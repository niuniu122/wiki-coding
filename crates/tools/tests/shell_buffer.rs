use std::sync::Arc;

use minimax_protocol::{MAX_SHELL_OUTPUT_BYTES, MAX_SHELL_UNREAD_BYTES};
use minimax_tools::{ShellOutputBudget, ShellOutputBuffer};

#[test]
fn split_utf8_and_split_ansi_sequences_normalize_once() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(&[0xe4, 0xbd]);
    buffer.append(&[0xa0, 0xe5, 0xa5, 0xbd]);
    buffer.append(b"\x1b[31");
    buffer.append(b"m red\x1b[0m\n");
    buffer.finish();
    assert_eq!(buffer.take(1024).output, "你好 red\n");
}

#[test]
fn unread_ring_drops_oldest_bytes_and_reports_truncation_once() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(&vec![b'a'; MAX_SHELL_UNREAD_BYTES]);
    buffer.append(b"tail");
    let first = buffer.take(MAX_SHELL_OUTPUT_BYTES);
    assert!(first.truncated);
    assert_eq!(
        buffer.unread_bytes(),
        MAX_SHELL_UNREAD_BYTES - MAX_SHELL_OUTPUT_BYTES
    );
    let second = buffer.take(MAX_SHELL_OUTPUT_BYTES);
    assert!(!second.truncated);
}

#[test]
fn controls_are_removed_but_terminal_whitespace_is_preserved() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(b"a\0\x01\tb\r\n\x7fc");
    buffer.append("\u{0085}d".as_bytes());
    buffer.finish();
    assert_eq!(buffer.take(1024).output, "a\tb\r\ncd");
}

#[test]
fn osc_sequences_split_across_chunks_are_discarded() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append(b"before\x1b]0;secret");
    buffer.append(b"\x07middle\x1b]8;;https://example.invalid");
    buffer.append(b"\x1b");
    buffer.append(b"\\after");
    buffer.finish();
    assert_eq!(buffer.take(1024).output, "beforemiddleafter");
}

#[test]
fn invalid_and_incomplete_utf8_are_replaced_lossily() {
    let mut invalid = ShellOutputBuffer::default();
    invalid.append(&[0xf0]);
    invalid.append(&[0x28, 0x8c]);
    invalid.append(&[0x28]);
    invalid.finish();
    assert_eq!(invalid.take(1024).output, "�(�(");

    let mut incomplete = ShellOutputBuffer::default();
    incomplete.append(&[0xe4, 0xbd]);
    incomplete.finish();
    assert_eq!(incomplete.take(1024).output, "�");
}

#[test]
fn take_respects_the_requested_limit_without_splitting_utf8() {
    let mut buffer = ShellOutputBuffer::default();
    buffer.append("a好bc".as_bytes());
    buffer.finish();

    assert_eq!(buffer.take(2).output, "a");
    assert_eq!(buffer.take(3).output, "好");
    assert_eq!(buffer.take(1).output, "b");
    assert_eq!(buffer.take(1024).output, "c");
    assert_eq!(buffer.unread_bytes(), 0);
}

#[test]
fn shared_budget_never_exceeds_eight_mib_and_releases_exactly() {
    let total_limit = 8 * 1_024 * 1_024;
    let budget = Arc::new(ShellOutputBudget::new(total_limit));
    let mut buffers = (0..8)
        .map(|_| ShellOutputBuffer::new(Arc::clone(&budget)))
        .collect::<Vec<_>>();

    for buffer in &mut buffers {
        buffer.append(&vec![b'a'; MAX_SHELL_UNREAD_BYTES]);
        assert_eq!(buffer.unread_bytes(), MAX_SHELL_UNREAD_BYTES);
        assert!(budget.used() <= total_limit);
    }
    assert_eq!(budget.used(), total_limit);

    let mut ninth = ShellOutputBuffer::new(Arc::clone(&budget));
    ninth.append(b"tail");
    assert_eq!(budget.used(), total_limit);
    assert_eq!(ninth.unread_bytes(), 0);
    assert!(ninth.take(MAX_SHELL_OUTPUT_BYTES).truncated);

    let before_drain = budget.used();
    let drained = buffers[0].take(MAX_SHELL_OUTPUT_BYTES);
    assert_eq!(drained.output.len(), MAX_SHELL_OUTPUT_BYTES);
    assert_eq!(budget.used(), before_drain - drained.output.len());

    ninth.append(&vec![b'z'; drained.output.len()]);
    assert_eq!(ninth.unread_bytes(), drained.output.len());
    assert_eq!(budget.used(), total_limit);

    drop(ninth);
    drop(buffers);
    assert_eq!(budget.used(), 0);
}

#[test]
fn a_full_buffer_reuses_its_own_global_reservation() {
    let budget = Arc::new(ShellOutputBudget::new(MAX_SHELL_UNREAD_BYTES));
    let mut buffer = ShellOutputBuffer::new(Arc::clone(&budget));
    buffer.append(&vec![b'a'; MAX_SHELL_UNREAD_BYTES]);
    buffer.append(b"tail");

    assert_eq!(budget.used(), MAX_SHELL_UNREAD_BYTES);
    assert_eq!(buffer.unread_bytes(), MAX_SHELL_UNREAD_BYTES);
    let chunk = buffer.take(MAX_SHELL_UNREAD_BYTES);
    assert!(chunk.truncated);
    assert!(chunk.output.ends_with("tail"));
    assert_eq!(budget.used(), 0);
}
