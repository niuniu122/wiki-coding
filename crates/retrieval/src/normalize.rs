use std::collections::BTreeSet;

pub const QUERY_TOKENIZER_VERSION: &str = "mixed-zh-en-v1";

const STOPWORDS: &[&str] = &["a", "an", "and", "the", "to", "of", "please"];

#[must_use]
pub fn normalize_query(value: &str) -> String {
    value
        .chars()
        .map(fold_width)
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[must_use]
pub fn tokenize_query(value: &str) -> Vec<String> {
    let normalized = normalize_query(value);
    let mut raw = Vec::new();
    let mut current = String::new();
    let mut current_han = false;

    for character in normalized.chars().chain(std::iter::once(' ')) {
        let han = is_han(character);
        let latin = character.is_ascii_alphanumeric()
            || matches!(character, '.' | '_' | '@' | ':' | '/' | '-');
        if (han || latin) && (current.is_empty() || han == current_han) {
            current.push(character);
            current_han = han;
            continue;
        }
        if !current.is_empty() {
            emit_token(&current, current_han, &mut raw);
            current.clear();
        }
        if han || latin {
            current.push(character);
            current_han = han;
        }
    }

    let mut unique = BTreeSet::new();
    let mut tokens = Vec::new();
    for token in raw {
        if !token.is_empty() && !STOPWORDS.contains(&token.as_str()) && unique.insert(token.clone())
        {
            tokens.push(token.clone());
            for synonym in synonyms(&token) {
                if unique.insert((*synonym).to_owned()) {
                    tokens.push((*synonym).to_owned());
                }
            }
        }
    }
    tokens
}

fn emit_token(token: &str, han: bool, output: &mut Vec<String>) {
    output.push(token.to_owned());
    if han {
        let characters = token.chars().collect::<Vec<_>>();
        output.extend(characters.iter().map(char::to_string));
        output.extend(
            characters
                .windows(2)
                .map(|pair| pair.iter().collect::<String>()),
        );
    } else {
        output.extend(
            token
                .split(['.', '_', '@', ':', '/', '-'])
                .filter(|part| !part.is_empty())
                .map(str::to_owned),
        );
    }
}

fn synonyms(token: &str) -> &'static [&'static str] {
    match token {
        "查看" => &["read", "show", "inspect"],
        "搜索" => &["search", "find", "查找"],
        "检查" => &["check", "inspect", "diagnose"],
        "测试" => &["test", "check"],
        "文件" => &["file", "files"],
        "项目" => &["project", "workspace"],
        "read" | "show" | "inspect" => &["查看"],
        "search" | "find" => &["搜索", "查找"],
        "check" | "diagnose" => &["检查"],
        "test" | "tests" => &["测试"],
        "file" | "files" => &["文件"],
        "project" | "workspace" => &["项目"],
        _ => &[],
    }
}

fn is_han(character: char) -> bool {
    matches!(character as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn fold_width(character: char) -> char {
    match character as u32 {
        0xFF01..=0xFF5E => char::from_u32(character as u32 - 0xFEE0).unwrap_or(character),
        0x3000 => ' ',
        _ => character,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_tokens_are_deterministic() {
        assert_eq!(
            normalize_query("  ＲＥＡＤ   README.MD  "),
            "read readme.md"
        );
        let tokens = tokenize_query("搜索 project-code");
        for token in [
            "搜索",
            "搜",
            "索",
            "search",
            "find",
            "project-code",
            "project",
            "code",
        ] {
            assert!(
                tokens.contains(&token.to_owned()),
                "missing {token:?} in {tokens:?}"
            );
        }
    }
}
