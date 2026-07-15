use std::collections::BTreeMap;
use std::path::Path;

use minimax_protocol::RuntimeErrorCode;
use minimax_provider::{
    ConfigDocument, ConfigLayer, ResolvedConfig, parse_config_document, resolve_config,
};

const MAX_CONFIG_BYTES: u64 = 64 * 1024;

pub fn load_optional_config(path: &Path) -> Result<Option<ConfigDocument>, RuntimeErrorCode> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(RuntimeErrorCode::Configuration),
    };
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return Err(RuntimeErrorCode::Configuration);
    }
    let raw = std::fs::read_to_string(path).map_err(|_| RuntimeErrorCode::Configuration)?;
    parse_config_document(&raw).map(Some)
}

pub fn resolve_from_files(
    user_path: &Path,
    project_path: &Path,
    environment: &BTreeMap<String, String>,
    cli: ConfigLayer,
) -> Result<ResolvedConfig, RuntimeErrorCode> {
    resolve_config(
        load_optional_config(user_path)?,
        load_optional_config(project_path)?,
        environment,
        cli,
    )
}
