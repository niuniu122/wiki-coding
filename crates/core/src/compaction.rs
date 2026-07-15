use std::collections::BTreeSet;
use std::fmt;

use minimax_protocol::{
    CompactionId, CompactionRecentTurn, CompactionRecord, SessionRecord, TurnStatus,
};

const MAX_ENTRY_BYTES: usize = 4096;
const MAX_CATEGORY_ENTRIES: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionBudget {
    pub max_record_bytes: usize,
    pub retain_recent_turns: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompactionError {
    NoCompletedTurns,
    EntryTooLarge,
    BudgetTooSmall,
    Serialization,
}

impl fmt::Display for CompactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NoCompletedTurns => "no completed visible exchange is available for compaction",
            Self::EntryTooLarge => "a complete visible message exceeds the compaction entry limit",
            Self::BudgetTooSmall => {
                "the compaction budget cannot retain the required whole entries"
            }
            Self::Serialization => "the compaction record could not be serialized",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for CompactionError {}

pub struct LocalCompactor;

impl LocalCompactor {
    pub fn compact(
        session: &SessionRecord,
        compaction_id: CompactionId,
        budget: CompactionBudget,
    ) -> Result<CompactionRecord, CompactionError> {
        if budget.max_record_bytes == 0 || budget.retain_recent_turns == 0 {
            return Err(CompactionError::BudgetTooSmall);
        }
        let mut exchanges = Vec::new();
        for turn in &session.turns {
            if turn.status != TurnStatus::Completed {
                continue;
            }
            let Some(assistant) = turn
                .assistant_message
                .as_ref()
                .filter(|message| !message.partial)
            else {
                continue;
            };
            let user = sanitize_visible(&turn.user_message.content)?;
            let assistant = sanitize_visible(&assistant.content)?;
            if user.is_empty() || assistant.is_empty() {
                continue;
            }
            exchanges.push(CompactionRecentTurn {
                turn_id: turn.turn_id.clone(),
                user,
                assistant,
            });
        }
        let covered_through_turn_id = exchanges
            .last()
            .map(|exchange| exchange.turn_id.clone())
            .ok_or(CompactionError::NoCompletedTurns)?;
        let all_messages = exchanges
            .iter()
            .flat_map(|exchange| [&exchange.user, &exchange.assistant])
            .cloned()
            .collect::<Vec<_>>();
        let before_estimated_tokens = estimate_tokens(
            all_messages
                .iter()
                .map(String::len)
                .try_fold(0_usize, |total, len| total.checked_add(len))
                .ok_or(CompactionError::Serialization)?,
        )?;
        let goal = vec![exchanges[0].user.clone()];
        let constraints = categorized(&all_messages, Category::Constraint, Edge::First);
        let decisions = categorized(&all_messages, Category::Decision, Edge::Last);
        let open_items = categorized(&all_messages, Category::OpenItem, Edge::Last);
        let retained_recent_turns = exchanges
            .into_iter()
            .rev()
            .take(budget.retain_recent_turns)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let mut record = CompactionRecord {
            compaction_id,
            covered_through_turn_id,
            goal,
            constraints,
            decisions,
            open_items,
            retained_recent_turns,
            before_estimated_tokens,
            after_estimated_tokens: 0,
        };
        fit_record(&mut record, budget.max_record_bytes)?;
        Ok(record)
    }
}

#[derive(Clone, Copy)]
enum Category {
    Constraint,
    Decision,
    OpenItem,
}

#[derive(Clone, Copy)]
enum Edge {
    First,
    Last,
}

fn categorized(messages: &[String], category: Category, edge: Edge) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut selected = messages
        .iter()
        .filter(|message| matches_category(message, category))
        .filter(|message| seen.insert((*message).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if selected.len() > MAX_CATEGORY_ENTRIES {
        match edge {
            Edge::First => selected.truncate(MAX_CATEGORY_ENTRIES),
            Edge::Last => {
                selected.drain(0..selected.len() - MAX_CATEGORY_ENTRIES);
            }
        }
    }
    selected
}

fn matches_category(message: &str, category: Category) -> bool {
    let lower = message.to_lowercase();
    match category {
        Category::Constraint => contains_any(
            &lower,
            &[
                "must",
                "cannot",
                "do not",
                "don't",
                "never",
                "only",
                "constraint",
                "requirement",
                "必须",
                "不能",
                "不要",
                "仅限",
                "禁止",
                "约束",
                "要求",
            ],
        ),
        Category::Decision => contains_any(
            &lower,
            &[
                "decided", "decision", "choose", "chosen", "chose", "selected", "adopt", "use ",
                "决定", "同意", "确定", "选择", "采用",
            ],
        ),
        Category::OpenItem => {
            lower.contains(['?', '？'])
                || contains_any(
                    &lower,
                    &[
                        "pending",
                        "todo",
                        "unresolved",
                        "open item",
                        "blocked",
                        "unknown",
                        "待办",
                        "待定",
                        "未决",
                        "未解决",
                        "阻塞",
                    ],
                )
        }
    }
}

fn contains_any(value: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| value.contains(pattern))
}

fn fit_record(record: &mut CompactionRecord, max_bytes: usize) -> Result<(), CompactionError> {
    loop {
        stabilize_after_estimate(record)?;
        let len = serde_json::to_vec(record)
            .map_err(|_| CompactionError::Serialization)?
            .len();
        if len <= max_bytes {
            return Ok(());
        }
        if record.open_items.pop().is_some()
            || record.decisions.pop().is_some()
            || record.constraints.pop().is_some()
        {
            continue;
        }
        return Err(CompactionError::BudgetTooSmall);
    }
}

fn stabilize_after_estimate(record: &mut CompactionRecord) -> Result<(), CompactionError> {
    for _ in 0..4 {
        let bytes = serde_json::to_vec(record)
            .map_err(|_| CompactionError::Serialization)?
            .len();
        let estimate = estimate_tokens(bytes)?;
        if estimate == record.after_estimated_tokens {
            return Ok(());
        }
        record.after_estimated_tokens = estimate;
    }
    Ok(())
}

fn estimate_tokens(bytes: usize) -> Result<u64, CompactionError> {
    let rounded = bytes.checked_add(3).ok_or(CompactionError::Serialization)? / 4;
    u64::try_from(rounded).map_err(|_| CompactionError::Serialization)
}

fn sanitize_visible(content: &str) -> Result<String, CompactionError> {
    let mut value = content.to_owned();
    for tag in ["think", "analysis", "reasoning"] {
        value = strip_tag_blocks(value, tag);
    }
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = redact_secrets(&normalized);
    if redacted.len() > MAX_ENTRY_BYTES {
        return Err(CompactionError::EntryTooLarge);
    }
    Ok(redacted)
}

fn strip_tag_blocks(mut value: String, tag: &str) -> String {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(start) = lower.find(&open) else {
            return value;
        };
        let Some(open_end_offset) = lower[start..].find('>') else {
            value.truncate(start);
            return value;
        };
        let content_start = start + open_end_offset + 1;
        let Some(close_offset) = lower[content_start..].find(&close) else {
            value.truncate(start);
            return value;
        };
        let end = content_start + close_offset + close.len();
        value.replace_range(start..end, " ");
    }
}

fn redact_secrets(content: &str) -> String {
    if content.to_lowercase().contains("private key-----") {
        return "[REDACTED]".to_owned();
    }
    let mut redact_next = false;
    content
        .split_whitespace()
        .map(|token| {
            if redact_next {
                redact_next = false;
                return "[REDACTED]".to_owned();
            }
            let lower = token.to_ascii_lowercase();
            if lower == "bearer" {
                redact_next = true;
                return token.to_owned();
            }
            if is_sensitive_token(&lower, token) {
                "[REDACTED]".to_owned()
            } else {
                token.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_sensitive_token(lower: &str, original: &str) -> bool {
    let named_secret = [
        "api_key=",
        "apikey=",
        "password=",
        "secret=",
        "token=",
        "authorization=",
        "sk-",
        "ghp_",
        "github_pat_",
        "glpat-",
        "do_not_persist",
        "raw_frame",
        "tool_body",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern));
    let high_entropy = original.len() >= 32
        && original.bytes().any(|byte| byte.is_ascii_alphabetic())
        && original.bytes().any(|byte| byte.is_ascii_digit())
        && original.bytes().collect::<BTreeSet<_>>().len() >= 10;
    named_secret || high_entropy
}
