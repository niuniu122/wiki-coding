use std::path::{Component, Path, PathBuf};

use crate::error::{ToolDenial, ToolDenialCode, io_denial};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRoot {
    canonical: PathBuf,
}

impl WorkspaceRoot {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, ToolDenial> {
        let canonical = std::fs::canonicalize(root.as_ref()).map_err(|error| io_denial(&error))?;
        if !canonical.is_dir() {
            return Err(ToolDenial::rejected(ToolDenialCode::WrongFileType));
        }
        Ok(Self { canonical })
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.canonical
    }

    pub fn resolve_existing(&self, relative: &str) -> Result<ResolvedToolPath, ToolDenial> {
        let lexical = validate_relative_path(relative)?;
        let joined = self.canonical.join(&lexical);
        let canonical = std::fs::canonicalize(&joined).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ToolDenial::rejected(ToolDenialCode::PathNotFound)
            } else {
                io_denial(&error)
            }
        })?;
        self.ensure_contained(&canonical)?;
        Ok(ResolvedToolPath {
            relative: lexical,
            absolute: canonical,
        })
    }

    pub fn resolve_write(&self, relative: &str) -> Result<ResolvedToolPath, ToolDenial> {
        let lexical = validate_relative_path(relative)?;
        let joined = self.canonical.join(&lexical);
        let mut ancestor = joined.as_path();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| ToolDenial::rejected(ToolDenialCode::OutsideWorkspace))?;
        }
        let canonical_ancestor =
            std::fs::canonicalize(ancestor).map_err(|error| io_denial(&error))?;
        self.ensure_contained(&canonical_ancestor)?;
        if joined.exists() {
            let canonical_target =
                std::fs::canonicalize(&joined).map_err(|error| io_denial(&error))?;
            self.ensure_contained(&canonical_target)?;
        }
        Ok(ResolvedToolPath {
            relative: lexical,
            absolute: joined,
        })
    }

    pub(crate) fn ensure_contained(&self, path: &Path) -> Result<(), ToolDenial> {
        if is_within(path, &self.canonical) {
            Ok(())
        } else {
            Err(ToolDenial::rejected(ToolDenialCode::OutsideWorkspace))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedToolPath {
    relative: PathBuf,
    absolute: PathBuf,
}

impl ResolvedToolPath {
    #[must_use]
    pub fn relative(&self) -> &Path {
        &self.relative
    }

    #[must_use]
    pub fn absolute(&self) -> &Path {
        &self.absolute
    }
}

pub(crate) fn validate_relative_path(path: &str) -> Result<PathBuf, ToolDenial> {
    let bytes = path.as_bytes();
    let has_windows_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if path.is_empty()
        || path.contains('\0')
        || path.starts_with("\\\\")
        || has_windows_prefix
        || cfg!(not(windows)) && path.contains('\\')
    {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidPath));
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidPath));
    }
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(ToolDenial::rejected(ToolDenialCode::InvalidPath));
            }
            Component::Normal(value) => validate_component(value.to_string_lossy().as_ref())?,
            Component::CurDir => {}
        }
    }
    Ok(path.to_path_buf())
}

fn validate_component(component: &str) -> Result<(), ToolDenial> {
    if component.is_empty() {
        return Err(ToolDenial::rejected(ToolDenialCode::InvalidPath));
    }
    #[cfg(windows)]
    {
        let trimmed = component.trim_end_matches(['.', ' ']);
        let base = trimmed
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if trimmed != component
            || component.contains(':')
            || matches!(
                base.as_str(),
                "con"
                    | "prn"
                    | "aux"
                    | "nul"
                    | "com1"
                    | "com2"
                    | "com3"
                    | "com4"
                    | "com5"
                    | "com6"
                    | "com7"
                    | "com8"
                    | "com9"
                    | "lpt1"
                    | "lpt2"
                    | "lpt3"
                    | "lpt4"
                    | "lpt5"
                    | "lpt6"
                    | "lpt7"
                    | "lpt8"
                    | "lpt9"
            )
        {
            return Err(ToolDenial::rejected(ToolDenialCode::InvalidPath));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_within(path: &Path, root: &Path) -> bool {
    let path = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let root = root.to_string_lossy().replace('/', "\\").to_lowercase();
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

#[cfg(not(windows))]
fn is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}
