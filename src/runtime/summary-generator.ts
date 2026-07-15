import type {ThreadItem} from "../types.js";

export type CompactReason = "manual" | "auto" | "resume";

export interface SummaryGenerator {
  generate(items: ThreadItem[], reason: CompactReason): Promise<string>;
}

interface VisibleEntry {
  item: ThreadItem;
  content: string;
}

interface SummarySection {
  heading: string;
  entries: Array<{label?: string; content: string}>;
}

const SUMMARY_LIMIT = 4096;
const ENTRY_LIMIT = 480;
const MAX_CATEGORIZED_ENTRIES = 4;
const CONSTRAINT_PATTERN =
  /\b(?:cannot|constraints?|do not|don't|must|need(?:s|ed)? to|never|only|prohibit(?:ed|ion)?|requirements?|requires?|shall|should)\b|(?:不得|不能|不要|仅限|必须|禁止|约束|要求)/iu;
const DECISION_PATTERN =
  /\b(?:adopt(?:ed)?|agreed|choose|chosen|chose|decided|decisions?|opted|pick(?:ed)?|prefer(?:red)?|select(?:ed)?|use)\b|(?:决定|同意|确定|选择|采用)/iu;
const OPEN_ITEM_PATTERN =
  /[?？]|\b(?:blocked|open item|pending|remains|todo|unknown|unresolved|whether|which)\b|(?:待办|待定|未决|未解决|阻塞)/iu;
const STANDALONE_CREDENTIAL_PATTERN =
  /\b(?:gh[pousr]_[A-Za-z0-9]{20,255}|github_pat_[A-Za-z0-9_]{20,255}|glpat-[A-Za-z0-9_-]{20,255}|(?:ABIA|ACCA|AGPA|AIDA|AIPA|AKIA|ANPA|ANVA|APKA|AROA|ASCA|ASIA)[A-Z0-9]{16})\b/gu;
const BOUNDED_TOKEN_CANDIDATE = /[A-Za-z0-9][A-Za-z0-9_+./=-]{31,511}/gu;

export class StructuredLocalSummaryGenerator implements SummaryGenerator {
  async generate(items: ThreadItem[], reason: CompactReason): Promise<string> {
    const visibleEntries = items.flatMap((item) => {
      if (!isCompletedVisibleMessage(item)) {
        return [];
      }
      const content = sanitizeContent(item.content);
      return content ? [{item, content}] : [];
    });
    const originalGoal = visibleEntries.find((entry) => entry.item.type === "user_message");
    const constraints = selectCategorized(visibleEntries, CONSTRAINT_PATTERN, "first");
    const decisions = selectCategorized(visibleEntries, DECISION_PATTERN, "last");
    const userOpenItems = selectCategorized(
      visibleEntries.filter((entry) => entry.item.type === "user_message"),
      OPEN_ITEM_PATTERN,
      "last"
    );
    const assistantOpenItems = selectCategorized(
      visibleEntries.filter((entry) => entry.item.type === "assistant_message"),
      OPEN_ITEM_PATTERN,
      "last"
    );
    const errorOpenItems = items
      .filter((item) => item.type === "error")
      .map((item) => ({
        item,
        content: item.turnId
          ? `An error occurred during turn ${item.turnId}.`
          : "An error occurred in the covered conversation."
      }));
    const prioritizedOpenItems = uniqueEntries([
      ...userOpenItems,
      ...errorOpenItems
    ]).slice(-MAX_CATEGORIZED_ENTRIES);
    const prioritizedContent = new Set(
      prioritizedOpenItems.map((entry) => entry.content)
    );
    const assistantSlots = MAX_CATEGORIZED_ENTRIES - prioritizedOpenItems.length;
    const assistantSupplement = assistantSlots > 0
      ? uniqueEntries(assistantOpenItems)
        .filter((entry) => !prioritizedContent.has(entry.content))
        .slice(-assistantSlots)
      : [];
    const openItems = [...prioritizedOpenItems, ...assistantSupplement];

    const recentEntries = completedExchanges(visibleEntries)
      .slice(-3)
      .flatMap((exchange) => [
        {label: "User", content: exchange.user.content},
        {label: "Assistant", content: exchange.assistant.content}
      ]);
    const sections: SummarySection[] = [
      {
        heading: "Original goal:",
        entries: [{content: originalGoal?.content ?? "None captured."}]
      },
      {
        heading: "Constraints:",
        entries: toSectionEntries(constraints)
      },
      {
        heading: "Decisions:",
        entries: toSectionEntries(decisions)
      },
      {
        heading: "Open items:",
        entries: toSectionEntries(openItems)
      },
      {
        heading: "Recent exchanges:",
        entries: recentEntries.length > 0 ? recentEntries : [{content: "None captured."}]
      }
    ];

    return renderBoundedSummary(reason, sections);
  }
}

export {StructuredLocalSummaryGenerator as LocalSummaryGenerator};

function isCompletedVisibleMessage(item: ThreadItem): boolean {
  if (item.type === "user_message") {
    return true;
  }
  return item.type === "assistant_message" && item.metadata?.partial !== true;
}

function sanitizeContent(content: string): string {
  return redactSecrets(stripReasoning(content)).replace(/\s+/gu, " ").trim();
}

function stripReasoning(content: string): string {
  return content
    .replace(/<(think|analysis|reasoning)\b[^>]*>[\s\S]*?<\/\1>/giu, " ")
    .replace(/<(?:think|analysis|reasoning)\b[^>]*>[\s\S]*$/giu, " ");
}

function redactSecrets(content: string): string {
  return content
    .replace(/-----BEGIN [^-]+ PRIVATE KEY-----[\s\S]*?-----END [^-]+ PRIVATE KEY-----/giu, "[REDACTED]")
    .replace(/\bBearer\s+[A-Za-z0-9._~+/-]+=*/giu, "Bearer [REDACTED]")
    .replace(/\bsk-[A-Za-z0-9_-]{8,}\b/gu, "[REDACTED]")
    .replace(STANDALONE_CREDENTIAL_PATTERN, "[REDACTED]")
    .replace(
      /\b((?:[A-Z][A-Z0-9_]*_)?(?:API_?KEY|PASSWORD|SECRET|TOKEN)|AUTHORIZATION)\b\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^\s,;]+)/giu,
      "$1=[REDACTED]"
    )
    .replace(/(https?:\/\/[^\s/:@]+:)[^\s@]+@/giu, "$1[REDACTED]@")
    .replace(BOUNDED_TOKEN_CANDIDATE, (candidate) =>
      isHighEntropyToken(candidate) ? "[REDACTED]" : candidate
    );
}

