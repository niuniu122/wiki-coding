pub mod app;
pub mod binding;
pub mod config;
pub mod doctor;
pub mod driver;
pub mod headless;
pub mod index;
pub mod maintenance;
pub mod migration;
pub mod wiki;

pub use binding::{ResolvedProjectVault, resolve_project_vault};
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
pub use migration::{
    MigrationError, MigrationInventory, MigrationPlan, MigrationReceipt, MigrationVerifyReport,
    apply_migration, build_migration_plan, inventory_migration, rollback_migration,
    verify_migration,
};
pub use wiki::{
    MainModelWikiDriver, PreparedWikiInputs, ProjectVaultBinding, VaultKnowledgePort,
    WikiDriverError, WikiFaultPoint, WikiRunReport, finalize_active_session_wiki,
    prepare_wiki_inputs,
};

/// Human-readable boundary used by architecture checks and documentation.
pub const CRATE_ROLE: &str = "composition root for Rust development commands and adapters";
pub use app::{
    CapabilityIndexAction, ChatArgs, Cli, CliCommand, CommonArgs, DoctorArgs, IndexAction,
    IndexArgs, MigrateAction, MigrateArgs, PermissionArg, ProjectIndexAction, ProtocolArg, RunArgs,
    VaultAction, VaultArgs, VaultForgetAction, VaultGcAction, WikiIndexAction,
};
