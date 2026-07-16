pub mod app;
pub mod config;
pub mod doctor;
pub mod driver;
pub mod headless;
pub mod wiki;

pub use doctor::{CheckStatus, DoctorCheck, DoctorReport, inspect};
pub use driver::{
    DriverError, DriverIds, HeadlessApprovalPort, HttpProviderPort, InteractiveApprovalPort,
    ProviderPort, RunReport, RuntimeDriver,
};
pub use headless::{ExitClass, JsonlWriter, exit_for_error, exit_for_report};
pub use wiki::{
    MainModelWikiDriver, ProjectVaultBinding, VaultKnowledgePort, WikiDriverError, WikiFaultPoint,
    WikiRunReport,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "composition root for Rust development commands and adapters";
pub use app::{
    ChatArgs, Cli, CliCommand, CommonArgs, DoctorArgs, MaintenanceRoute, PermissionArg,
    ProtocolArg, RunArgs,
};
