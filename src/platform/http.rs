//! The HTTP executor for `cloudset` and site icons (SECURITY.md, Network): sends each request as is with `ureq` + rustls and the
//! system's certificate verification. No redirects (3xx goes back to the core), no cookies, no cache, bodies read only up to
//! the core's limit, fixed timeouts. Never logs URLs, headers or bodies.
use safay_app::cloud::{Http, TransportError};
use safay_app::cloudset::{CertificatePin, HttpRequest, HttpResponse};
use std::{collections::HashMap, io::Read, sync::Mutex, time::Duration};

pub struct Executor {
    agent: ureq::Agent,
    /// One agent per pinned certificate (`super::tls`), made on first use.
    pinned: Mutex<HashMap<[u8; 32], ureq::Agent>>,
    connect: Duration,
    total: Duration,
    max_body: usize,
}
impl Executor {
    fn config(connect: Duration, total: Duration) -> ureq::config::Config {
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        ureq::Agent::config_builder()
            .max_redirects(0)
            // WebDAV's PROPFIND and MKCOL: without this, ureq refuses them before sending anything.
            .allow_non_standard_methods(true)
            .http_status_as_error(false)
            .timeout_connect(Some(connect))
            .timeout_global(Some(total))
            .tls_config(tls)
            .build()
    }
    pub fn new(connect: Duration, total: Duration, max_body: usize) -> Self {
        Self { agent: Self::config(connect, total).into(), pinned: Mutex::new(HashMap::new()), connect, total, max_body }
    }
    /// The agent for requests a pinned certificate covers: plain TCP, then the pinned TLS step. No proxies, as the
    /// system agent's direct connections.
    fn pinned_agent(&self, pin: [u8; 32]) -> Option<ureq::Agent> {
        use ureq::unversioned::{
            resolver::DefaultResolver,
            transport::{Connector, TcpConnector},
        };
        let mut agents = self.pinned.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(agent) = agents.get(&pin) {
            return Some(agent.clone());
        }
        let connector = TcpConnector::default().chain(super::tls::PinnedConnector::new(pin)?);
        let agent = ureq::Agent::with_parts(Self::config(self.connect, self.total), connector, DefaultResolver::default());
        agents.insert(pin, agent.clone());
        Some(agent)
    }
    /// Cloud storage: 10 s to connect, 60 s per request, the core's 16 MiB body limit.
    pub fn storage() -> Self {
        Self::new(Duration::from_secs(10), Duration::from_secs(60), safay_app::cloudset::Limits::default().max_body_bytes)
    }
}
impl Http for Executor {
    fn execute(&self, request: HttpRequest, pin: Option<&CertificatePin>) -> Result<HttpResponse, TransportError> {
        self.execute_with_limit(request, pin, self.max_body)
    }
}
impl Executor {
    /// As `execute`, reading at most `limit + 1` bytes of the body (site icons give a limit per request).
    pub fn execute_with_limit(
        &self,
        request: HttpRequest,
        pin: Option<&CertificatePin>,
        limit: usize,
    ) -> Result<HttpResponse, TransportError> {
        // A request the pin covers goes through the pinned TLS step; every other request, the system's validation alone.
        let pinned = pin.filter(|p| p.applies_to(&request.url)).map(|p| *p.certificate().as_bytes());
        let agent = match pinned {
            Some(pin) => self.pinned_agent(pin).ok_or(TransportError::Unreachable)?,
            None => self.agent.clone(),
        };
        let url = request.url.clone();
        let mut builder = ureq::http::Request::builder().method(request.method.as_str());
        builder = builder.uri(request.url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let built = builder.body(request.body).map_err(|_| TransportError::Unreachable)?;
        let mut response = agent.run(built).map_err(|e| match e {
            ref e if pinned.is_some() && super::tls::is_pin_mismatch(e) => TransportError::PinMismatch,
            // Read the certificate the system rejected, for the user to compare (setup only offers to trust it).
            ref e if pinned.is_none() && super::tls::is_certificate_error(e) => {
                TransportError::Untrusted(super::tls::fingerprint_of(&url, self.connect))
            }
            ref e if super::tls::is_certificate_error(e) => TransportError::Untrusted(None),
            _ => TransportError::Unreachable,
        })?;
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_owned(), v.to_owned())))
            .collect();
        // Read at most limit + 1 bytes: the core reports anything longer as too large.
        let mut body = Vec::new();
        response.body_mut().as_reader().take(limit as u64 + 1).read_to_end(&mut body).map_err(|_| TransportError::Unreachable)?;
        Ok(HttpResponse::new(status).with_headers(headers).with_body(body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safay_app::cloudset::Method;
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };

    /// A one-shot plain-HTTP server on the loopback interface that answers with `reply`.
    fn serve(reply: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let mut stream = stream;
            drop(stream.write_all(&reply));
        });
        address
    }
    fn get(url: String) -> HttpRequest {
        HttpRequest::new(Method::Get, url).with_header("X-Test", "1")
    }

    #[test]
    fn redirects_come_back_unfollowed() {
        let url = serve(b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/elsewhere\r\nContent-Length: 0\r\n\r\n".to_vec());
        let executor = Executor::new(Duration::from_secs(5), Duration::from_secs(5), 1024);
        let response = executor.execute(get(url), None).unwrap();
        assert_eq!(response.status, 302);
        assert_eq!(response.header("location"), Some("http://127.0.0.1:9/elsewhere"));
    }

    #[test]
    fn bodies_are_read_only_up_to_the_limit() {
        let mut reply = b"HTTP/1.1 200 OK\r\nContent-Length: 4096\r\n\r\n".to_vec();
        reply.extend(std::iter::repeat_n(b'x', 4096));
        let executor = Executor::new(Duration::from_secs(5), Duration::from_secs(5), 100);
        let response = executor.execute(get(serve(reply)), None).unwrap();
        assert_eq!(response.body.len(), 101, "limit + 1, so the core reports it as too large");
    }

    #[test]
    fn webdav_methods_are_sent() {
        // ureq refuses methods outside RFC 9110 unless allowed; WebDAV and Nextcloud need PROPFIND and MKCOL.
        let executor = Executor::new(Duration::from_secs(5), Duration::from_secs(5), 1024);
        for method in [Method::Propfind, Method::Mkcol] {
            let url = serve(b"HTTP/1.1 207 Multi-Status\r\nContent-Length: 0\r\n\r\n".to_vec());
            let request = HttpRequest::new(method, url).with_header("Depth", "0").with_body(b"<propfind/>".to_vec());
            assert_eq!(executor.execute(request, None).unwrap().status, 207);
        }
    }

    /// A loopback HTTPS server with the synthetic self-signed certificate in `testdata/`: it answers `200 ok` to every
    /// request and keeps accepting (the fingerprint probe's handshake comes first).
    fn serve_self_signed() -> (String, [u8; 32]) {
        use rustls::{ServerConfig, ServerConnection, StreamOwned};
        use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        let cert = include_bytes!("testdata/self-signed.der").to_vec();
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(include_bytes!("testdata/self-signed.key.der").to_vec()));
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        let config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(cert.clone())], key)
            .unwrap();
        let config = std::sync::Arc::new(config);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("https://{}/", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let config = config.clone();
                std::thread::spawn(move || {
                    let Ok(connection) = ServerConnection::new(config) else { return };
                    let mut tls = StreamOwned::new(connection, stream);
                    let mut reader = BufReader::new(&mut tls);
                    let mut line = String::new();
                    while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                        line.clear();
                    }
                    drop(tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"));
                    drop(tls.flush());
                });
            }
        });
        (address, super::super::tls::fingerprint(&cert))
    }
    fn pin(url: &str, fingerprint: [u8; 32]) -> CertificatePin {
        let endpoint = safay_app::cloudset::Endpoint::parse(url, safay_app::cloudset::Transport::RequireHttps).unwrap();
        CertificatePin::new(&endpoint, safay_app::cloudset::PinnedCertificate::from_slice(&fingerprint).unwrap()).unwrap()
    }

    #[test]
    fn a_self_signed_server_is_trusted_only_by_its_confirmed_fingerprint() {
        let (url, fingerprint) = serve_self_signed();
        let executor = Executor::new(Duration::from_secs(5), Duration::from_secs(5), 1024);
        // Not pinned: the system rejects it, and the executor reads the certificate for the user to compare.
        assert_eq!(executor.execute(get(url.clone()), None).unwrap_err(), TransportError::Untrusted(Some(fingerprint)));
        // Pinned to exactly that certificate: the request goes through, though the host name is not the certificate's.
        let response = executor.execute(get(url.clone()), Some(&pin(&url, fingerprint))).unwrap();
        assert_eq!((response.status, response.body.as_slice()), (200, b"ok".as_slice()));
        // Pinned to another certificate: refused as a changed certificate, never sent.
        let mut other = fingerprint;
        other[0] ^= 0xff;
        assert_eq!(executor.execute(get(url.clone()), Some(&pin(&url, other))).unwrap_err(), TransportError::PinMismatch);
        // A pin covers only its own origin: another port of the same host gets the system's validation alone.
        let elsewhere = pin("https://127.0.0.1:1/", fingerprint);
        assert!(matches!(executor.execute(get(url), Some(&elsewhere)).unwrap_err(), TransportError::Untrusted(_)));
    }

    #[test]
    fn unreachable_and_pinned_requests_fail_as_transport_errors() {
        let executor = Executor::new(Duration::from_millis(500), Duration::from_secs(2), 100);
        // Port 9 (discard) on loopback is closed on a test machine.
        assert_eq!(executor.execute(get("http://127.0.0.1:9/".into()), None).unwrap_err(), TransportError::Unreachable);
    }
}
