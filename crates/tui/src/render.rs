use std::collections::BTreeMap;

use minimax_protocol::{
    CapabilityKind, CapabilityReadiness, CapabilityWorkspaceResponse,
    CapabilityWorkspaceStatusRecord, ForgetPlan, GcClass, GcPlan, IndexDomain, IndexStatusRecord,
    RetrievalDegradedReason, RetrievalMode, RetrievalResponse, RuntimeEvent, RuntimeEventV1,
    RuntimeTerminalOutcome, SHELL_TOOL_NAMES, SessionRecord, SessionStatus, ShellReceipt,
    ShellSessionState, ToolEffect, ToolInvocation, ToolResult, TraceCode, TraceEntry,
    VaultLintReport,
};

const MAX_RENDER_CHARS: usize = 16_000;

pub struct EventRenderer;

impl EventRenderer {
    #[must_use]
    pub fn index_status(status: &IndexStatusRecord) -> String {
        sanitize_bounded(&format!(
            "index status | domain={} | documents={} | mode={} | degraded={} | source={} | fingerprint={}",
            domain_name(status.domain),
            status.documents,
            mode_name(status.mode),
            status.degraded_reason.map_or("none", degraded_reason_name),
            status.source,
            status.fingerprint.as_deref().unwrap_or("none")
        ))
    }

    #[must_use]
    pub fn retrieval(response: &RetrievalResponse) -> String {
        let mut lines = vec![format!(
            "retrieval | domain={} | query={} | mode={} | degraded={} | keywords={}",
            domain_name(response.domain),
            response.query,
            mode_name(response.mode),
            response
                .degraded_reason
                .map_or("none", degraded_reason_name),
            if response.keywords.is_empty() {
                "none".to_owned()
            } else {
                response.keywords.join(",")
            }
        )];
        for hit in &response.results {
            let mut facts = vec![
                format!("id={}", hit.id),
                format!("title={}", hit.title),
                format!("lexical_rank={}", hit.explanation.lexical_rank),
                format!("lexical_score={:.6}", hit.explanation.lexical_score),
                format!(
                    "matched_terms={}",
                    if hit.explanation.matched_terms.is_empty() {
                        "none".to_owned()
                    } else {
                        hit.explanation.matched_terms.join(",")
                    }
                ),
            ];
            if response.domain == IndexDomain::Project {
                facts.extend([
                    format!("source={}", hit.source_url.as_deref().unwrap_or("unknown")),
                    format!(
                        "repository={}",
                        hit.repository_url.as_deref().unwrap_or("unknown")
                    ),
                    format!("license={}", hit.license.as_deref().unwrap_or("unknown")),
                    format!(
                        "platforms={}",
                        if hit.platforms.is_empty() {
                            "unknown".to_owned()
                        } else {
                            hit.platforms.join(",")
                        }
                    ),
                    format!(
                        "last_activity={}",
                        hit.last_activity.as_deref().unwrap_or("unknown")
                    ),
                    format!(
                        "latest_release={}",
                        hit.latest_release.as_deref().unwrap_or("unknown")
                    ),
                    format!(
                        "maintenance={}",
                        if hit.maintenance.is_empty() {
                            "unknown".to_owned()
                        } else {
                            hit.maintenance.join(",")
                        }
                    ),
                    format!("confidence_penalty={}", hit.confidence_penalty),
                ]);
            }
            if let Some(rank) = hit.explanation.semantic_rank {
                facts.push(format!("semantic_rank={rank}"));
            }
            if let Some(score) = hit.explanation.fused_score {
                facts.push(format!("fused_score={score:.6}"));
            }
            lines.push(format!("result | {}", facts.join(" | ")));
        }
        sanitize_bounded(&lines.join("\n"))
    }

    #[must_use]
    pub fn capability_workspace_status(status: &CapabilityWorkspaceStatusRecord) -> String {
        let mut lines = vec![format!(
            "capability workspace | fingerprint={}",
            status.workspace_fingerprint
        )];
        lines.extend(status.catalogs.iter().map(|catalog| {
            format!(
                "catalog | kind={} | documents={} | mode={} | degraded={} | source={} | fingerprint={}",
                domain_name(catalog.domain),
                catalog.documents,
                mode_name(catalog.mode),
                catalog
                    .degraded_reason
                    .map_or("none", degraded_reason_name),
                catalog.source,
                catalog.fingerprint.as_deref().unwrap_or("none")
            )
        }));
        sanitize_bounded(&lines.join("\n"))
    }

