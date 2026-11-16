//! Server certificates the system does not trust (SECURITY.md, Network): a self-signed WebDAV or
//! Nextcloud server the user confirmed by its SHA-256 fingerprint.
//!
//! - [`PinnedConnector`] is the TLS step of a `ureq` agent for one pinned certificate: the system's validation decides
//!   first (a server that moves to a trusted certificate just works); when it fails, only a leaf certificate with exactly
//!   the pinned fingerprint is accepted, compared in constant time, its host name not checked (self-signed certificates
//!   often name another host or an IP). The handshake's signatures are always verified, so the server must hold the
//!   certificate's key. The check happens inside the one handshake that carries the request: nothing can change between
//!   checking and sending.
//! - [`fingerprint_of`] reads the leaf certificate of a server the system rejected, for the user to compare with what the
//!   server shows. That handshake sends nothing and always fails.
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, SignatureScheme, StreamOwned,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{CryptoProvider, WebPkiSupportedAlgorithms},
};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::Duration,
};
use ureq::unversioned::transport::{Buffers, ConnectionDetails, Connector, Either, LazyBuffers, NextTimeout, Transport, TransportAdapter};

/// SHA-256 of a certificate's DER.
pub fn fingerprint(der: &[u8]) -> [u8; 32] {
    Sha256::digest(der).into()
}

fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// The system's validation, then the pinned fingerprint.
#[derive(Debug)]
struct PinVerifier {
    system: Arc<dyn ServerCertVerifier>,
    pin: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
}
impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        match self.system.verify_server_cert(end_entity, intermediates, server_name, ocsp, now) {
            Ok(verified) => Ok(verified),
            Err(_) if same(&fingerprint(end_entity), &self.pin) => Ok(ServerCertVerified::assertion()),
            // Told apart from other TLS failures by the executor: "the certificate has changed".
            Err(_) => Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)),
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The rustls error inside a `ureq` error: its own variant, or the I/O error the handshake reports it in.
pub fn rustls_error(error: &ureq::Error) -> Option<&rustls::Error> {
    match error {
        ureq::Error::Rustls(e) => Some(e),
        ureq::Error::Io(e) => e.get_ref().and_then(|inner| inner.downcast_ref::<rustls::Error>()),
        _ => None,
    }
}
/// Whether the server's certificate was rejected (by the system, or by [`PinVerifier`]).
pub fn is_certificate_error(error: &ureq::Error) -> bool {
    matches!(rustls_error(error), Some(rustls::Error::InvalidCertificate(_)))
}
/// Whether a `ureq` error is [`PinVerifier`]'s rejection (a different certificate than the pinned one).
pub fn is_pin_mismatch(error: &ureq::Error) -> bool {
    matches!(rustls_error(error), Some(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure)))
}

/// The TLS step of an agent that trusts the system's roots and one pinned certificate.
#[derive(Debug)]
pub struct PinnedConnector {
    config: Arc<ClientConfig>,
}
impl PinnedConnector {
    pub fn new(pin: [u8; 32]) -> Option<Self> {
        let provider = provider();
        let system = rustls_platform_verifier::Verifier::new(provider.clone()).ok()?;
        let verifier = PinVerifier { system: Arc::new(system), pin, algorithms: provider.signature_verification_algorithms };
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .ok()?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        Some(Self { config: Arc::new(config) })
    }
}

/// The host of a URI as rustls names it (an IPv6 address without its brackets).
fn server_name(host: &str) -> Option<ServerName<'static>> {
    ServerName::try_from(host.trim_start_matches('[').trim_end_matches(']').to_owned()).ok()
}

impl<In: Transport> Connector<In> for PinnedConnector {
    type Out = Either<In, PinnedTransport>;

