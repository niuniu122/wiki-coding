import type {ModelContextMessage} from "../types.js";

const SAFETY_MARGIN = 1.15;
const MESSAGE_OVERHEAD = 4;
const LATIN_PROSE_CHARS_PER_TOKEN = 4;
const CODE_CHARS_PER_TOKEN = 3;
const CJK_OR_EMOJI_COMPONENT =
  /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{Extended_Pictographic}\u{1F1E6}-\u{1F1FF}\u{1F3FB}-\u{1F3FF}\u{200D}\u{20E3}\u{FE0E}-\u{FE0F}\u{E0020}-\u{E007F}]/u;
const KEYCAP_BASE = /[0-9#*]/u;
const CODE_KEYWORD =
  /\b(?:async|await|class|const|delete|else|export|for|function|if|import|insert|interface|let|return|select|throw|type|update|var|while)\b/i;
const CODE_PUNCTUATION = /[{}[\]();=<>`\\]/u;

export interface TokenEstimator {
  estimateText(text: string): number;
  estimateMessages(messages: ModelContextMessage[]): number;
}

export class ConservativeTokenEstimator implements TokenEstimator {
  estimateText(text: string): number {
    return withSafetyMargin(estimateRawText(text));
  }

  estimateMessages(messages: ModelContextMessage[]): number {
    const raw = messages.reduce(
      (total, message) => total + MESSAGE_OVERHEAD + estimateRawText(message.content),
      0
    );
    return withSafetyMargin(raw);
  }
}

function estimateRawText(text: string): number {
  const codePoints = Array.from(text);
  const charsPerToken = isCodeHeavy(text, codePoints)
    ? CODE_CHARS_PER_TOKEN
    : LATIN_PROSE_CHARS_PER_TOKEN;
  let individualTokens = 0;
  let groupedCharacters = 0;

  for (const [index, codePoint] of codePoints.entries()) {
    if (
      CJK_OR_EMOJI_COMPONENT.test(codePoint) ||
      isKeycapBase(codePoint, index, codePoints)
    ) {
      individualTokens++;
    } else {
      groupedCharacters++;
    }
  }

  return individualTokens + groupedCharacters / charsPerToken;
}

function isKeycapBase(
  codePoint: string,
  index: number,
  codePoints: string[]
): boolean {
  if (!KEYCAP_BASE.test(codePoint)) {
    return false;
  }
  return (
    codePoints[index + 1] === "\u20E3" ||
    (codePoints[index + 1] === "\uFE0F" && codePoints[index + 2] === "\u20E3")
  );
}

function isCodeHeavy(text: string, codePoints: string[]): boolean {
  if (text.includes("```") || CODE_KEYWORD.test(text)) {
    return true;
  }

  const syntaxCharacters = codePoints.filter((codePoint) =>
    CODE_PUNCTUATION.test(codePoint)
  ).length;
  return codePoints.length > 0 && syntaxCharacters / codePoints.length >= 0.12;
}

function withSafetyMargin(raw: number): number {
  return Math.ceil(raw * SAFETY_MARGIN);
}
