use minimax_protocol::{ForgetPlan, GcPlan, VaultLintReport, VaultManifest};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VaultStatusOutput {
    pub manifest: VaultManifest,
    pub lint: VaultLintReport,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GcPlanOutput {
    pub plan: GcPlan,
    pub confirmation: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForgetPlanOutput {
    pub plan: ForgetPlan,
    pub confirmation: String,
}
