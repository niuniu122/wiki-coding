use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use minimax_protocol::MAX_SHELL_UNREAD_BYTES;

const DEFAULT_SHELL_OUTPUT_BUDGET_BYTES: usize = 8 * 1_024 * 1_024;

pub struct ShellOutputBuffer {
    unread: VecDeque<u8>,
    truncated: bool,
    normalizer: TerminalNormalizer,
    budget: Arc<ShellOutputBudget>,
}

pub struct ShellOutputBudget {
    used: AtomicUsize,
    limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellOutputChunk {
    pub output: String,
    pub truncated: bool,
}

impl ShellOutputBudget {
    pub const fn new(limit: usize) -> Self {
        Self {
            used: AtomicUsize::new(0),
            limit,
        }
    }

    pub fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }

    fn reserve_up_to(&self, requested: usize) -> usize {
        let mut used = self.used();
        loop {
            let reserved = requested.min(self.limit.saturating_sub(used));
            if reserved == 0 {
                return 0;
            }
            match self.used.compare_exchange_weak(
                used,
                used + reserved,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return reserved,
                Err(actual) => used = actual,
            }
        }
    }

    fn release(&self, released: usize) {
        if released == 0 {
            return;
        }
        let previous = self.used.fetch_sub(released, Ordering::AcqRel);
        debug_assert!(previous >= released);
    }
}

impl ShellOutputBuffer {
    pub fn new(budget: Arc<ShellOutputBudget>) -> Self {
        Self {
            unread: VecDeque::new(),
            truncated: false,
            normalizer: TerminalNormalizer::default(),
            budget,
        }
    }

    pub fn append(&mut self, bytes: &[u8]) {
        let normalized = self.normalizer.append(bytes);
        self.append_normalized(normalized);
    }

    pub fn finish(&mut self) {
        let normalized = self.normalizer.finish();
        self.append_normalized(normalized);
    }

    pub fn take(&mut self, max_bytes: usize) -> ShellOutputChunk {
        let requested = max_bytes.min(self.unread.len());
        let boundary = utf8_floor_boundary(&self.unread, requested);
        let bytes = self.unread.drain(..boundary).collect::<Vec<_>>();
        self.budget.release(boundary);
        let output = match String::from_utf8(bytes) {
            Ok(output) => output,
            Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
        };
        ShellOutputChunk {
            output,
            truncated: std::mem::take(&mut self.truncated),
        }
    }

    pub fn unread_bytes(&self) -> usize {
        self.unread.len()
    }

    fn append_normalized(&mut self, mut bytes: Vec<u8>) {
        if bytes.is_empty() {
            return;
        }

        if bytes.len() > MAX_SHELL_UNREAD_BYTES {
            let start = utf8_suffix_start(&bytes, MAX_SHELL_UNREAD_BYTES);
            bytes.drain(..start);
            self.truncated = true;
        }

        let excess = self
            .unread
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(MAX_SHELL_UNREAD_BYTES);
        if excess > 0 {
            self.drop_oldest(excess);
            self.truncated = true;
        }

        let budget = Arc::clone(&self.budget);
        let reserved = reserve_with_oldest_eviction(
            &mut self.unread,
            &mut self.truncated,
            bytes.len(),
            |requested| budget.reserve_up_to(requested),
            |released| budget.release(released),
        );
        let start = utf8_suffix_start(&bytes, reserved);
        let retained = bytes.len() - start;
        self.budget.release(reserved - retained);
        if start > 0 {
            self.truncated = true;
        }
        self.unread.extend(&bytes[start..]);
    }

    fn drop_oldest(&mut self, requested: usize) {
        let boundary = utf8_ceil_boundary(&self.unread, requested);
        self.unread.drain(..boundary);
        self.budget.release(boundary);
    }
}

impl Default for ShellOutputBuffer {
    fn default() -> Self {
        Self::new(Arc::new(ShellOutputBudget::new(
            DEFAULT_SHELL_OUTPUT_BUDGET_BYTES,
        )))
    }
}

impl Drop for ShellOutputBuffer {
    fn drop(&mut self) {
        self.budget.release(self.unread.len());
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum TerminalState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
}

#[derive(Default)]
struct TerminalNormalizer {
    state: TerminalState,
    utf8_pending: Vec<u8>,
}

impl TerminalNormalizer {
    fn append(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut output = String::new();
        for &byte in bytes {
            match self.state {
                TerminalState::Ground => {
                    if byte == b'\x1b' {
                        self.decode_pending(&mut output, true);
                        self.state = TerminalState::Escape;
                    } else {
                        self.utf8_pending.push(byte);
                        self.decode_pending(&mut output, false);
                    }
                }
                TerminalState::Escape => match byte {
                    b'[' => self.state = TerminalState::Csi,
                    b']' => self.state = TerminalState::Osc,
                    b'\x1b' | 0x20..=0x2f => {}
                    _ => self.state = TerminalState::Ground,
                },
                TerminalState::Csi => {
                    if byte == b'\x1b' {
                        self.state = TerminalState::Escape;
                    } else if (0x40..=0x7e).contains(&byte) {
                        self.state = TerminalState::Ground;
                    }
                }
                TerminalState::Osc => match byte {
                    b'\x07' => self.state = TerminalState::Ground,
                    b'\x1b' => self.state = TerminalState::OscEscape,
                    _ => {}
                },
                TerminalState::OscEscape => match byte {
                    b'\\' | b'\x07' => self.state = TerminalState::Ground,
                    b'\x1b' => {}
                    _ => self.state = TerminalState::Osc,
                },
            }
        }
        output.into_bytes()
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut output = String::new();
        self.decode_pending(&mut output, true);
        self.state = TerminalState::Ground;
        output.into_bytes()
    }