    #[must_use]
    pub fn capability_workspace(response: &CapabilityWorkspaceResponse) -> String {
        let mut lines = vec![format!(
            "capability workspace search | query={} | kind={} | mode={} | degraded={} | keywords={}",
            response.query,
            response.selected_kind.map_or("all", capability_kind_name),
            mode_name(response.mode),
            response
                .degraded_reason
                .map_or("none", degraded_reason_name),
            if response.keywords.is_empty() {
                "none".to_owned()
            } else {
                response.keywords.join(",")
            }
        )];
        for hit in &response.results {
            lines.push(format!(
                "recommendation | kind={} | title={} | status={} | why={} | next={} | source={} | repository={} | license={} | platforms={} | permissions={} | authorization={} | matched_terms={} | lexical_rank={}{}",
                capability_kind_name(hit.kind),
                hit.title,
                readiness_name(hit.readiness),
                hit.readiness_reason,
                hit.next_action,
                hit.source_url,
                hit.repository_url.as_deref().unwrap_or("unknown"),
                hit.license.as_deref().unwrap_or("unknown"),
                optional_facts(hit.platforms.as_deref()),
                optional_facts(hit.permissions.as_deref()),
                optional_facts(hit.authorizations.as_deref()),
                if hit.explanation.matched_terms.is_empty() {
                    "none".to_owned()
                } else {
                    hit.explanation.matched_terms.join(",")
                },
                hit.explanation.lexical_rank,
                hit.explanation
                    .semantic_rank
                    .map_or_else(String::new, |rank| format!(" | semantic_rank={rank}"))
            ));
        }
        sanitize_bounded(&lines.join("\n"))
    }

    #[must_use]
    pub fn vault_lint(report: &VaultLintReport) -> String {
        if report.issues.is_empty() {
            return format!(
                "vault lint | project={} | clean",
                sanitize_bounded(report.project_id.as_str())
            );
        }
        let mut lines = vec![format!(
            "vault lint | project={} | issues={}",
            sanitize_bounded(report.project_id.as_str()),
            report.issues.len()
        )];
        lines.extend(report.issues.iter().map(|issue| {
            format!(
                "{:?} | path={}{}",
                issue.code,
                sanitize_bounded(&issue.relative_path),
                issue.related_id.as_deref().map_or_else(String::new, |id| {
                    format!(" | id={}", sanitize_bounded(id))
                })
            )
        }));
        lines.join("\n")
    }

