use minimax_protocol::StreamEvent;

const OPEN_TAG: &str = "<think>";
const CLOSE_TAG: &str = "</think>";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum FilterMode {
    #[default]
    Visible,
    Reasoning,
}

/// Removes MiniMax reasoning blocks carried inside Chat Completions `content`.
///
/// The pending buffer is bounded by the longest tag minus one byte. This lets
/// the filter recognize tags split across provider events without retaining
/// reasoning or arbitrary visible output.
#[derive(Debug, Default)]
pub(crate) struct ChatReasoningFilter {
    mode: FilterMode,
    pending: String,
}

impl ChatReasoningFilter {
    pub(crate) fn accept(&mut self, event: StreamEvent) -> Vec<StreamEvent> {
        match event {
            StreamEvent::VisibleTextDelta { delta } => self.accept_content(&delta),
            StreamEvent::Terminal { .. } => {
                self.finish();
                vec![event]
            }
            _ => vec![event],
        }
    }

    fn accept_content(&mut self, delta: &str) -> Vec<StreamEvent> {
        self.pending.push_str(delta);
        let mut events = Vec::new();

        loop {
            match self.mode {
                FilterMode::Visible => {
                    if let Some(index) = find_ascii_case_insensitive(&self.pending, OPEN_TAG) {
                        push_visible(&mut events, &self.pending[..index]);
                        self.pending = self.pending[index + OPEN_TAG.len()..].to_owned();
                        self.mode = FilterMode::Reasoning;
                        events.push(StreamEvent::ReasoningFiltered);
                        continue;
                    }

                    let retained = longest_suffix_matching_tag_prefix(&self.pending, OPEN_TAG);
                    let visible_len = self.pending.len() - retained;
                    let visible = self.pending[..visible_len].to_owned();
                    self.pending = self.pending[visible_len..].to_owned();
                    push_visible(&mut events, &visible);
                    break;
                }
                FilterMode::Reasoning => {
                    if let Some(index) = find_ascii_case_insensitive(&self.pending, CLOSE_TAG) {
                        self.pending = self.pending[index + CLOSE_TAG.len()..].to_owned();
                        self.mode = FilterMode::Visible;
                        continue;
                    }

                    let retained = longest_suffix_matching_tag_prefix(&self.pending, CLOSE_TAG);
                    self.pending = self.pending[self.pending.len() - retained..].to_owned();
                    break;
                }
            }
        }

        events
    }

    fn finish(&mut self) {
        // A partial opening tag and every unfinished reasoning block are
        // intentionally discarded. Emitting either would weaken the
        // fail-closed boundary at an abnormal provider termination.
        self.pending.clear();
    }
}

fn push_visible(events: &mut Vec<StreamEvent>, text: &str) {
    if !text.is_empty() {
        events.push(StreamEvent::VisibleTextDelta {
            delta: text.to_owned(),
        });
    }
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn longest_suffix_matching_tag_prefix(value: &str, tag: &str) -> usize {
    let value = value.as_bytes();
    let tag = tag.as_bytes();
    let limit = value.len().min(tag.len().saturating_sub(1));
    (1..=limit)
        .rev()
        .find(|length| value[value.len() - length..].eq_ignore_ascii_case(&tag[..*length]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use minimax_protocol::{StreamEvent, TerminalOutcome};

    use super::ChatReasoningFilter;

    #[test]
    fn incomplete_reasoning_boundaries_fail_closed() {
        let mut filter = ChatReasoningFilter::default();
        let events = filter.accept(StreamEvent::VisibleTextDelta {
            delta: "visible<THI".to_owned(),
        });
        assert_eq!(
            events,
            vec![StreamEvent::VisibleTextDelta {
                delta: "visible".to_owned()
            }]
        );
        assert_eq!(
            filter.accept(StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed
            }),
            vec![StreamEvent::Terminal {
                outcome: TerminalOutcome::Completed
            }]
        );
    }
}
