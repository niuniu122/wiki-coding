const SYNONYMS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  查看: ["read", "show", "inspect"],
  搜索: ["search", "find", "查找"],
  检查: ["check", "inspect", "diagnose"],
  测试: ["test", "check"],
  文件: ["file", "files"],
  项目: ["project", "workspace"]
});

export const QUERY_TOKENIZER_VERSION = "mixed-zh-en-v1";

export function normalizeQuery(value: string): string {
  return value.normalize("NFKC").toLocaleLowerCase("en-US").trim().replace(/\s+/g, " ");
}

export function tokenizeQuery(value: string): string[] {
  const normalized = normalizeQuery(value);
  const tokens: string[] = [];
  for (const token of normalized.match(/[a-z0-9]+(?:[._@:/-][a-z0-9]+)*|[\p{Script=Han}]+/gu) ?? []) {
    tokens.push(token);
    if (/^[\p{Script=Han}]+$/u.test(token)) {
      const chars = [...token];
      tokens.push(...chars);
      for (let index = 0; index < chars.length - 1; index += 1) {
        tokens.push(`${chars[index]}${chars[index + 1]}`);
      }
    } else {
      tokens.push(...token.split(/[._@:/-]+/).filter(Boolean));
    }
    for (const synonym of SYNONYMS[token] ?? []) tokens.push(synonym);
  }
  const unique = [...new Set(tokens.filter(Boolean))];
  for (const token of [...unique]) {
    for (const synonym of SYNONYMS[token] ?? []) unique.push(synonym);
  }
  return [...new Set(unique)];
}
