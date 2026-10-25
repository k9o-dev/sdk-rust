//! URL grammar helpers.

use crate::kerror::KError;
use url::Url;

/// Validate + normalise a baseUrl.
/// Returns `Ok(normalised)` or `Err(KError(K9O_INVALID_BASE_URL))`.
pub fn validate_base_url(base_url: &str) -> Result<String, KError> {
    let parsed = Url::parse(base_url).map_err(|e| {
        KError::new("K9O_INVALID_BASE_URL", format!("base_url {:?} is not a valid URL: {}", base_url, e))
            .with_context("base_url", base_url)
    })?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(KError::new(
            "K9O_INVALID_BASE_URL",
            format!("base_url {:?} uses unsupported scheme {:?}; expected http or https", base_url, scheme),
        )
        .with_context("base_url", base_url)
        .with_context("scheme", scheme));
    }
    if parsed.host_str().is_none() {
        return Err(KError::new(
            "K9O_INVALID_BASE_URL",
            format!("base_url {:?} missing host", base_url),
        ));
    }
    if parsed.path() != "" && parsed.path() != "/" {
        return Err(KError::new(
            "K9O_INVALID_BASE_URL",
            format!(
                "base_url {:?} must not contain a path segment; got {:?}",
                base_url,
                parsed.path()
            ),
        ));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(KError::new(
            "K9O_INVALID_BASE_URL",
            format!("base_url {:?} must not contain query or fragment", base_url),
        ));
    }

    let host = parsed.host_str().unwrap();
    let normalised = match parsed.port() {
        Some(port) => format!("{}://{}:{}", scheme, host, port),
        None => format!("{}://{}", scheme, host),
    };
    Ok(normalised)
}

/// Bare platform apex hosts org-subdomain derivation applies to — production
/// and staging share the `<org_slug>.<apex>` wire-routing scheme (confirmed
/// live). A third apex needs the same confirmation before joining this set.
const K9O_ORG_DERIVABLE_APEXES: [&str; 2] = ["api.k9o.io", "api-staging.k9o.io"];

/// Resolve the effective baseUrl. `base_url` is
/// OPTIONAL — it defaults to the platform apex `https://api.k9o.io`. When the
/// (validated) host is a bare apex in `K9O_ORG_DERIVABLE_APEXES` and
/// `org_slug` is supplied, the SDK composes the canonical org-subdomain wire
/// root `https://<org_slug>.<apex>`. An explicit `<org_slug>`
/// subdomain, a self-hosted / Sovereign host, or a dev host (anything not in
/// that set) passes through unchanged — `org_slug` is intentionally NOT
/// composed onto a non-apex host.
///
/// Fixed 2026-08-30 (SDK-2): this used to silently ignore `org_slug` for
/// `api-staging.k9o.io` too, not just genuine self-hosted/custom hosts,
/// since only the literal production apex string was checked.
///
/// Mirrors `resolveBaseUrl(baseUrl?, orgSlug?)` in `packages/core/src/url-grammar.ts`.
pub fn resolve_base_url(base_url: Option<&str>, org_slug: Option<&str>) -> Result<String, KError> {
    let validated = validate_base_url(base_url.unwrap_or("https://api.k9o.io"))?;
    // Re-parse the validated form to inspect host + scheme.
    let parsed = Url::parse(&validated).map_err(|e| {
        KError::new("K9O_INVALID_BASE_URL", format!("base_url {:?} is not a valid URL: {}", validated, e))
            .with_context("base_url", validated.as_str())
    })?;
    if let Some(host) = parsed.host_str() {
        if K9O_ORG_DERIVABLE_APEXES.contains(&host) {
            if let Some(slug) = org_slug {
                validate_slug(slug, "org")?;
                return Ok(format!("{}://{}.{}", parsed.scheme(), slug, host));
            }
        }
    }
    Ok(validated)
}

/// Validate org/project slug.
pub fn validate_slug(slug: &str, kind: &str) -> Result<(), KError> {
    let valid = slug.len() >= 2
        && slug.len() <= 63
        && slug.chars().next().map_or(false, |c| c.is_ascii_lowercase())
        && !slug.ends_with('-')
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(KError::new(
            "K9O_INVALID_FIELD_VALUE",
            format!(
                "{}_slug {:?} does not match required format ^[a-z][a-z0-9-]{{1,62}}$ (must not end with hyphen)",
                kind, slug
            ),
        )
        .with_context("slug", slug)
        .with_context("kind", kind));
    }
    Ok(())
}

/// Member-plane path: `/@<slug>/<sub-resource>`.
pub fn project_path(project_slug: &str, sub_resource: &str) -> String {
    let cleaned = sub_resource.strip_prefix('/').unwrap_or(sub_resource);
    format!("/@{}/{}", project_slug, cleaned)
}

/// Org-plane path: `/<resource>`.
pub fn org_path(resource: &str) -> String {
    format!("/{}", resource.strip_prefix('/').unwrap_or(resource))
}

/// Platform-plane path: `/<resource>`.
pub fn platform_path(resource: &str) -> String {
    format!("/{}", resource.strip_prefix('/').unwrap_or(resource))
}
