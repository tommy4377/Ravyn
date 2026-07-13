//! Explainable advisory trust scoring for download sources and artifacts.

use serde::{Deserialize, Serialize};

use crate::{
    core::models::Job,
    error::Result,
    storage::Repository,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustFactor {
    pub code: String,
    pub label: String,
    pub points: i32,
    pub satisfied: bool,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustReport {
    pub score: u8,
    pub level: String,
    pub factors: Vec<TrustFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ed25519SignatureInput {
    pub public_key_hex: String,
    pub signature_hex: String,
    /// SHA-256 digest whose raw 32 bytes were signed.
    pub signed_sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrustPreviewRequest {
    pub source_url: String,
    /// Whether a real TLS handshake completed with a valid certificate chain.
    pub tls_certificate_valid: Option<bool>,
    pub checksum_available: bool,
    pub checksum_verified: bool,
    pub signature_valid: Option<bool>,
    /// Whether the signature key is anchored in an operator-trusted key set.
    pub signer_trusted: bool,
    pub ed25519_signature: Option<Ed25519SignatureInput>,
    pub known_mirror: bool,
    pub metadata_consistent: bool,
}

/// Validates an optional Ed25519 signature and computes the advisory report.
pub fn evaluate(input: &TrustPreviewRequest) -> Result<TrustReport> {
    let mut evaluated = input.clone();
    if let Some(signature) = input.ed25519_signature.as_ref() {
        evaluated.signature_valid = Some(verify_ed25519(signature)?);
    }
    Ok(compute(&evaluated))
}

fn verify_ed25519(input: &Ed25519SignatureInput) -> Result<bool> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let public_key = hex::decode(&input.public_key_hex)
        .map_err(|_| crate::error::RavynError::Invalid("public_key_hex must be hexadecimal".into()))?;
    let public_key: [u8; 32] = public_key.try_into().map_err(|_| {
        crate::error::RavynError::Invalid("public_key_hex must contain 32 bytes".into())
    })?;
    let signature = hex::decode(&input.signature_hex)
        .map_err(|_| crate::error::RavynError::Invalid("signature_hex must be hexadecimal".into()))?;
    let signature = Signature::try_from(signature.as_slice()).map_err(|_| {
        crate::error::RavynError::Invalid("signature_hex must contain 64 bytes".into())
    })?;
    let digest = hex::decode(&input.signed_sha256)
        .map_err(|_| crate::error::RavynError::Invalid("signed_sha256 must be hexadecimal".into()))?;
    let digest: [u8; 32] = digest.try_into().map_err(|_| {
        crate::error::RavynError::Invalid("signed_sha256 must contain 32 bytes".into())
    })?;
    let key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| crate::error::RavynError::Invalid("public_key_hex is not a valid Ed25519 key".into()))?;
    Ok(key.verify_strict(&digest, &signature).is_ok())
}

pub fn compute(input: &TrustPreviewRequest) -> TrustReport {
    let https = url::Url::parse(&input.source_url)
        .ok()
        .is_some_and(|url| url.scheme() == "https");
    let signature_points = match (input.signature_valid, input.signer_trusted) {
        (Some(true), true) => 20,
        (Some(true), false) => 5,
        (Some(false), _) => -40,
        (None, _) => 0,
    };
    let factors = vec![
        factor(
            "https",
            "Encrypted source scheme",
            if https { 10 } else { 0 },
            https,
            if https {
                "The source uses HTTPS. Certificate validity is scored separately after a real handshake."
            } else {
                "The source does not use HTTPS."
            },
        ),
        factor(
            "tls_certificate",
            "TLS certificate",
            match input.tls_certificate_valid {
                Some(true) => 5,
                Some(false) => -25,
                None => 0,
            },
            input.tls_certificate_valid == Some(true),
            match input.tls_certificate_valid {
                Some(true) => "A real TLS handshake completed with a valid certificate chain.",
                Some(false) => "TLS certificate validation failed.",
                None => "No real TLS certificate result is available.",
            },
        ),
        factor(
            "checksum_available",
            "Published checksum",
            if input.checksum_available { 15 } else { 0 },
            input.checksum_available,
            if input.checksum_available {
                "A SHA-256 identity is available for the payload."
            } else {
                "No published SHA-256 identity was supplied."
            },
        ),
        factor(
            "checksum_verified",
            "Checksum verified",
            if input.checksum_verified { 25 } else { 0 },
            input.checksum_verified,
            if input.checksum_verified {
                "The downloaded bytes match the expected SHA-256 value."
            } else {
                "The payload has not been verified against a published SHA-256 value."
            },
        ),
        factor(
            "signature",
            "Digital signature",
            signature_points,
            input.signature_valid == Some(true),
            match (input.signature_valid, input.signer_trusted) {
                (Some(true), true) => {
                    "A supplied digital signature was verified against an operator-trusted key."
                }
                (Some(true), false) => {
                    "The signature is cryptographically valid, but its key is not trusted."
                }
                (Some(false), _) => "A supplied digital signature failed verification.",
                (None, _) => "No digital signature result is available.",
            },
        ),
        factor(
            "known_mirror",
            "Known mirror set",
            if input.known_mirror { 15 } else { 0 },
            input.known_mirror,
            if input.known_mirror {
                "The source belongs to a verified or explicitly trusted mirror set."
            } else {
                "The source has no verified mirror relationship."
            },
        ),
        factor(
            "metadata_consistency",
            "Metadata consistency",
            if input.metadata_consistent { 10 } else { 0 },
            input.metadata_consistent,
            if input.metadata_consistent {
                "The observed file metadata is consistent with the job metadata."
            } else {
                "The file metadata has not been confirmed against the job metadata."
            },
        ),
    ];
    let score = factors
        .iter()
        .map(|factor| factor.points)
        .sum::<i32>()
        .clamp(0, 100) as u8;
    TrustReport {
        score,
        level: match score {
            80..=100 => "high",
            50..=79 => "medium",
            25..=49 => "low",
            _ => "untrusted",
        }
        .into(),
        factors,
    }
}

pub async fn for_job(repository: &Repository, job: &Job) -> Result<TrustReport> {
    let outputs = repository.list_job_outputs(job.id).await?;
    let checksum_available = job.expected_sha256.is_some()
        || outputs
            .iter()
            .any(|output| output.checksum_value.is_some());
    let checksum_verified = job.expected_sha256.as_ref().is_some_and(|expected| {
        outputs.iter().any(|output| {
            output
                .checksum_value
                .as_deref()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        })
    });
    let metadata_consistent = match job.total_bytes {
        Some(total) if total >= 0 => outputs
            .iter()
            .filter_map(|output| output.size_bytes)
            .any(|size| size == total as u64),
        _ => !outputs.is_empty(),
    };
    let report = compute(&TrustPreviewRequest {
        source_url: job.source.clone(),
        // The current job record does not retain a real TLS handshake result.
        // Leave this unknown rather than inferring certificate validity from completion.
        tls_certificate_valid: None,
        checksum_available,
        checksum_verified,
        signature_valid: None,
        signer_trusted: false,
        ed25519_signature: None,
        known_mirror: job.options_json.metalink.is_some(),
        metadata_consistent,
    });
    repository
        .set_library_trust_for_job(job.id, &report)
        .await?;
    Ok(report)
}

fn factor(
    code: &str,
    label: &str,
    points: i32,
    satisfied: bool,
    explanation: &str,
) -> TrustFactor {
    TrustFactor {
        code: code.into(),
        label: label.into(),
        points,
        satisfied,
        explanation: explanation.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_explainable_and_penalizes_invalid_signatures() {
        let trusted = compute(&TrustPreviewRequest {
            source_url: "https://example.test/file".into(),
            tls_certificate_valid: Some(true),
            checksum_available: true,
            checksum_verified: true,
            signature_valid: Some(true),
            signer_trusted: true,
            ed25519_signature: None,
            known_mirror: true,
            metadata_consistent: true,
        });
        assert_eq!(trusted.score, 100);
        assert_eq!(trusted.level, "high");

        let invalid = compute(&TrustPreviewRequest {
            source_url: "http://example.test/file".into(),
            signature_valid: Some(false),
            ..TrustPreviewRequest::default()
        });
        assert_eq!(invalid.score, 0);
        assert!(invalid.factors.iter().any(|factor| factor.points < 0));
    }

    #[test]
    fn verifies_ed25519_signatures_over_sha256_digests() {
        use ed25519_dalek::{Signer, SigningKey};

        let key = SigningKey::from_bytes(&[7_u8; 32]);
        let digest = [9_u8; 32];
        let signature = key.sign(&digest);
        let report = evaluate(&TrustPreviewRequest {
            source_url: "https://example.test/file".into(),
            signer_trusted: true,
            ed25519_signature: Some(Ed25519SignatureInput {
                public_key_hex: hex::encode(key.verifying_key().to_bytes()),
                signature_hex: hex::encode(signature.to_bytes()),
                signed_sha256: hex::encode(digest),
            }),
            ..TrustPreviewRequest::default()
        })
        .unwrap();
        assert!(report
            .factors
            .iter()
            .any(|factor| factor.code == "signature" && factor.satisfied));
    }
}
