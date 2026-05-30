use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref DOMAINS: HashSet<&'static str> = {
        vec!["auth", "doc", "provider", "infra"].into_iter().collect()
    };

    static ref ACTIONS: HashSet<&'static str> = {
        vec![
            // Original 15 verbs
            "received", "validated", "rejected", "published", "failed",
            "exhausted", "expired", "attempted", "succeeded", "created",
            "updated", "deleted", "queried", "connected", "disconnected",
            // Auth / identity domain verbs (v1.2 — added from real integration feedback)
            "registered",   // auth.user.registered (sign-up, distinct from created)
            "revoked",      // auth.session.revoked (preserves audit trail, ≠ deleted)
            "locked",       // auth.login.locked (lockout after failed attempts)
            "refreshed",    // auth.token.refreshed (more specific than updated)
            "challenged",   // auth.mfa.challenged (MFA prompt issued)
            "verified",     // auth.email.verified / auth.mfa.verified
            "enabled",      // auth.mfa.enabled (feature toggle, ≠ created)
            "disabled",     // auth.mfa.disabled
            "rotated",      // auth.key.rotated (secrets, API keys)
            "granted",      // auth.permission.granted (explicit allow)
            "denied",       // auth.permission.denied (explicit deny, ≠ rejected)
        ].into_iter().collect()
    };

    static ref EVENT_PATTERN: Regex = Regex::new(r"^[a-z]+\.[a-z_]+\.[a-z_]+$").unwrap();
}

/// Validate event string against grammar: domain.object.action
pub fn validate_event(event: &str) -> Result<(), String> {
    // Check format: three segments separated by dots, all lowercase/underscore
    if !EVENT_PATTERN.is_match(event) {
        return Err(format!(
            "Event '{}' must match grammar domain.object.action (lowercase, dots only)",
            event
        ));
    }

    let parts: Vec<&str> = event.split('.').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Event '{}' must have exactly 3 segments, got {}",
            event,
            parts.len()
        ));
    }

    let domain = parts[0];
    let action = parts[2];

    // Validate domain
    if !DOMAINS.contains(domain) {
        return Err(format!(
            "Unknown domain '{}' in event '{}'. Allowed: {:?}",
            domain, event, DOMAINS.iter().collect::<Vec<_>>()
        ));
    }

    // Validate action (object is free-form)
    if !ACTIONS.contains(action) {
        return Err(format!(
            "Unknown action '{}' in event '{}'. Allowed: {:?}",
            action, event, ACTIONS.iter().collect::<Vec<_>>()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_events() {
        assert!(validate_event("auth.jwt.validated").is_ok());
        assert!(validate_event("doc.document.created").is_ok());
        assert!(validate_event("provider.send.failed").is_ok());
        assert!(validate_event("infra.db.queried").is_ok());
        // v1.2 auth/identity verbs
        assert!(validate_event("auth.user.registered").is_ok());
        assert!(validate_event("auth.session.revoked").is_ok());
        assert!(validate_event("auth.login.locked").is_ok());
        assert!(validate_event("auth.token.refreshed").is_ok());
        assert!(validate_event("auth.mfa.challenged").is_ok());
        assert!(validate_event("auth.email.verified").is_ok());
        assert!(validate_event("auth.mfa.enabled").is_ok());
        assert!(validate_event("auth.permission.granted").is_ok());
        assert!(validate_event("auth.permission.denied").is_ok());
    }

    #[test]
    fn test_invalid_events() {
        assert!(validate_event("auth.jwt").is_err()); // too few segments
        assert!(validate_event("unknown.obj.action").is_err()); // invalid domain
        assert!(validate_event("auth.obj.unknown_verb").is_err()); // invalid action
    }
}
