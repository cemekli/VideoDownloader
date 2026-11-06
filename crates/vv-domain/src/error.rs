//! Shared error shape for every backend → UI boundary (SPEC §18.3, §19).
//!
//! The UI only ever receives this object: no stack traces, shell commands,
//! tokens, cookies or full authenticated URLs. `safe_details` holds a small
//! allowlisted map and nothing else.

use serde::Serialize;
use std::collections::BTreeMap;

/// Stable error codes from SPEC §19. Serialized as SCREAMING_SNAKE_CASE, the
/// same strings the UI switches on. New codes are appended, never renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidUrl,
    UrlBlocked,
    UrlHasCredentials,
    InternalError,
}

impl ErrorCode {
    /// Whether retrying the same request can succeed (SPEC §19 "İş davranışı").
    pub fn retryable(self) -> bool {
        // None of the current codes is retryable; NETWORK_UNAVAILABLE,
        // RATE_LIMITED and TIMEOUT will be when they are added.
        match self {
            ErrorCode::InvalidUrl
            | ErrorCode::UrlBlocked
            | ErrorCode::UrlHasCredentials
            | ErrorCode::InternalError => false,
        }
    }

    /// Localization key; the UI owns the actual text.
    pub fn message_key(self) -> &'static str {
        match self {
            ErrorCode::InvalidUrl => "error.invalid_url",
            ErrorCode::UrlBlocked => "error.url_blocked",
            ErrorCode::UrlHasCredentials => "error.url_has_credentials",
            ErrorCode::InternalError => "error.internal",
        }
    }
}

/// Suggested next step for the user. Kept as a closed set on purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    FixInput,
    ShowDiagnostics,
}

/// SPEC §18.3 `DomainError`. On the wire fields are camelCase like every other
/// DTO (`messageKey`, `safeDetails`, ...); the shape is pinned by
/// `tests/fixtures/ipc-contract.json` and changes only with a contract bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainError {
    pub code: ErrorCode,
    pub message_key: &'static str,
    pub retryable: bool,
    pub suggested_action: Option<SuggestedAction>,
    pub diagnostic_id: String,
    pub safe_details: BTreeMap<&'static str, String>,
}

/// Upper bound for `safe_details` so a bug cannot stream large data to the UI.
pub const MAX_SAFE_DETAILS: usize = 8;
/// Upper bound for one `safe_details` value, in characters.
pub const MAX_SAFE_DETAIL_LEN: usize = 200;

impl DomainError {
    pub fn new(code: ErrorCode, diagnostic_id: impl Into<String>) -> Self {
        let suggested_action = match code {
            ErrorCode::InvalidUrl | ErrorCode::UrlBlocked | ErrorCode::UrlHasCredentials => {
                Some(SuggestedAction::FixInput)
            }
            ErrorCode::InternalError => Some(SuggestedAction::ShowDiagnostics),
        };
        DomainError {
            code,
            message_key: code.message_key(),
            retryable: code.retryable(),
            suggested_action,
            diagnostic_id: diagnostic_id.into(),
            safe_details: BTreeMap::new(),
        }
    }

    /// Adds an allowlisted detail. Values are truncated and extra keys are
    /// dropped instead of failing, so error reporting itself never errors.
    pub fn with_detail(mut self, key: &'static str, value: impl Into<String>) -> Self {
        if self.safe_details.len() < MAX_SAFE_DETAILS || self.safe_details.contains_key(key) {
            let v: String = value.into().chars().take(MAX_SAFE_DETAIL_LEN).collect();
            self.safe_details.insert(key, v);
        }
        self
    }
}

impl From<&crate::url_policy::UrlPolicyError> for ErrorCode {
    fn from(e: &crate::url_policy::UrlPolicyError) -> Self {
        use crate::url_policy::UrlPolicyError::*;
        match e {
            InvalidUrl => ErrorCode::InvalidUrl,
            HasCredentials => ErrorCode::UrlHasCredentials,
            Blocked => ErrorCode::UrlBlocked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::url_policy::UrlPolicyError;

    #[test]
    fn serializes_to_spec_18_3_shape() {
        let e = DomainError::new(ErrorCode::UrlBlocked, "diag-1");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "URL_BLOCKED");
        assert_eq!(v["messageKey"], "error.url_blocked");
        assert_eq!(v["retryable"], false);
        assert_eq!(v["suggestedAction"], "fix_input");
        assert_eq!(v["diagnosticId"], "diag-1");
        assert!(v["safeDetails"].as_object().unwrap().is_empty());
    }

    #[test]
    fn url_policy_codes_match_error_codes() {
        for e in [
            UrlPolicyError::InvalidUrl,
            UrlPolicyError::HasCredentials,
            UrlPolicyError::Blocked,
        ] {
            let code = serde_json::to_value(ErrorCode::from(&e)).unwrap();
            assert_eq!(
                code,
                e.code(),
                "UrlPolicyError::code() and ErrorCode must agree"
            );
        }
    }

    #[test]
    fn safe_details_are_bounded() {
        let mut e = DomainError::new(ErrorCode::InternalError, "d");
        let keys = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        for k in keys {
            e = e.with_detail(k, "x".repeat(500));
        }
        assert_eq!(e.safe_details.len(), MAX_SAFE_DETAILS);
        assert!(e
            .safe_details
            .values()
            .all(|v| v.chars().count() == MAX_SAFE_DETAIL_LEN));
    }
}