    #[must_use]
    pub fn gc_plan(plan: &GcPlan, confirmation: &str) -> String {
        let eligible = plan
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(candidate.class, GcClass::Rebuildable | GcClass::Collectable)
            })
            .count();
        let bytes = plan
            .candidates
            .iter()
            .filter(|candidate| {
                matches!(candidate.class, GcClass::Rebuildable | GcClass::Collectable)
            })
            .fold(0_u64, |total, candidate| {
                total.saturating_add(candidate.bytes)
            });
        format!(
            "gc report | id={} | eligible={} | bytes={} | protected={}\nconfirmation: {}",
            sanitize_bounded(plan.gc_id.as_str()),
            eligible,
            bytes,
            plan.candidates.len().saturating_sub(eligible),
            sanitize_bounded(confirmation)
        )
    }

    #[must_use]
    pub fn forget_plan(plan: &ForgetPlan, confirmation: &str) -> String {
        let mut lines = vec![format!(
            "forget plan | id={} | affected_claims={} | evidence_hash={}",
            sanitize_bounded(plan.forget_id.as_str()),
            plan.affected_page_paths.len(),
            plan.expected_hash.as_str()
        )];
        lines.extend(
            plan.affected_page_paths
                .iter()
                .map(|path| format!("claim path={}", sanitize_bounded(path))),
        );
        lines.push(format!("confirmation: {}", sanitize_bounded(confirmation)));
        lines.join("\n")
    }

    #[must_use]
    pub fn event(record: &RuntimeEventV1) -> String {
        let rendered = match &record.event {
            RuntimeEvent::TurnStarted {
                session_id,
                turn_id,
                request_id,
            } => format!(
                "turn started | session={} | turn={} | request={}",
                session_id.as_str(),
                turn_id.as_str(),
                request_id.as_str()
            ),
            RuntimeEvent::VisibleTextDelta { delta } => delta.clone(),
            RuntimeEvent::ReasoningFiltered => "hidden reasoning filtered".to_owned(),
            RuntimeEvent::ToolCallObserved { call_id, name } => format!(
                "tool request observed | call={} | name={}",
                call_id.as_str(),
                name.as_deref().unwrap_or("unknown")
            ),
            RuntimeEvent::Usage { usage } => format!(
                "usage | input={} | output={} | total={}",
                render_token_count(usage.input_tokens),
                render_token_count(usage.output_tokens),
                render_token_count(usage.total_tokens)
            ),
            RuntimeEvent::Diagnostic { code } => format!("diagnostic | {code:?}"),
            RuntimeEvent::Terminal { outcome } => terminal(outcome),
        };
        sanitize_bounded(&rendered)
    }

    #[must_use]
    pub fn history(session: &SessionRecord) -> String {
        let mut lines = vec![format!(
            "session {} | {:?}",
            session.session_id.as_str(),
            session.status
        )];
        for turn in &session.turns {
            lines.push(format!(
                "user [{}]: {}",
                turn.turn_id.as_str(),
                turn.user_message.content
            ));
            if let Some(assistant) = &turn.assistant_message {
                let suffix = if assistant.partial {
                    format!(" [partial {:?}]", turn.status)
                } else {
                    String::new()
                };
                lines.push(format!("assistant: {}{suffix}", assistant.content));
            }
        }
        sanitize_bounded(&lines.join("\n"))
    }

    #[must_use]
    pub fn sessions(sessions: &[(&str, SessionStatus, u64, usize)]) -> String {
        if sessions.is_empty() {
            return "no sessions".to_owned();
        }
        sanitize_bounded(
            &sessions
                .iter()
                .map(|(id, status, updated, turns)| {
                    let marker = if *status == SessionStatus::Active {
                        "*"
                    } else {
                        " "
                    };
                    format!("{marker} {id} | {status:?} | updated={updated} | turns={turns}")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    #[must_use]
    pub fn trace(entries: &[TraceEntry], expanded: bool) -> String {
        if expanded {
            return sanitize_bounded(
                &entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{} | {:?} | {}",
                            entry.recorded_at_unix_ms,
                            entry.code,
                            render_facts(&entry.facts)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        let mut counts = BTreeMap::<TraceCode, u64>::new();
        for entry in entries {
            *counts.entry(entry.code).or_insert(0) += 1;
        }
        sanitize_bounded(
            &counts
                .into_iter()
                .map(|(code, count)| format!("{code:?}={count}"))
                .collect::<Vec<_>>()
                .join(" | "),
        )
    }

    #[must_use]
    pub fn not_available(command: &str, owning_phase: u8) -> String {
        sanitize_bounded(&format!(
            "{command} is not available in the Rust development shell until Phase {owning_phase}"
        ))
    }

    #[must_use]
    pub fn approval_request(invocation: &ToolInvocation) -> String {
        let mut value = serde_json::from_str::<serde_json::Value>(&invocation.call.arguments_json)
            .unwrap_or(serde_json::Value::Null);
        let scope = value
            .get("path")
            .and_then(|path| path.as_str())
            .map(|path| path.replace('\\', "/"))
            .unwrap_or_else(|| "project".to_owned());
        if let Some(path) = value.get_mut("path")
            && let Some(raw) = path.as_str()
        {
            *path = serde_json::Value::String(raw.replace('\\', "/"));
        }
        let arguments = serde_json::to_string(&value).unwrap_or_else(|_| "<invalid>".to_owned());
        sanitize_bounded(&format!(
            "approval required | call={} | tool={} | effect={} | scope={} | arguments={}\nType exactly yes to allow this one call: ",
            invocation.call.call_id.as_str(),
            invocation.call.name,
            effect_name(invocation.effect),
            scope,
            arguments
        ))
    }

    #[must_use]
    pub fn tool_result(result: &ToolResult) -> String {
        if SHELL_TOOL_NAMES.contains(&result.tool_name.as_str())
            && let Some(output) = result.output.as_deref()
            && let Ok(receipt) = serde_json::from_str::<ShellReceipt>(output)
        {
            return render_shell_receipt(result, &receipt);
        }
        render_generic_tool_result(result)
    }
}

fn render_shell_receipt(_result: &ToolResult, receipt: &ShellReceipt) -> String {
    let header = format!(
        "shell | session={} | state={} | exit={} | truncated={}",
        receipt.session_id.as_str(),
        shell_state_name(receipt.state),
        receipt
            .exit_code
            .map_or_else(|| "none".to_owned(), |exit_code| exit_code.to_string()),
        receipt.output_truncated
    );
    if receipt.output.is_empty() {
        sanitize_bounded(&header)
    } else {
        sanitize_bounded(&format!("{header}\n{}", receipt.output))
    }
}

fn render_generic_tool_result(result: &ToolResult) -> String {
    sanitize_bounded(&format!(
        "tool result | call={} | tool={} | status={:?} | code={}{}",
        result.call_id.as_str(),
        result.tool_name,
        result.status,
        result.code,
        result
            .output
            .as_deref()
            .map_or_else(String::new, |output| format!(" | output={output}"))
    ))
}

const fn shell_state_name(state: ShellSessionState) -> &'static str {
    match state {
        ShellSessionState::Running => "running",
        ShellSessionState::Exited => "exited",
        ShellSessionState::Stopped => "stopped",
        ShellSessionState::Failed => "failed",
    }
}

const fn domain_name(domain: IndexDomain) -> &'static str {
    match domain {
        IndexDomain::Capability => "capability",
        IndexDomain::Project => "project",
        IndexDomain::Skill => "skill",
        IndexDomain::Mcp => "mcp",
        IndexDomain::Wiki => "wiki",
    }
}

const fn capability_kind_name(kind: CapabilityKind) -> &'static str {
    match kind {
        CapabilityKind::Project => "project",
        CapabilityKind::Skill => "skill",
        CapabilityKind::Mcp => "mcp",
    }
}

const fn readiness_name(readiness: CapabilityReadiness) -> &'static str {
    match readiness {
        CapabilityReadiness::Ready => "ready",
        CapabilityReadiness::NeedsInstall => "needs_install",
        CapabilityReadiness::NeedsAccess => "needs_authorization",
    }
}

fn optional_facts(values: Option<&[String]>) -> String {
    values.map_or_else(|| "unknown".to_owned(), |values| values.join(","))
}

const fn mode_name(mode: RetrievalMode) -> &'static str {
    match mode {
        RetrievalMode::Exact => "exact",
        RetrievalMode::Bm25 => "bm25",
        RetrievalMode::HybridVerified => "hybrid_verified",
    }
}

