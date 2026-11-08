//! Human review is a separate application action, never a compiler/model verdict.
use crate::RunProfile;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn text_sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewDraft {
    pub source_id: String,
    pub source_text: String,
    pub source_label: String,
    pub formal_text: String,
}
impl ReviewDraft {
    pub fn validate_bounds(&self) -> Result<(), String> {
        for (name, text, limit) in [
            ("source identity", &self.source_id, 256),
            ("source", &self.source_text, 16 * 1024),
            ("source label", &self.source_label, 1024),
            ("formal draft", &self.formal_text, 64 * 1024),
        ] {
            if text.len() > limit {
                return Err(format!("The {name} must not exceed {limit} bytes."));
            }
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_bounds()?;
        if [
            &self.source_id,
            &self.source_label,
            &self.source_text,
            &self.formal_text,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
        {
            return Err(
                "Fill in the source identity, label, original text and formal draft before review."
                    .into(),
            );
        }
        Ok(())
    }
}

/// A worker-held preparation. It proves neither source accuracy nor intent fidelity.
/// The worker issues a fresh ticket and checks its exact stored value at admission.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewPreparation {
    pub schema: u32,
    pub ticket: String,
    pub draft: ReviewDraft,
    pub source_sha256: String,
    pub formal_sha256: String,
    pub epoch: String,
    pub revision: String,
    pub profile: RunProfile,
    pub engine_revision: String,
    pub canonical_kr: String,
    pub backtranslation: String,
    pub logic: Value,
    pub detached_admission_record: Value,
}

/// Learner-attested application provenance, carried separately from engine KR labels.
/// Imported provenance is history supplied by the importer, not authentication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewProvenance {
    pub source_id: String,
    pub source_text: String,
    pub source_sha256: String,
    pub formal_sha256: String,
    pub review_ticket: String,
    pub engine_revision: String,
    pub reviewed_epoch: String,
    pub reviewed_revision: String,
}
impl ReviewProvenance {
    pub fn validate(&self, formal_text: &str) -> Result<(), String> {
        if self.source_id.trim().is_empty()
            || self.source_id.len() > 256
            || self.source_text.trim().is_empty()
            || self.source_text.len() > 16 * 1024
            || self.review_ticket.is_empty()
            || self.review_ticket.len() > 512
            || self.engine_revision.len() > 256
            || self.reviewed_epoch.is_empty()
            || self.reviewed_epoch.len() > 128
            || self.reviewed_revision.len() > 32
            || self.source_sha256 != text_sha256(&self.source_text)
            || self.formal_sha256 != text_sha256(formal_text)
        {
            return Err(
                "Reviewed-source provenance does not match its retained source/draft bytes.".into(),
            );
        }
        Ok(())
    }
}
