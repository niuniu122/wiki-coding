use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use minimax_provider::ConfigLayer;

#[derive(Clone, Debug, Parser)]
#[command(
    name = "minimax-codex-rust",
    version,
    about = "Local Rust agent shell for MiniMax Codex"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: CliCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum CliCommand {
    Run(RunArgs),
    Chat(ChatArgs),
    Doctor(DoctorArgs),
    Migrate(MigrateArgs),
    Vault(VaultArgs),
    Index(IndexArgs),
    #[command(name = "__release-probe", hide = true)]
    ReleaseProbe {
        #[arg(long, default_value_t = 2_000, value_parser = clap::value_parser!(u64).range(100..=10_000))]
        hold_ms: u64,
    },
}

#[derive(Clone, Debug, Args)]
pub struct MigrateArgs {
    #[arg(long)]
    pub json: bool,
    #[command(subcommand)]
    pub action: MigrateAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum MigrateAction {
    Inventory {
        #[arg(long, default_value = ".mini-codex")]
        source: PathBuf,
        #[arg(long, default_value = ".")]
        target: PathBuf,
    },
    DryRun {
        #[arg(long, default_value = ".mini-codex")]
        source: PathBuf,
        #[arg(long, default_value = ".")]
        target: PathBuf,
    },
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        confirmation: String,
    },
    Verify {
        #[arg(long)]
        receipt: PathBuf,
    },
    Rollback {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        confirmation: String,
    },
}

