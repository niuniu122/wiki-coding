use std::collections::BTreeMap;
use std::path::Path;

use minimax_protocol::{
    KnowledgePage, KnowledgePageStatus, PageId, SchemaVersion, SourceCitation, TopicId,
};

use crate::VaultError;

const FRONTMATTER_KEYS: [&str; 7] = [
    "schema_version",
    "page_id",
    "topic_id",
    "title",
    "status",
    "superseded_by",
    "sources",
];

pub fn render_wiki_page(page: &KnowledgePage) -> Result<Vec<u8>, VaultError> {
    let page = page
        .clone()
        .validate()
        .map_err(|_| VaultError::InvalidPage)?;
    validate_slug_path(&page.relative_path)?;
    let page_id = json_string(page.page_id.as_str())?;
    let topic_id = json_string(page.topic_id.as_str())?;
    let title = json_string(&page.title)?;
    let superseded_by = page
        .superseded_by
        .as_ref()
        .map(|value| json_string(value.as_str()))
        .transpose()?
        .unwrap_or_else(|| "null".to_owned());
    let sources = serde_json::to_string(&page.sources).map_err(|_| VaultError::InvalidPage)?;
    let status = match page.status {
        KnowledgePageStatus::Current => "current",
        KnowledgePageStatus::Superseded => "superseded",
    };
    let body = page.body.trim_end_matches(['\r', '\n']);
    Ok(format!(
        "---\nschema_version: 1\npage_id: {page_id}\ntopic_id: {topic_id}\ntitle: {title}\nstatus: {status}\nsuperseded_by: {superseded_by}\nsources: {sources}\n---\n{body}\n"
    )
    .into_bytes())
}

pub fn parse_wiki_page(relative_path: &str, bytes: &[u8]) -> Result<KnowledgePage, VaultError> {
    let text = std::str::from_utf8(bytes).map_err(|_| VaultError::InvalidPage)?;
    let rest = text.strip_prefix("---\n").ok_or(VaultError::InvalidPage)?;
    let (frontmatter, body) = rest.split_once("\n---\n").ok_or(VaultError::InvalidPage)?;
    let lines = frontmatter.lines().collect::<Vec<_>>();
    if lines.len() != FRONTMATTER_KEYS.len() {
        return Err(VaultError::InvalidPage);
    }
    let mut values = BTreeMap::new();
    for (line, expected_key) in lines.iter().zip(FRONTMATTER_KEYS) {
        let (key, value) = line.split_once(": ").ok_or(VaultError::InvalidPage)?;
        if key != expected_key || values.insert(key, value).is_some() {
            return Err(VaultError::InvalidPage);
        }
    }
    if values.get("schema_version") != Some(&"1") {
        return Err(VaultError::InvalidPage);
    }
    let status = match values.get("status").copied() {
        Some("current") => KnowledgePageStatus::Current,
        Some("superseded") => KnowledgePageStatus::Superseded,
        _ => return Err(VaultError::InvalidPage),
    };
    let superseded_by = match values.get("superseded_by").copied() {
        Some("null") => None,
        Some(value) => {
            Some(PageId::new(parse_json_string(value)?).map_err(|_| VaultError::InvalidPage)?)
        }
        None => return Err(VaultError::InvalidPage),
    };
    let sources = serde_json::from_str::<Vec<SourceCitation>>(
        values.get("sources").ok_or(VaultError::InvalidPage)?,
    )
    .map_err(|_| VaultError::InvalidPage)?;
    let page = KnowledgePage {
        schema_version: SchemaVersion,
        page_id: PageId::new(parse_json_string(
            values.get("page_id").ok_or(VaultError::InvalidPage)?,
        )?)
        .map_err(|_| VaultError::InvalidPage)?,
        topic_id: TopicId::new(parse_json_string(
            values.get("topic_id").ok_or(VaultError::InvalidPage)?,
        )?)
        .map_err(|_| VaultError::InvalidPage)?,
        relative_path: relative_path.to_owned(),
        title: parse_json_string(values.get("title").ok_or(VaultError::InvalidPage)?)?,
        status,
        superseded_by,
        sources,
        body: body.trim_end_matches(['\r', '\n']).to_owned(),
    }
    .validate()
    .map_err(|_| VaultError::InvalidPage)?;
    validate_slug_path(&page.relative_path)?;
    Ok(page)
}

pub fn read_wiki_pages(
    vault: &crate::ProjectVault,
) -> Result<BTreeMap<String, KnowledgePage>, VaultError> {
    let mut files = Vec::new();
    collect_files(&vault.root().join("wiki"), &mut files)?;
    files.sort();
    let mut pages = BTreeMap::new();
    for file in files {
        if file == vault.root().join("wiki/index.md") {
            continue;
        }
        let relative = file
            .strip_prefix(vault.root())
            .map_err(|_| VaultError::InvalidPath)?
            .to_string_lossy()
            .replace('\\', "/");
        let page = parse_wiki_page(
            &relative,
            &std::fs::read(&file).map_err(|_| VaultError::Io)?,
        )?;
        pages.insert(relative, page);
    }
    Ok(pages)
}

#[must_use]
pub fn normalize_wiki_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.trim().chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            slug.push(character);
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "page".to_owned()
    } else {
        slug
    }
}

fn validate_slug_path(relative_path: &str) -> Result<(), VaultError> {
    let path = Path::new(relative_path);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or(VaultError::InvalidPage)?;
    if stem == "index" || normalize_wiki_slug(stem) != stem {
        return Err(VaultError::InvalidPage);
    }
    Ok(())
}

fn collect_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<(), VaultError> {
    let mut entries = std::fs::read_dir(directory)
        .map_err(|_| VaultError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| VaultError::Io)?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let kind = entry.file_type().map_err(|_| VaultError::Io)?;
        if kind.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if kind.is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("md")
        {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn json_string(value: &str) -> Result<String, VaultError> {
    serde_json::to_string(value).map_err(|_| VaultError::InvalidPage)
}

fn parse_json_string(value: &str) -> Result<String, VaultError> {
    serde_json::from_str(value).map_err(|_| VaultError::InvalidPage)
}
