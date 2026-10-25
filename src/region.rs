//! Regional host resolution. Mirrors `packages/gate/src/region.ts`.
//!
//! The region-free org host `<org>.api[-staging].k9o.io` redirects GET/HEAD
//! only and refuses a request carrying a body, `Authorization` or cookie, so
//! credentials must go to `<org>.api[-staging]-<rgn>.k9o.io`. The SDK learns
//! the region with ONE unauthenticated `HEAD https://<org>.api[-staging].k9o.io/`
//! sent WITHOUT following redirects, takes the `308` `Location`, caches the
//! regional origin and sends every request there.
//!
//! Properties held here:
//!   - only a region-free org host is ever probed; any other base URL is used
//!     exactly as given;
//!   - the probe carries no credential and none of the configured default
//!     headers, and uses a dedicated reqwest client with
//!     `redirect::Policy::none()` (never the caller's client);
//!   - the `Location` is trusted only if https, no userinfo, no port, and the
//!     host is exactly the SAME org on the SAME apex family with a `[a-z]{2}\d{2}`
//!     region; anything else is ignored;
//!   - a definite answer (incl. "no redirect") is cached for the life of the
//!     client; a transport failure of the probe is not.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use url::Url;

/// What the probe saw: status + raw `Location` header (if any).
pub(crate) struct ProbeReply {
    pub status: u16,
    pub location: Option<String>,
}

/// The transport the probe is sent over. Production uses [`ReqwestProbe`];
/// the seam exists so tests can stub the network.
#[async_trait]
pub(crate) trait ProbeTransport: Send + Sync {
    /// `HEAD <url>` with only `accept: */*`, redirects NOT followed.
    /// `Err(())` is a transport failure.
    async fn head(&self, url: &str) -> Result<ProbeReply, ()>;
}

pub(crate) struct ReqwestProbe {
    client: reqwest::Client,
}

impl ReqwestProbe {
    pub(crate) fn new() -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

#[async_trait]
impl ProbeTransport for ReqwestProbe {
    async fn head(&self, url: &str) -> Result<ProbeReply, ()> {
        let res = self
            .client
            .head(url)
            .header("accept", "*/*")
            .send()
            .await
            .map_err(|_| ())?;
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        Ok(ProbeReply {
            status: res.status().as_u16(),
            location,
        })
    }
}

/// Per-client probe transport + cached answer.
pub(crate) struct RegionState {
    transport: Arc<dyn ProbeTransport>,
    cached: Mutex<Option<String>>,
}

impl RegionState {
    pub(crate) fn new() -> Self {
        Self::with_transport(Arc::new(ReqwestProbe::new()))
    }

    pub(crate) fn with_transport(transport: Arc<dyn ProbeTransport>) -> Self {
        Self {
            transport,
            cached: Mutex::new(None),
        }
    }
}

impl std::fmt::Debug for RegionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegionState").finish_non_exhaustive()
    }
}