const fn degraded_reason_name(reason: RetrievalDegradedReason) -> &'static str {
    match reason {
        RetrievalDegradedReason::EmbeddingMissing => "embedding_missing",
        RetrievalDegradedReason::IncompatibleCpu => "incompatible_cpu",
        RetrievalDegradedReason::InvalidManifest => "invalid_manifest",
        RetrievalDegradedReason::HashMismatch => "hash_mismatch",
        RetrievalDegradedReason::RuntimeAbiMismatch => "runtime_abi_mismatch",
        RetrievalDegradedReason::FingerprintMismatch => "fingerprint_mismatch",
        RetrievalDegradedReason::HelperUnavailable => "helper_unavailable",
        RetrievalDegradedReason::HelperTimeout => "helper_timeout",
        RetrievalDegradedReason::HelperCrashed => "helper_crashed",
        RetrievalDegradedReason::MalformedVector => "malformed_vector",
        RetrievalDegradedReason::NonFiniteVector => "non_finite_vector",
        RetrievalDegradedReason::WrongDimension => "wrong_dimension",
    }
}

const fn effect_name(effect: ToolEffect) -> &'static str {
    match effect {
        ToolEffect::Read => "read",
        ToolEffect::Write => "write",
        ToolEffect::Process => "process",
    }
}

fn terminal(outcome: &RuntimeTerminalOutcome) -> String {
    match outcome {
        RuntimeTerminalOutcome::Completed => "terminal | completed".to_owned(),
        RuntimeTerminalOutcome::Interrupted => "terminal | interrupted".to_owned(),
        RuntimeTerminalOutcome::Stopped => "terminal | stopped".to_owned(),
        RuntimeTerminalOutcome::Failed { failure } => format!("terminal | failed | {failure}"),
    }
}

fn render_facts(facts: &BTreeMap<String, String>) -> String {
    facts
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_token_count(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".to_owned(), |value| value.to_string())
}

fn sanitize_bounded(value: &str) -> String {
    let mut rendered = String::new();
    let mut rendered_chars = 0_usize;
    for character in value.chars() {
        if rendered_chars >= MAX_RENDER_CHARS {
            rendered.push('…');
            break;
        }
        match character {
            '\n' => {
                rendered.push('\n');
                rendered_chars += 1;
            }
            '\t' => {
                let spaces = (MAX_RENDER_CHARS - rendered_chars).min(4);
                rendered.extend(std::iter::repeat_n(' ', spaces));
                rendered_chars += spaces;
            }
            character if character.is_control() => {
                rendered.push('�');
                rendered_chars += 1;
            }
            character => {
                rendered.push(character);
                rendered_chars += 1;
            }
        }
    }
    rendered
}
