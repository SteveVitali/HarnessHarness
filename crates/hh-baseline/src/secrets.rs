//! Deny-by-default environment allow-list, known-value masking, and the leak scan
//! (R-2.8.3 Stage-0 allow-list half; §5g.3). **Throwaway Stage-0 subset.**
//!
//! AC-R-2.8.3-1 (Stage-0 allow-list half): "an environment dump and a read of the
//! process-environment file inside the handle show placeholders only; `leak_scan(run) = ∅`; a
//! `SecretRef` renders as name + description". At Stage 0 there is no credential broker
//! (S1.13) and no reference monitor (S1.11): the baseline instead holds secret *values*
//! kernel-side (never handed to the model-blind gateway or the tool executor), exposes only
//! `SecretRef` placeholders, and scans every produced byte for a leak.

use std::collections::BTreeSet;

/// A reference to a secret: its stable name and a human description. The value never appears
/// in a `SecretRef` — it is held kernel-side (see [`crate::gateway::Gateway`]). Rendering a
/// `SecretRef` yields `name + description`, never the carrier (AC-R-2.8.3-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRef {
    pub name: String,
    pub description: String,
}

impl SecretRef {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
        }
    }

    /// The placeholder that stands in for the value everywhere it would otherwise appear.
    pub fn placeholder(&self) -> String {
        format!("${{SECRET:{}}}", self.name)
    }

    /// How a `SecretRef` renders to any surface: name + description, never the value.
    pub fn render(&self) -> String {
        format!("{} ({})", self.name, self.description)
    }
}

/// The deny-by-default environment allow-list. An environment variable is exposed to the
/// sandbox only if its name is explicitly allowed; everything else — including every secret
/// carrier — is withheld and shown as a placeholder.
#[derive(Debug, Clone, Default)]
pub struct AllowList {
    allowed: BTreeSet<String>,
}

impl AllowList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allow a specific variable name (deny-by-default: absence means denied).
    pub fn allow(mut self, name: impl Into<String>) -> Self {
        self.allowed.insert(name.into());
        self
    }

    pub fn is_allowed(&self, name: &str) -> bool {
        self.allowed.contains(name)
    }
}

/// A held secret: the `SecretRef` plus its value, which stays kernel-side. This type is never
/// serialized and its value never crosses into a produced byte.
#[derive(Debug, Clone)]
pub struct HeldSecret {
    pub reference: SecretRef,
    value: String,
}

impl HeldSecret {
    pub fn new(reference: SecretRef, value: impl Into<String>) -> Self {
        Self {
            reference,
            value: value.into(),
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Replace every known secret value in `text` with its placeholder (known-value masking). Used
/// on every byte that leaves the kernel boundary — env dumps, tool output, trace payloads.
pub fn mask_known_values(text: &str, secrets: &[HeldSecret]) -> String {
    let mut masked = text.to_string();
    for s in secrets {
        if !s.value.is_empty() {
            masked = masked.replace(&s.value, &s.reference.placeholder());
        }
    }
    masked
}

/// A detected leak: a secret whose raw value appeared in produced output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leak {
    pub secret_name: String,
    pub where_found: String,
}

/// Scan a run's produced bytes for any raw secret value. `∅` (empty) is the pass condition of
/// AC-R-2.8.3-1. `outputs` is `(where, bytes)` for every produced surface.
pub fn leak_scan(outputs: &[(String, String)], secrets: &[HeldSecret]) -> Vec<Leak> {
    let mut leaks = Vec::new();
    for (where_found, bytes) in outputs {
        for s in secrets {
            if !s.value.is_empty() && bytes.contains(&s.value) {
                leaks.push(Leak {
                    secret_name: s.reference.name.clone(),
                    where_found: where_found.clone(),
                });
            }
        }
    }
    leaks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held() -> Vec<HeldSecret> {
        vec![HeldSecret::new(
            SecretRef::new("API_TOKEN", "gateway credential"),
            "sk-secret-123",
        )]
    }

    #[test]
    fn secret_ref_renders_name_and_description_never_value() {
        let r = SecretRef::new("API_TOKEN", "gateway credential");
        assert_eq!(r.render(), "API_TOKEN (gateway credential)");
        assert!(!r.render().contains("sk-secret"));
        assert_eq!(r.placeholder(), "${SECRET:API_TOKEN}");
    }

    #[test]
    fn allow_list_is_deny_by_default() {
        let a = AllowList::new().allow("PATH");
        assert!(a.is_allowed("PATH"));
        assert!(!a.is_allowed("API_TOKEN"));
        assert!(!a.is_allowed("HOME"));
    }

    #[test]
    fn masking_replaces_known_values_with_placeholders() {
        let masked = mask_known_values("token=sk-secret-123 done", &held());
        assert_eq!(masked, "token=${SECRET:API_TOKEN} done");
        assert!(!masked.contains("sk-secret-123"));
    }

    #[test]
    fn leak_scan_is_empty_when_masked() {
        let masked = mask_known_values("token=sk-secret-123", &held());
        let leaks = leak_scan(&[("env_dump".into(), masked)], &held());
        assert!(leaks.is_empty());
    }

    #[test]
    fn leak_scan_catches_an_unmasked_value() {
        // If masking were skipped, the scan must fire — this guards the AC.
        let leaks = leak_scan(
            &[("tool_output".into(), "token=sk-secret-123".into())],
            &held(),
        );
        assert_eq!(leaks.len(), 1);
        assert_eq!(leaks[0].secret_name, "API_TOKEN");
    }
}