fn valid_org(s: &str) -> bool {
    let b = s.as_bytes();
    (2..=63).contains(&b.len())
        && b[0].is_ascii_lowercase()
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// `(org, apex)` if `host` is exactly `<org>.(api|api-staging).k9o.io`.
fn region_free_parts(host: &str) -> Option<(&str, &str)> {
    let rest = host.strip_suffix(".k9o.io")?;
    let (org, apex) = rest.split_once('.')?;
    if !valid_org(org) || !(apex == "api" || apex == "api-staging") {
        return None;
    }
    Some((org, apex))
}

fn valid_region(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 4
        && b[0].is_ascii_lowercase()
        && b[1].is_ascii_lowercase()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
}

/// The regional origin a `Location` may name for region-free `host`, or None.
pub(crate) fn regional_origin_from_location(
    host: &str,
    location: &str,
    base: &str,
) -> Option<String> {
    let (org, apex) = region_free_parts(host)?;
    let base = Url::parse(base).ok()?;
    let u = base.join(location).ok()?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
    {
        return None;
    }
    let h = u.host_str()?;
    let rest = h.strip_suffix(".k9o.io")?;
    let (l_org, l_apex) = rest.split_once('.')?;
    let region = l_apex.strip_prefix(apex)?.strip_prefix('-')?;
    if l_org != org || !valid_region(region) {
        return None;
    }
    Some(format!("https://{}", h))
}

/// The origin this client's requests go to: the cached regional origin once
/// learned, else the configured base URL.
pub(crate) async fn resolve_origin(base_url: &str, state: &RegionState) -> String {
    let configured = base_url.trim_end_matches('/').to_string();
    if let Some(hit) = state.cached.lock().ok().and_then(|g| g.clone()) {
        return hit;
    }
    let host = match Url::parse(base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
    {
        Some(h) => h,
        None => return configured,
    };
    if region_free_parts(&host).is_none() {
        return configured;
    }
    let probe_url = format!("{}/", configured);
    match state.transport.head(&probe_url).await {
        Ok(reply) => {
            let origin = if reply.status == 308 {
                reply
                    .location
                    .as_deref()
                    .and_then(|l| regional_origin_from_location(&host, l, base_url))
            } else {
                None
            }
            .unwrap_or(configured);
            if let Ok(mut g) = state.cached.lock() {
                *g = Some(origin.clone());
            }
            origin
        }
        // Transport failure: keep the configured URL, do NOT cache.
        Err(()) => configured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{HttpClientConfig, RetryConfig};
    use crate::stream::open_sse_stream;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone)]
    struct Seen {
        url: String,
    }

    struct Stub {
        reply: Mutex<Vec<Result<(u16, Option<String>), ()>>>,
        seen: Mutex<Vec<Seen>>,
        calls: AtomicUsize,
    }

    impl Stub {
        fn new(replies: Vec<Result<(u16, Option<String>), ()>>) -> Arc<Self> {
            Arc::new(Self {
                reply: Mutex::new(replies),
                seen: Mutex::new(vec![]),
                calls: AtomicUsize::new(0),
            })
        }
        fn redirect(loc: &str) -> Arc<Self> {
            Self::new(vec![Ok((308, Some(loc.to_string())))])
        }
        fn count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ProbeTransport for Stub {
        async fn head(&self, url: &str) -> Result<ProbeReply, ()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.seen.lock().unwrap().push(Seen { url: url.into() });
            let mut q = self.reply.lock().unwrap();
            let r = if q.len() > 1 {
                q.remove(0)
            } else {
                q[0].clone()
            };
            r.map(|(status, location)| ProbeReply { status, location })
        }
    }

    const ORG: &str = "https://acme.api.k9o.io";

    #[tokio::test]
    async fn probe_goes_first_is_head_unauthenticated_then_cached() {
        let stub = Stub::redirect("https://acme.api-eu01.k9o.io/");
        let st = RegionState::with_transport(stub.clone());
        assert_eq!(
            resolve_origin(ORG, &st).await,
            "https://acme.api-eu01.k9o.io"
        );
        assert_eq!(
            resolve_origin(ORG, &st).await,
            "https://acme.api-eu01.k9o.io"
        );
        assert_eq!(stub.count(), 1, "one probe for two calls");
        assert_eq!(stub.seen.lock().unwrap()[0].url, "https://acme.api.k9o.io/");
    }

    #[tokio::test]
    async fn staging_family_and_relative_location() {
        let stub = Stub::redirect("https://acme.api-staging-de01.k9o.io/x");
        let st = RegionState::with_transport(stub);
        assert_eq!(
            resolve_origin("https://acme.api-staging.k9o.io", &st).await,
            "https://acme.api-staging-de01.k9o.io"
        );
        // Relative Location resolves against the base host => same host => not a regional host => ignored.
        let stub = Stub::redirect("/foo");
        let st = RegionState::with_transport(stub);
        assert_eq!(resolve_origin(ORG, &st).await, ORG);
    }

    #[tokio::test]
    async fn no_redirect_uses_configured_and_is_cached() {
        let stub = Stub::new(vec![Ok((200, None))]);
        let st = RegionState::with_transport(stub.clone());
        assert_eq!(resolve_origin(ORG, &st).await, ORG);
        assert_eq!(resolve_origin(ORG, &st).await, ORG);
        assert_eq!(stub.count(), 1);
        // non-308 redirect is not honoured either
        let stub = Stub::new(vec![Ok((
            301,
            Some("https://acme.api-eu01.k9o.io/".into()),
        ))]);
        let st = RegionState::with_transport(stub);
        assert_eq!(resolve_origin(ORG, &st).await, ORG);
    }

    #[tokio::test]
    async fn non_region_free_bases_are_never_probed() {
        for base in [
            "https://acme.api-eu01.k9o.io",
            "https://acme.api-staging-eu01.k9o.io",
            "https://api.k9o.io",
            "https://k9o.internal.example.com",
            "https://acme.api.k9o.io.evil.com",
            "https://evil-acme.api.k9o.io.example",
            "http://localhost:8080",
            "https://a.api.k9o.io",
        ] {
            let stub = Stub::redirect("https://acme.api-eu01.k9o.io/");
            let st = RegionState::with_transport(stub.clone());
            assert_eq!(resolve_origin(base, &st).await, base);
            assert_eq!(stub.count(), 0, "{base} must not be probed");
        }
    }

    #[tokio::test]
    async fn hostile_locations_refused() {
        for loc in [
            "https://evil.com/",
            "https://acme.api-eu01.k9o.io.evil.com/",
            "https://acme.api-eu01.k9o.io.evil/",
            "https://evilacme.api-eu01.k9o.io/",
            "https://other.api-eu01.k9o.io/",
            "https://acme.api-staging-eu01.k9o.io/",
            "http://acme.api-eu01.k9o.io/",
            "https://user:pw@acme.api-eu01.k9o.io/",
            "https://user@acme.api-eu01.k9o.io/",
            "https://acme.api-eu01.k9o.io:8443/",
            "https://acme.api-europe.k9o.io/",
            "https://acme.api-eu1.k9o.io/",
            "https://acme.api-EU01x.k9o.io/",
            "https://acme.api-eu01.k9o.io./",
            "//evil.com/",
        ] {
            let stub = Stub::redirect(loc);
            let st = RegionState::with_transport(stub);
            assert_eq!(resolve_origin(ORG, &st).await, ORG, "{loc}");
        }
        // and the staging family refuses a prod-family regional host
        let st = RegionState::with_transport(Stub::redirect("https://acme.api-eu01.k9o.io/"));
        assert_eq!(
            resolve_origin("https://acme.api-staging.k9o.io", &st).await,
            "https://acme.api-staging.k9o.io"
        );
    }

    #[tokio::test]
    async fn probe_network_failure_is_not_cached() {
        let stub = Stub::new(vec![
            Err(()),
            Ok((308, Some("https://acme.api-eu01.k9o.io/".into()))),
        ]);
        let st = RegionState::with_transport(stub.clone());
        assert_eq!(resolve_origin(ORG, &st).await, ORG);
        assert_eq!(
            resolve_origin(ORG, &st).await,
            "https://acme.api-eu01.k9o.io"
        );
        assert_eq!(stub.count(), 2);
    }

    /// Config whose normal client cannot reach anything (connection refused),
    /// so the failing request's `url` context reveals where it was sent.
    fn cfg(stub: Arc<Stub>, base: &str) -> HttpClientConfig {
        let normal = reqwest::Client::builder()
            .resolve("acme.api-eu01.k9o.io", "127.0.0.1:1".parse().unwrap())
            .resolve("acme.api.k9o.io", "127.0.0.1:1".parse().unwrap())
            .build()
            .unwrap();
        HttpClientConfig {
            base_url: base.into(),
            api_key: Some("secret-key".into()),
            org_slug: Some("acme".into()),
            project_slug: Some("p".into()),
            default_headers: HashMap::from([("x-custom".into(), "v".into())]),
            reqwest: Arc::new(normal),
            retry: RetryConfig {
                max_retries: 0,
                ..Default::default()
            },
            region: Arc::new(RegionState::with_transport(stub)),
        }
    }

    #[tokio::test]
    async fn request_and_gate_and_stream_go_to_regional_host() {
        use crate::gate::{GateNamespace, SignInInput};
        let stub = Stub::redirect("https://acme.api-eu01.k9o.io/");
        let c = cfg(stub.clone(), ORG);

        let g = GateNamespace { config: &c };
        let e = g
            .sign_in(SignInInput {
                email: "a@b.c",
                password: "pw",
            })
            .await
            .unwrap_err();
        let url = e.context["url"].as_str().unwrap().to_string();
        assert!(url.starts_with("https://acme.api-eu01.k9o.io/"), "{url}");

        let r = open_sse_stream::<serde_json::Value, ()>(
            &c,
            reqwest::Method::GET,
            "/v1/x/stream",
            None,
            None,
        )
        .await;
        let e = r.err().unwrap();
        let url = e.context["url"].as_str().unwrap().to_string();
        assert!(url.starts_with("https://acme.api-eu01.k9o.io/"), "{url}");

        assert_eq!(stub.count(), 1, "one probe across request + gate + stream");
    }

    #[tokio::test]
    async fn custom_base_request_not_probed() {
        let stub = Stub::redirect("https://acme.api-eu01.k9o.io/");
        let c = cfg(stub.clone(), "https://acme.api-eu01.k9o.io");
        let _ = crate::http::request::<serde_json::Value, ()>(
            &c,
            reqwest::Method::GET,
            "/v1/x",
            None,
            None,
        )
        .await;
        assert_eq!(stub.count(), 0);
    }

    #[tokio::test]
    async fn real_probe_is_head_no_credentials_and_does_not_follow_redirect() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let srv = MockServer::start().await;
        Mock::given(method("HEAD"))
            .and(path("/"))
            .respond_with(
                ResponseTemplate::new(308)
                    .insert_header("location", format!("{}/followed", srv.uri()).as_str()),
            )
            .mount(&srv)
            .await;
        Mock::given(path("/followed"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&srv)
            .await;
        let r = ReqwestProbe::new()
            .head(&format!("{}/", srv.uri()))
            .await
            .ok()
            .unwrap();
        assert_eq!(r.status, 308);
        assert_eq!(
            r.location.as_deref(),
            Some(format!("{}/followed", srv.uri()).as_str())
        );
        let reqs = srv.received_requests().await.unwrap();
        assert_eq!(reqs.len(), 1, "redirect must not be followed");
        assert_eq!(reqs[0].method.as_str(), "HEAD");
        for h in [
            "authorization",
            "cookie",
            "x-custom",
            "x-k9o-org-slug",
            "x-k9o-project-slug",
        ] {
            assert!(reqs[0].headers.get(h).is_none(), "{h} leaked on probe");
        }
    }
}
