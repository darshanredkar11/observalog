use regex::Regex;
use std::collections::HashSet;
use lazy_static::lazy_static;

lazy_static! {
    static ref DOMAINS: HashSet<&'static str> = {
        vec!["auth", "doc", "provider", "infra"].into_iter().collect()
    };

    static ref ACTIONS: HashSet<&'static str> = {
        vec![
            "received", "validated", "rejected", "published", "failed",
            "exhausted", "expired", "attempted", "succeeded", "created",
            "updated", "deleted", "queried", "connected", "disconnected",
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
    }

    #[test]
    fn test_invalid_events() {
        assert!(validate_event("auth.jwt").is_err()); // too few segments
        assert!(validate_event("unknown.obj.action").is_err()); // invalid domain
        assert!(validate_event("auth.obj.unknown_verb").is_err()); // invalid action
    }
}