    fn decode_pending(&mut self, output: &mut String, finished: bool) {
        loop {
            match std::str::from_utf8(&self.utf8_pending) {
                Ok(valid) => {
                    push_displayable(output, valid);
                    self.utf8_pending.clear();
                    return;
                }
                Err(error) => {
                    let valid_up_to = error.valid_up_to();
                    if valid_up_to > 0 {
                        let valid = self.utf8_pending.drain(..valid_up_to).collect::<Vec<_>>();
                        if let Ok(valid) = std::str::from_utf8(&valid) {
                            push_displayable(output, valid);
                        }
                        continue;
                    }
                    if let Some(error_len) = error.error_len() {
                        self.utf8_pending.drain(..error_len);
                        output.push('\u{fffd}');
                        continue;
                    }
                    if finished {
                        let lossy = String::from_utf8_lossy(&self.utf8_pending);
                        push_displayable(output, &lossy);
                        self.utf8_pending.clear();
                    }
                    return;
                }
            }
        }
    }
}

fn push_displayable(output: &mut String, value: &str) {
    output.extend(
        value
            .chars()
            .filter(|character| matches!(character, '\n' | '\r' | '\t') || !character.is_control()),
    );
}

fn utf8_floor_boundary(bytes: &VecDeque<u8>, requested: usize) -> usize {
    let mut boundary = requested.min(bytes.len());
    while boundary > 0
        && boundary < bytes.len()
        && bytes
            .get(boundary)
            .is_some_and(|byte| is_utf8_continuation(*byte))
    {
        boundary -= 1;
    }
    boundary
}

fn utf8_ceil_boundary(bytes: &VecDeque<u8>, requested: usize) -> usize {
    let mut boundary = requested.min(bytes.len());
    while boundary < bytes.len()
        && bytes
            .get(boundary)
            .is_some_and(|byte| is_utf8_continuation(*byte))
    {
        boundary += 1;
    }
    boundary
}

fn utf8_suffix_start(bytes: &[u8], max_bytes: usize) -> usize {
    let mut start = bytes.len().saturating_sub(max_bytes);
    while start < bytes.len() && is_utf8_continuation(bytes[start]) {
        start += 1;
    }
    start
}

fn reserve_with_oldest_eviction(
    unread: &mut VecDeque<u8>,
    truncated: &mut bool,
    wanted: usize,
    mut reserve: impl FnMut(usize) -> usize,
    mut release: impl FnMut(usize),
) -> usize {
    let mut reserved = 0;
    while reserved < wanted {
        let remaining = wanted - reserved;
        reserved += reserve(remaining).min(remaining);
        if reserved == wanted || unread.is_empty() {
            break;
        }

        let shortfall = wanted - reserved;
        let boundary = utf8_ceil_boundary(unread, shortfall.min(unread.len()));
        unread.drain(..boundary);
        release(boundary);
        *truncated = true;
    }
    reserved
}

const fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::reserve_with_oldest_eviction;

    #[derive(Debug, Eq, PartialEq)]
    enum ReservationEvent {
        Reserve(usize),
        ReleaseOldest(usize),
    }

    #[test]
    fn actual_shortfall_releases_oldest_output_before_retrying() {
        let mut unread = VecDeque::from(vec![b'o'; 6]);
        let mut truncated = false;
        let scripted_reservations = RefCell::new(VecDeque::from([2, 1, 0]));
        let events = RefCell::new(Vec::new());

        let reserved = reserve_with_oldest_eviction(
            &mut unread,
            &mut truncated,
            6,
            |requested| {
                events
                    .borrow_mut()
                    .push(ReservationEvent::Reserve(requested));
                scripted_reservations
                    .borrow_mut()
                    .pop_front()
                    .expect("scripted reservation")
            },
            |released| {
                events
                    .borrow_mut()
                    .push(ReservationEvent::ReleaseOldest(released));
            },
        );

        assert_eq!(reserved, 3);
        assert!(unread.is_empty());
        assert!(truncated);
        assert_eq!(
            events.into_inner(),
            [
                ReservationEvent::Reserve(6),
                ReservationEvent::ReleaseOldest(4),
                ReservationEvent::Reserve(4),
                ReservationEvent::ReleaseOldest(2),
                ReservationEvent::Reserve(3),
            ]
        );
    }
}
