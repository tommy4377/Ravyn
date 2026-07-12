use std::path::{Component, Path};

use crate::{
    config::Config,
    error::{RavynError, Result},
};

/// Ensures output paths stay inside Ravyn's configured download root.
pub fn validate_output_path(config: &Config, path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(RavynError::Invalid(
            "output path may not contain parent traversal".into(),
        ));
    }
    let root = std::fs::canonicalize(absolutize(&config.effective_download_dir())?)?;
    let candidate = absolutize(path)?;
    let mut existing = candidate.as_path();
    while !existing.exists() {
        existing = existing
            .parent()
            .ok_or_else(|| RavynError::Invalid("output path has no existing ancestor".into()))?;
    }
    let resolved_ancestor = std::fs::canonicalize(existing)?;
    if !resolved_ancestor.starts_with(&root) {
        return Err(RavynError::Invalid(format!(
            "output path {} is outside the configured download root {}",
            candidate.display(),
            root.display()
        )));
    }
    Ok(())
}

fn absolutize(path: &Path) -> Result<std::path::PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

/// Rejects absolute paths and traversal in templates passed to external tools.
pub fn validate_relative_template(value: &str, label: &str) -> Result<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(RavynError::Invalid(format!(
            "{label} must stay relative to the job destination"
        )));
    }
    Ok(())
}

pub fn validate_regular_file_under(path: &Path, root: &Path, label: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(RavynError::Invalid(format!(
            "{label} must be a regular non-symlink file"
        )));
    }
    let canonical_root = std::fs::canonicalize(root)?;
    let canonical_path = std::fs::canonicalize(path)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(RavynError::Invalid(format!(
            "{label} must be stored under {}",
            canonical_root.display()
        )));
    }
    Ok(())
}

/// Blocks obviously dangerous local-network targets unless explicitly enabled.
pub fn validate_network_source(config: &Config, source: &str) -> Result<()> {
    if source.len() > 16_384 {
        return Err(RavynError::Invalid(
            "network source URLs may not exceed 16384 characters".into(),
        ));
    }
    let url = url::Url::parse(source)?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(RavynError::Invalid(
            "only HTTP and HTTPS sources are supported".into(),
        ));
    }
    if url.host_str().is_none() {
        return Err(RavynError::Invalid("network source URL has no host".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(RavynError::Invalid(
            "network source URLs may not contain credentials; use a secret reference".into(),
        ));
    }
    if url.fragment().is_some() {
        return Err(RavynError::Invalid(
            "network source URLs may not contain fragments".into(),
        ));
    }
    if config.allow_private_network {
        return Ok(());
    }
    let host = normalize_url_host(url.host_str().unwrap_or_default());
    if host.eq_ignore_ascii_case("localhost") {
        return Err(RavynError::Invalid(
            "localhost downloads require --allow-private-network".into(),
        ));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let private = match ip {
            std::net::IpAddr::V4(ip) => {
                ip.is_private() || ip.is_loopback() || ip.is_link_local() || ip.is_unspecified()
            }
            std::net::IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
            }
        };
        if private {
            return Err(RavynError::Invalid(
                "private-network downloads require --allow-private-network".into(),
            ));
        }
    }
    Ok(())
}

/// Resolves the target before the request and rejects hostnames pointing to private networks.
///
/// This supplements the lexical URL check and prevents common DNS-rebinding and
/// server-side request forgery paths. Callers should still avoid following a
/// redirect to a private address; Ravyn validates the final URL after redirects.
pub async fn validate_network_source_resolved(config: &Config, source: &str) -> Result<()> {
    resolve_network_source(config, source).await.map(|_| ())
}

/// Resolves every address that may be used for a connection and returns the
/// approved set so the HTTP client can pin the exact DNS result. Mixed
/// public/private answers are rejected by the same policy as wholly private
/// answers, closing the validation-then-rebind gap.
pub async fn resolve_network_source(
    config: &Config,
    source: &str,
) -> Result<Vec<std::net::SocketAddr>> {
    validate_network_source(config, source)?;
    let url = url::Url::parse(source)?;
    let host = url
        .host_str()
        .ok_or_else(|| RavynError::Invalid("download URL has no host".into()))?;
    let host = normalize_url_host(host);
    let port = url
        .port_or_known_default()
        .ok_or_else(|| RavynError::Invalid("download URL has no known port".into()))?;
    let mut addresses = if let Ok(address) = host.parse::<std::net::IpAddr>() {
        vec![std::net::SocketAddr::new(address, port)]
    } else {
        tokio::net::lookup_host((host, port)).await?.collect()
    };
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() {
        return Err(RavynError::Invalid(format!(
            "{host} did not resolve to any address"
        )));
    }
    if !config.allow_private_network {
        if let Some(address) = addresses
            .iter()
            .find(|address| is_private_address(address.ip()))
        {
            return Err(RavynError::Invalid(format!(
                "{host} resolves to private address {}; use --allow-private-network to permit it",
                address.ip()
            )));
        }
    }
    Ok(addresses)
}

fn is_private_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_documentation()
        }
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

fn normalize_url_host(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host)
}

#[cfg(test)]
mod property_tests {
    use clap::Parser;
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        #[test]
        fn output_confinement_accepts_children_and_rejects_parent_traversal(
            component in "[A-Za-z0-9_-]{1,32}"
        ) {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("downloads");
            std::fs::create_dir_all(&root).unwrap();
            let config = Config::try_parse_from([
                "ravyn",
                "--data-dir",
                temp.path().to_str().unwrap(),
                "--download-dir",
                root.to_str().unwrap(),
            ]).unwrap();
            prop_assert!(validate_output_path(&config, &root.join(&component)).is_ok());
            prop_assert!(validate_output_path(
                &config,
                &root.join("..").join(&component),
            ).is_err());
        }
    }

    #[test]
    fn rejects_private_and_special_ip_literal_sources_by_default() {
        let config = Config::try_parse_from(["ravyn"]).unwrap();
        for source in [
            "http://127.0.0.1/file",
            "http://10.0.0.1/file",
            "http://169.254.1.1/file",
            "http://[::1]/file",
            "http://[fe80::1]/file",
        ] {
            assert!(
                validate_network_source(&config, source).is_err(),
                "{source}"
            );
        }
    }

    #[test]
    fn rejects_embedded_credentials_even_when_private_networks_are_allowed() {
        let mut config = Config::try_parse_from(["ravyn"]).unwrap();
        config.allow_private_network = true;
        assert!(
            validate_network_source(&config, "https://user:password@example.com/file").is_err()
        );
        assert!(validate_network_source(&config, "https://example.com/file#secret").is_err());
    }
}
