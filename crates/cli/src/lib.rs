pub mod app;
pub mod config;
pub mod doctor;
pub mod driver;
pub mod headless;
pub mod index;
pub mod maintenance;
pub mod wiki;

pub use doctor::{CheckStatus, DoctorCheck, DoctorReport, inspect};
pub use driver::{
    DriverError, DriverIds, HeadlessApprovalPort, HttpProviderPort, InteractiveApprovalPort,
    ProviderPort, RunReport, RuntimeDriver,
};
pub use headless::{ExitClass, JsonlWriter, exit_for_error, exit_for_report};
pub use index::{
    IndexError, capability_search, capability_status, project_search, project_status, wiki_search,
    wiki_status,
};
pub use maintenance::{ForgetPlanOutput, GcPlanOutput, VaultStatusOutput};
pub use wiki::{
    MainModelWikiDriver, ProjectVaultBinding, VaultKnowledgePort, WikiDriverError, WikiFaultPoint,
    WikiRunReport,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "composition root for Rust development commands and adapters";
pub use app::{
    CapabilityIndexAction, ChatArgs, Cli, CliCommand, CommonArgs, DoctorArgs, IndexAction,
    IndexArgs, MaintenanceRoute, PermissionArg, ProjectIndexAction, ProtocolArg, RunArgs,
    VaultAction, VaultArgs, VaultForgetAction, VaultGcAction, WikiIndexAction,
};
