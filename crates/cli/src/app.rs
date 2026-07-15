use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use minimax_provider::ConfigLayer;

#[derive(Clone, Debug, Parser)]
#[command(
    name = "minimax-codex-rust",
    version,
    about = "Rust development shell for MiniMax Codex"
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
    Migrate,
    Vault,
    Index,
}

#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[arg(long)]
    pub jsonl: bool,
    #[arg(long, short = 'p')]
    pub prompt: String,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceRoute {
    Migrate,
    Vault,
    Index,
}

impl MaintenanceRoute {
    #[must_use]
    pub const fn owning_phase(self) -> u8 {
        match self {
            Self::Migrate => 6,
            Self::Vault => 4,
            Self::Index => 5,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Migrate => "migrate",
            Self::Vault => "vault",
            Self::Index => "index",
        }
    }

    #[must_use]
    pub fn not_available(self) -> String {
        format!(
            "{} is not available in the Rust development shell until Phase {}",
            self.name(),
            self.owning_phase()
        )
    }
}