function isHighEntropyToken(candidate: string): boolean {
  const characterClasses = [
    /[a-z]/u,
    /[A-Z]/u,
    /[0-9]/u,
    /[_+./=-]/u
  ].filter((pattern) => pattern.test(candidate)).length;
  const hasLetterAndDigit = /[A-Za-z]/u.test(candidate) && /[0-9]/u.test(candidate);
  if (
    new Set(candidate).size < 10 ||
    (characterClasses < 3 && !hasLetterAndDigit)
  ) {
    return false;
  }

  const counts = new Map<string, number>();
  for (const character of candidate) {
    counts.set(character, (counts.get(character) ?? 0) + 1);
  }
  const entropy = Array.from(counts.values()).reduce((total, count) => {
    const probability = count / candidate.length;
    return total - probability * Math.log2(probability);
  }, 0);
  return entropy >= 3.5;
}

function selectCategorized(
  entries: VisibleEntry[],
  pattern: RegExp,
  edge: "first" | "last"
): VisibleEntry[] {
  const matches = uniqueEntries(entries.filter((entry) => pattern.test(entry.content)));
  return edge === "first"
    ? matches.slice(0, MAX_CATEGORIZED_ENTRIES)
    : matches.slice(-MAX_CATEGORIZED_ENTRIES);
}

function uniqueEntries(entries: VisibleEntry[]): VisibleEntry[] {
  const seen = new Set<string>();
  return entries.filter((entry) => {
    if (seen.has(entry.content)) {
      return false;
    }
    seen.add(entry.content);
    return true;
  });
}

function toSectionEntries(entries: VisibleEntry[]): SummarySection["entries"] {
  return entries.length > 0
    ? entries.map((entry) => ({content: entry.content}))
    : [{content: "None captured."}];
}

function completedExchanges(entries: VisibleEntry[]): Array<{
  user: VisibleEntry;
  assistant: VisibleEntry;
}> {
  const pendingByTurn = new Map<string, VisibleEntry>();
  const pendingUsers: VisibleEntry[] = [];
  const exchanges: Array<{user: VisibleEntry; assistant: VisibleEntry}> = [];

  for (const entry of entries) {
    if (entry.item.type === "user_message") {
      pendingUsers.push(entry);
      if (entry.item.turnId) {
        pendingByTurn.set(entry.item.turnId, entry);
      }
      continue;
    }

    const user = entry.item.turnId
      ? pendingByTurn.get(entry.item.turnId) ?? pendingUsers.at(-1)
      : pendingUsers.at(-1);
    if (!user) {
      continue;
    }
    exchanges.push({user, assistant: entry});
    if (user.item.turnId) {
      pendingByTurn.delete(user.item.turnId);
    }
    pendingUsers.splice(pendingUsers.indexOf(user), 1);
  }

  return exchanges;
}

function renderBoundedSummary(reason: CompactReason, sections: SummarySection[]): string {
  const prefix = `Compaction reason: ${reason}`;
  const entryCount = sections.reduce((total, section) => total + section.entries.length, 0);
  const headingsLength = sections.reduce(
    (total, section) => total + section.heading.length,
    0
  );
  const fixedLength =
    prefix.length +
    2 +
    headingsLength +
    entryCount +
    Math.max(0, 2 * (sections.length - 1));
  const perEntryLimit = Math.min(
    ENTRY_LIMIT,
    Math.max(2, Math.floor((SUMMARY_LIMIT - fixedLength) / Math.max(1, entryCount)))
  );
  const renderedSections = sections.map((section) => [
    section.heading,
    ...section.entries.map((entry) => renderEntry(entry, perEntryLimit))
  ].join("\n"));

  return `${prefix}\n\n${renderedSections.join("\n\n")}`;
}

function renderEntry(
  entry: {label?: string; content: string},
  limit: number
): string {
  const label = entry.label ? `- ${entry.label}: ` : "- ";
  const available = Math.max(0, limit - label.length);
  if (entry.content.length <= available) {
    return `${label}${entry.content}`;
  }
  if (available <= 1) {
    return label.slice(0, limit);
  }
  return `${label}${entry.content.slice(0, available - 1)}…`;
}
