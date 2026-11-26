//! Provider-owned Gate 5.1 state envelope contract. No DB/schema integration.
//! State is advisory-only, versioned, tenant-scoped, and incompatible state abstains.

use serde::{Deserialize, Serialize};

pub const STATE_FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaselineLifecycle {
    Cold,
    Warming,
    Ready,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderStateEnvelope {
    pub provider_id: String,
    pub provider_version: String,
    pub tenant_scope_token: String,
    pub feature_schema_version: u16,
    pub state_format_version: u16,
    pub lifecycle: BaselineLifecycle,
    /// Provider-owned opaque payload. The host must not inspect or share it.
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateCompatibility {
    Compatible,
    ColdStart,
    IncompatibleProvider,
    IncompatibleModelVersion,
    IncompatibleTenant,
    IncompatibleFeatureSchema,
    IncompatibleFormat,
}

pub fn validate_state(
    envelope: &ProviderStateEnvelope,
    expected_provider: &str,
    expected_model: &str,
    expected_tenant_scope: &str,
    expected_feature_schema: u16,
) -> StateCompatibility {
    if envelope.provider_id != expected_provider { return StateCompatibility::IncompatibleProvider; }
    if envelope.provider_version != expected_model { return StateCompatibility::IncompatibleModelVersion; }
    if envelope.tenant_scope_token != expected_tenant_scope { return StateCompatibility::IncompatibleTenant; }
    if envelope.feature_schema_version != expected_feature_schema { return StateCompatibility::IncompatibleFeatureSchema; }
    if envelope.state_format_version != STATE_FORMAT_VERSION { return StateCompatibility::IncompatibleFormat; }
    if envelope.lifecycle == BaselineLifecycle::Cold { return StateCompatibility::ColdStart; }
    StateCompatibility::Compatible
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ProviderStateEnvelope {
        ProviderStateEnvelope {
            provider_id: "provider-a".into(), provider_version: "model-1".into(),
            tenant_scope_token: "opaque-tenant-token".into(), feature_schema_version: 1,
            state_format_version: STATE_FORMAT_VERSION, lifecycle: BaselineLifecycle::Warming,
            payload: serde_json::json!({"sample_count": 7}),
        }
    }

    #[test]
    fn accepts_matching_warm_state() {
        assert_eq!(validate_state(&sample(), "provider-a", "model-1", "opaque-tenant-token", 1), StateCompatibility::Compatible);
    }
    #[test]
    fn cold_state_is_explicitly_not_ready() {
        let mut s = sample(); s.lifecycle = BaselineLifecycle::Cold;
        assert_eq!(validate_state(&s, "provider-a", "model-1", "opaque-tenant-token", 1), StateCompatibility::ColdStart);
    }
    #[test]
    fn rejects_provider_model_tenant_schema_and_format_mismatches() {
        let s = sample();
        assert_eq!(validate_state(&s, "provider-b", "model-1", "opaque-tenant-token", 1), StateCompatibility::IncompatibleProvider);
        assert_eq!(validate_state(&s, "provider-a", "model-2", "opaque-tenant-token", 1), StateCompatibility::IncompatibleModelVersion);
        assert_eq!(validate_state(&s, "provider-a", "model-1", "tenant-other", 1), StateCompatibility::IncompatibleTenant);
        assert_eq!(validate_state(&s, "provider-a", "model-1", "opaque-tenant-token", 2), StateCompatibility::IncompatibleFeatureSchema);
        let mut old = s; old.state_format_version = 0;
        assert_eq!(validate_state(&old, "provider-a", "model-1", "opaque-tenant-token", 1), StateCompatibility::IncompatibleFormat);
    }
}