#[derive(Clone, Debug, Args)]
pub struct IndexArgs {
    #[arg(long)]
    pub jsonl: bool,
    #[command(subcommand)]
    pub action: IndexAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum IndexAction {
    Capabilities {
        #[command(subcommand)]
        action: CapabilityIndexAction,
    },
    Projects {
        #[command(subcommand)]
        action: ProjectIndexAction,
    },
    Workspace {
        #[command(subcommand)]
        action: WorkspaceIndexAction,
    },
    Wiki {
        #[command(subcommand)]
        action: WikiIndexAction,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum CapabilityIndexAction {
    Status,
    Search {
        query: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum ProjectIndexAction {
    Status {
        #[arg(long)]
        catalog: Option<PathBuf>,
        #[arg(long)]
        embedding_resource: Option<PathBuf>,
    },
    Search {
        query: String,
        #[arg(long)]
        catalog: Option<PathBuf>,
        #[arg(long)]
        embedding_resource: Option<PathBuf>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum CapabilityKindArg {
    All,
    Project,
    Skill,
    Mcp,
}

impl CapabilityKindArg {
    #[must_use]
    pub const fn selected_kind(self) -> Option<minimax_protocol::CapabilityKind> {
        match self {
            Self::All => None,
            Self::Project => Some(minimax_protocol::CapabilityKind::Project),
            Self::Skill => Some(minimax_protocol::CapabilityKind::Skill),
            Self::Mcp => Some(minimax_protocol::CapabilityKind::Mcp),
        }
    }
}

#[derive(Clone, Debug, Subcommand)]
pub enum WorkspaceIndexAction {
    Status {
        #[arg(long)]
        catalog_root: Option<PathBuf>,
        #[arg(long)]
        embedding_resource: Option<PathBuf>,
    },
    Search {
        query: String,
        #[arg(long, value_enum, default_value = "all")]
        kind: CapabilityKindArg,
        #[arg(long)]
        catalog_root: Option<PathBuf>,
        #[arg(long)]
        inventory: Option<PathBuf>,
        #[arg(long)]
        embedding_resource: Option<PathBuf>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum WikiIndexAction {
    Status {
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        vault: PathBuf,
        #[arg(long)]
        project_id: String,
    },
    Search {
        query: String,
        #[arg(long, default_value = ".")]
        project: PathBuf,
        #[arg(long)]
        vault: PathBuf,
        #[arg(long)]
        project_id: String,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
}

#[derive(Clone, Debug, Args)]
pub struct VaultArgs {
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
    #[arg(long)]
    pub vault: PathBuf,
    #[arg(long)]
    pub project_id: String,
    #[arg(long)]
    pub jsonl: bool,
    #[command(subcommand)]
    pub action: VaultAction,
}

#[derive(Clone, Debug, Subcommand)]
pub enum VaultAction {
    Bootstrap,
    Status,
    Lint,
    Repair,
    Rebuild,
    Import {
        relative_path: String,
    },
    Gc {
        #[command(subcommand)]
        action: VaultGcAction,
    },
    Forget {
        #[command(subcommand)]
        action: VaultForgetAction,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum VaultGcAction {
    Report,
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        confirmation: String,
    },
    Undo {
        gc_id: String,
    },
    Purge {
        gc_id: String,
        #[arg(long)]
        confirmation: String,
    },
}

#[derive(Clone, Debug, Subcommand)]
pub enum VaultForgetAction {
    Plan {
        evidence_id: String,
        expected_hash: String,
    },
    Apply {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        patch: PathBuf,
        #[arg(long)]
        confirmation: String,
    },
}

#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[arg(long)]
    pub jsonl: bool,
    #[arg(long)]
    pub agent: bool,
    #[arg(long, value_enum, default_value_t = PermissionArg::Confirm)]
    pub permission: PermissionArg,
    #[arg(long, short = 'p')]
    pub prompt: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum PermissionArg {
    #[default]
    Confirm,
    FullAccess,
}

impl From<PermissionArg> for minimax_core::PermissionMode {
    fn from(value: PermissionArg) -> Self {
        match value {
            PermissionArg::Confirm => Self::Confirm,
            PermissionArg::FullAccess => Self::FullAccess,
        }
    }
}

#[derive(Clone, Debug, Args)]
pub struct ChatArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[arg(long, short = 'p')]
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct DoctorArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Clone, Debug, Args)]
pub struct CommonArgs {
    #[arg(long, default_value = ".")]
    pub project: PathBuf,
    #[arg(long)]
    pub user_config: Option<PathBuf>,
    #[arg(long)]
    pub project_config: Option<PathBuf>,
    #[arg(long)]
    pub vault: Option<PathBuf>,
    #[arg(long)]
    pub project_id: Option<String>,
    #[arg(long)]
    pub embedding_resource: Option<PathBuf>,
    #[arg(long)]
    pub capability_root: Option<PathBuf>,
    #[arg(long)]
    pub capability_inventory: Option<PathBuf>,
    #[arg(long)]
    pub provider_id: Option<String>,
    #[arg(long)]
    pub endpoint: Option<String>,
    #[arg(long, value_enum)]
    pub protocol: Option<ProtocolArg>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub environment_key: Option<String>,
    #[arg(long)]
    pub timeout_ms: Option<u64>,
    #[arg(long)]
    pub max_output_tokens: Option<u32>,
    #[arg(long)]
    pub allow_insecure_loopback: bool,
}

impl CommonArgs {
    #[must_use]
    pub fn config_layer(&self) -> ConfigLayer {
        ConfigLayer {
            provider_id: self.provider_id.clone(),
            endpoint: self.endpoint.clone(),
            protocol: self.protocol.map(Into::into),
            model: self.model.clone(),
            environment_key: self.environment_key.clone(),
            timeout_ms: self.timeout_ms,
            max_output_tokens: self.max_output_tokens,
            allow_insecure_loopback: self.allow_insecure_loopback.then_some(true),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ProtocolArg {
    Responses,
    ChatCompletions,
}

impl From<ProtocolArg> for minimax_protocol::ProviderProtocolKind {
    fn from(value: ProtocolArg) -> Self {
        match value {
            ProtocolArg::Responses => Self::Responses,
            ProtocolArg::ChatCompletions => Self::ChatCompletions,
        }
    }
}