    fn connect(&self, details: &ConnectionDetails, chained: Option<In>) -> Result<Option<Self::Out>, ureq::Error> {
        let Some(transport) = chained else { return Ok(None) };
        if !details.needs_tls() || transport.is_tls() {
            return Ok(Some(Either::A(transport)));
        }
        let name = details.uri.host().and_then(server_name).ok_or(ureq::Error::Tls("invalid server name"))?;
        let mut connection = ClientConnection::new(self.config.clone(), name)?;
        let mut socket = TransportAdapter::new(transport.boxed(), details.current_time.clone());
        socket.set_timeout(details.timeout);
        connection.complete_io(&mut socket)?;
        let buffers = LazyBuffers::new(details.config.input_buffer_size(), details.config.output_buffer_size());
        Ok(Some(Either::B(PinnedTransport { buffers, stream: StreamOwned { conn: connection, sock: socket } })))
    }
}

/// A TLS stream over the chained transport, as `ureq`'s own rustls transport.
pub struct PinnedTransport {
    buffers: LazyBuffers,
    stream: StreamOwned<ClientConnection, TransportAdapter>,
}
impl std::fmt::Debug for PinnedTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PinnedTransport").finish_non_exhaustive()
    }
}
impl Transport for PinnedTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        &mut self.buffers
    }
    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let output = &self.buffers.output()[..amount];
        self.stream.write_all(output)?;
        Ok(())
    }
    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        self.stream.get_mut().set_timeout(timeout);
        let input = self.buffers.input_append_buf();
        let amount = self.stream.read(input)?;
        self.buffers.input_appended(amount);
        Ok(amount > 0)
    }
    fn is_open(&mut self) -> bool {
        self.stream.get_mut().get_mut().is_open()
    }
    fn is_tls(&self) -> bool {
        true
    }
}

/// Records the leaf certificate and rejects every handshake.
#[derive(Debug)]
struct Capture {
    leaf: Mutex<Option<[u8; 32]>>,
    algorithms: WebPkiSupportedAlgorithms,
}
impl ServerCertVerifier for Capture {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        *self.leaf.lock().unwrap_or_else(|e| e.into_inner()) = Some(fingerprint(end_entity));
        Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The SHA-256 fingerprint of the leaf certificate the `https://` server at `url` presents; `None` when it can't be
/// reached. Only a handshake: nothing is sent, and it is rejected once the certificate is read.
pub fn fingerprint_of(url: &str, timeout: Duration) -> Option<[u8; 32]> {
    let uri: ureq::http::Uri = url.parse().ok()?;
    if uri.scheme_str() != Some("https") {
        return None;
    }
    let host = uri.host()?;
    let port = uri.port_u16().unwrap_or(443);
    let address = (host.trim_start_matches('[').trim_end_matches(']'), port).to_socket_addrs().ok()?.next()?;
    let mut socket = TcpStream::connect_timeout(&address, timeout).ok()?;
    socket.set_read_timeout(Some(timeout)).ok()?;
    socket.set_write_timeout(Some(timeout)).ok()?;
    let provider = provider();
    let capture = Arc::new(Capture { leaf: Mutex::new(None), algorithms: provider.signature_verification_algorithms });
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(capture.clone())
        .with_no_client_auth();
    let mut connection = ClientConnection::new(Arc::new(config), server_name(host)?).ok()?;
    // Fails once the certificate is read; nothing goes out but the handshake.
    drop(connection.complete_io(&mut socket));
    *capture.leaf.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_compare_exactly() {
        let a = fingerprint(b"certificate");
        let mut b = a;
        assert!(same(&a, &b));
        b[31] ^= 1;
        assert!(!same(&a, &b));
    }

    #[test]
    fn server_names_accept_hosts_and_bracketed_ipv6() {
        assert!(server_name("nas.example.invalid").is_some());
        assert!(server_name("192.168.1.10").is_some());
        assert!(server_name("[::1]").is_some());
    }

    #[test]
    fn plain_http_and_unreachable_servers_have_no_fingerprint() {
        assert_eq!(fingerprint_of("http://127.0.0.1:9/", Duration::from_millis(300)), None);
        assert_eq!(fingerprint_of("https://127.0.0.1:9/", Duration::from_millis(300)), None);
    }
}
