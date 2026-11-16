//! Cloud storage backup (SECURITY.md, Network): `cloudset` builds every request; this module only runs its state machines
//! with an injected HTTP executor and keeps the serialized connection in an injected credential store (the system
//! keychain in the app, memory in tests). Remote files are the folder backend's: `Safay/vault.safayvault` and
//! `Safay/vault.metadata.json`, only ciphertext and non-secret metadata.
use crate::sync::{Backend, Expect, Remote, SyncError};
use cloudset::{
    BoxOperation, Client, ClientConfig, CloudError, Connection, HttpRequest, HttpResponse, Precondition, ProviderKind, RemotePath,
    Revision, Step,
};
use safay_core::{
    backup::{BackupMetadata, DEFAULT_FOLDER, METADATA_FILE_NAME, VAULT_FILE_NAME},
    sync::RemoteVersion,
    vault,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use zeroize::Zeroizing;

/// The storage services a location can use.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    Webdav,
    Nextcloud,
    Dropbox,
    Onedrive,
    GoogleDrive,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Webdav => "WebDAV",
            Provider::Nextcloud => "Nextcloud",
            Provider::Dropbox => "Dropbox",
            Provider::Onedrive => "OneDrive",
            Provider::GoogleDrive => "Google Drive",
        }
    }
    pub fn of(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::WebDav => Provider::Webdav,
            ProviderKind::Nextcloud => Provider::Nextcloud,
            ProviderKind::Dropbox => Provider::Dropbox,
            ProviderKind::OneDrive => Provider::Onedrive,
            ProviderKind::GoogleDrive => Provider::GoogleDrive,
        }
    }
}

/// A transport failure, reported by the executor; it never enters `cloudset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    /// Offline, DNS, timeout, connection reset, or another TLS failure.
    Unreachable,
    /// The certificate failed system validation (and no pin covers the request), with the SHA-256 of the leaf
    /// certificate when the executor could read it: the user can confirm it while setting up a location.
    Untrusted(Option<[u8; 32]>),
    /// A pinned server presented a different certificate.
    PinMismatch,
}

/// Sends one request as is: no redirects, no cookies, no cache, at most `request.limits` of body.
pub trait Http: Send + Sync {
    fn execute(&self, request: HttpRequest, pin: Option<&cloudset::CertificatePin>) -> Result<HttpResponse, TransportError>;
}

/// Where serialized connections live. The app uses the system credential store; tests use memory.
pub trait Credentials: Send + Sync {
    fn load(&self, id: &str) -> Option<Zeroizing<Vec<u8>>>;
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), SyncError>;
    fn delete(&self, id: &str);
}

/// The process's services, installed once at startup (or by a test).
#[derive(Clone)]
pub struct Services {
    pub http: Arc<dyn Http>,
    pub credentials: Arc<dyn Credentials>,
    pub user_agent: String,
}
static SERVICES: RwLock<Option<Services>> = RwLock::new(None);
pub fn install(services: Services) {
    *SERVICES.write().unwrap_or_else(|e| e.into_inner()) = Some(services);
}
pub fn services() -> Result<Services, SyncError> {
    SERVICES.read().unwrap_or_else(|e| e.into_inner()).clone().ok_or(SyncError::Network)
}

/// Runs an operation to completion, saving a refreshed connection. Transport failures abort it.
pub fn run<T>(
    services: &Services,
    credential: Option<&str>,
    mut operation: BoxOperation<T>,
    pin: Option<cloudset::CertificatePin>,
) -> Result<T, SyncError> {
    let mut response = None;
    loop {
        match operation.advance(response.take()).map_err(map_error)? {
            Step::Send(request) => {
                let reply = services.http.execute(request, pin.as_ref()).map_err(|e| match e {
                    TransportError::Unreachable => SyncError::Network,
                    TransportError::Untrusted(None) => SyncError::Certificate,
                    TransportError::Untrusted(Some(fingerprint)) => SyncError::CertificateUnverified(fingerprint),
                    TransportError::PinMismatch => SyncError::CertificateChanged,
                })?;
                response = Some(reply);
            }
            Step::Done(done) => {
                if let (Some(id), Some(connection)) = (credential, done.connection.as_ref()) {
                    services.credentials.save(id, &connection.to_bytes())?;
                }
                return Ok(done.value);
            }
        }
    }
}

pub fn map_error(e: CloudError) -> SyncError {
    match e {
        CloudError::ReauthRequired | CloudError::AuthorizationDenied | CloudError::InvalidConnection => SyncError::SignIn,
        CloudError::Forbidden => SyncError::Forbidden,
        CloudError::NotFound => SyncError::NoRemote,
        CloudError::PreconditionFailed => SyncError::RemoteChanged,
        CloudError::QuotaExceeded => SyncError::Quota,
        CloudError::RateLimited { .. } => SyncError::RateLimited,
        CloudError::Server { status } => SyncError::Server(status),
        CloudError::TooLarge => SyncError::TooLarge,
        _ => SyncError::Protocol,
    }
}

pub struct CloudBackend {
    credential: String,
}
impl CloudBackend {
    pub fn new(credential: String) -> Self {
        Self { credential }
    }
    fn client(&self, services: &Services) -> Result<(Client, Option<cloudset::CertificatePin>), SyncError> {
        let blob = services.credentials.load(&self.credential).ok_or(SyncError::SignIn)?;
        let connection = Connection::from_bytes(&blob).map_err(|_| SyncError::SignIn)?;
        let pin = connection.certificate_pin();
        Ok((Client::new(connection, ClientConfig::new(services.user_agent.clone())), pin))
    }
}
fn path(name: &str) -> RemotePath {
    RemotePath::parse(&format!("{DEFAULT_FOLDER}/{name}")).expect("static path")
}
impl Backend for CloudBackend {
    fn read(&self) -> Result<Option<Remote>, SyncError> {
        let services = services()?;
        let (client, pin) = self.client(&services)?;
        let Some(file) = run(&services, Some(&self.credential), client.read(path(VAULT_FILE_NAME)), pin.clone())? else {
            return Ok(None);
        };
        if !vault::is_valid_envelope(&file.data) {
            return Err(SyncError::Corrupt);
        }
        let device = run(&services, Some(&self.credential), client.read(path(METADATA_FILE_NAME)), pin)
            .ok()
            .flatten()
            .and_then(|m| BackupMetadata::device_name(&m.data));
        let version = file.entry.revision.map(|r| RemoteVersion::Revision { value: r.as_str().to_owned() });
        Ok(Some(Remote { data: file.data, version, device }))
    }
    fn write(&self, data: &[u8], metadata: &BackupMetadata, expect: &Expect) -> Result<Option<RemoteVersion>, SyncError> {
        let services = services()?;
        let (client, pin) = self.client(&services)?;
        // The folder may exist already; any other failure surfaces on the write itself.
        let _ =
            run(&services, Some(&self.credential), client.create_dir(RemotePath::parse(DEFAULT_FOLDER).expect("static path")), pin.clone());
        let precondition = match expect {
            Expect::Any => Precondition::None,
            Expect::Absent => Precondition::Absent,
            Expect::Unchanged(RemoteVersion::Revision { value }) => Precondition::Matches(Revision::new(value.clone())),
            Expect::Unchanged(RemoteVersion::ModifiedAt { .. }) => Precondition::None,
        };
        let entry = run(&services, Some(&self.credential), client.write(path(VAULT_FILE_NAME), data.to_vec(), precondition), pin.clone())?;
        // Metadata is only for display; a failed write does not fail the backup.
        let _ = run(&services, Some(&self.credential), client.write(path(METADATA_FILE_NAME), metadata.encode(), Precondition::None), pin);
        Ok(entry.revision.map(|r| RemoteVersion::Revision { value: r.as_str().to_owned() }))
    }
}

/// Signs out: revokes the grant where the service has a revocation endpoint, then forgets the stored connection.
pub fn forget(credential: &str) {
    if let Ok(services) = services() {
        if let Some(blob) = services.credentials.load(credential) {
            revoke(&blob);
        }
        services.credentials.delete(credential);
    }
}
/// Revokes a serialized connection's grant (best effort) that is stored nowhere else any more.
pub fn revoke(connection: &[u8]) {
    if let (Ok(services), Ok(connection)) = (services(), Connection::from_bytes(connection)) {
        let pin = connection.certificate_pin();
        let client = Client::new(connection, ClientConfig::new(services.user_agent.clone()));
        let _ = run(&services, None, client.revoke(), pin);
    }
}
/// Removes a stored connection without revoking it and hands it back (with any tokens refreshed while stored).
pub fn take(credential: &str) -> Option<Zeroizing<Vec<u8>>> {
    let services = services().ok()?;
    let blob = services.credentials.load(credential);
    services.credentials.delete(credential);
    blob
}
/// Checks and stores a connection kept from an earlier sign-in, as [`connect`] does for a new one.
pub fn reconnect(connection: &[u8]) -> Result<crate::sync::Location, SyncError> {
    connect(Connection::from_bytes(connection).map_err(map_error)?)
}

/// The cloud sign-ins made during first-run restore, one per service, so choosing the service again needs no new
/// sign-in: after "no backup found", an error, or going back from the password step. Memory only (never the keychain or
/// disk) and gone when the app quits. For WebDAV and Nextcloud the form submitted (`F`, with the WebDAV password) is kept
/// too, to prefill it. Safay signs in to the app folder only, so one sign-in per service is enough.
///
/// A connection is held here only while it is stored nowhere else: the restore location's own sign-in stays in the
/// credential store until that location is dropped ([`take`]). The caller revokes what [`Self::signed_in`] replaces and
/// what [`Self::release`] gives back, the sign-ins nothing uses once the vault exists.
pub struct RestoreSignIns<F> {
    entries: Vec<(Provider, SignIn<F>)>,
}
struct SignIn<F> {
    connection: Option<Zeroizing<Vec<u8>>>,
    form: Option<F>,
}
impl<F> Default for RestoreSignIns<F> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}
impl<F: Clone> RestoreSignIns<F> {
    fn entry(&mut self, provider: Provider) -> &mut SignIn<F> {
        if let Some(i) = self.entries.iter().position(|(p, _)| *p == provider) {
            return &mut self.entries[i].1;
        }
        self.entries.push((provider, SignIn { connection: None, form: None }));
        &mut self.entries.last_mut().expect("just pushed").1
    }
    fn get(&self, provider: Provider) -> Option<&SignIn<F>> {
        self.entries.iter().find(|(p, _)| *p == provider).map(|(_, e)| e)
    }
    /// The form last submitted for `provider`.
    pub fn form(&self, provider: Provider) -> Option<F> {
        self.get(provider).and_then(|e| e.form.clone())
    }
    /// The kept connection for `provider`, to [`reconnect`] with.
    pub fn connection(&self, provider: Provider) -> Option<Zeroizing<Vec<u8>>> {
        self.get(provider).and_then(|e| e.connection.clone())
    }
    /// A new sign-in to `provider` (with its form for WebDAV and Nextcloud). The kept connection it replaces is returned,
    /// to revoke: nothing uses it any more.
    pub fn signed_in(&mut self, provider: Provider, form: Option<F>) -> Option<Zeroizing<Vec<u8>>> {
        let entry = self.entry(provider);
        entry.form = form;
        entry.connection.take()
    }
    /// Keeps `connection`, just taken out of the credential store, for `provider`.
    pub fn keep(&mut self, provider: Provider, connection: Zeroizing<Vec<u8>>) {
        self.entry(provider).connection = Some(connection);
    }
    /// The kept connection no longer works (the grant was revoked elsewhere): forget it, keeping the form.
    pub fn invalid(&mut self, provider: Provider) {
        if let Some(i) = self.entries.iter().position(|(p, _)| *p == provider) {
            self.entries[i].1.connection = None;
        }
    }
    /// The vault exists: forgets everything and returns the kept connections to revoke, except `in_use`'s, the service the
    /// vault was restored from (its kept connection is the same grant as the stored one).
    pub fn release(&mut self, in_use: Option<Provider>) -> Vec<Zeroizing<Vec<u8>>> {
        std::mem::take(&mut self.entries).into_iter().filter(|(p, _)| Some(*p) != in_use).filter_map(|(_, e)| e.connection).collect()
    }
}

/// Why a server address can't be used yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressError {
    /// Not a usable `http(s)://` address.
    Invalid,
    /// Plain HTTP: the user must confirm first.
    NeedsInsecureConsent,
}
/// The user's address, completed (`https://` for names, `http://` for IP addresses) and validated.
pub fn endpoint(address: &str, allow_http: bool) -> Result<cloudset::Endpoint, AddressError> {
    let completed = cloudset::Endpoint::complete_address(address);
    let insecure = completed.starts_with("http://");
    if insecure && !allow_http {
        return Err(AddressError::NeedsInsecureConsent);
    }
    let transport = if insecure { cloudset::Transport::AllowInsecureHttp } else { cloudset::Transport::RequireHttps };
    cloudset::Endpoint::parse(&completed, transport).map_err(|_| AddressError::Invalid)
}

/// Checks a new connection by reading its root, then stores it under a fresh credential ID. Returns the location to link.
pub fn connect(connection: Connection) -> Result<crate::sync::Location, SyncError> {
    let services = services()?;
    let pin = connection.certificate_pin();
    let client = Client::new(connection.clone(), ClientConfig::new(services.user_agent.clone()));
    run(&services, None, client.stat(RemotePath::root()), pin)?;
    let credential = uuid::Uuid::new_v4().to_string();
    services.credentials.save(&credential, &connection.to_bytes())?;
    let account = match connection.provider() {
        ProviderKind::WebDav | ProviderKind::Nextcloud => format!("{} · {}", connection.username(), connection.host()),
        _ => connection.username().to_owned(),
    };
    let pinned = connection.certificate_pin().is_some();
    Ok(crate::sync::Location::Cloud { provider: Provider::of(connection.provider()), account, credential, pinned })
}

/// A server certificate the user confirmed (its SHA-256), for requests to `server`'s origin.
fn pin_for(server: &cloudset::Endpoint, trusted: Option<[u8; 32]>) -> Result<Option<cloudset::CertificatePin>, SyncError> {
    trusted
        .map(|t| cloudset::PinnedCertificate::from_slice(&t).and_then(|c| cloudset::CertificatePin::new(server, c)))
        .transpose()
        .map_err(map_error)
}
/// A new WebDAV connection; `trusted` pins a server certificate the user confirmed (`https://` only).
pub fn webdav(
    endpoint: cloudset::Endpoint,
    username: String,
    password: String,
    trusted: Option<[u8; 32]>,
) -> Result<Connection, SyncError> {
    let connection = Connection::webdav(endpoint, username, password);
    match trusted {
        Some(t) => cloudset::PinnedCertificate::from_slice(&t).and_then(|c| connection.with_pinned_certificate(c)).map_err(map_error),
        None => Ok(connection),
    }
}
/// Nextcloud Login Flow v2: the page to open in the browser, and the poll state. `trusted` as in [`webdav`]: the polls
/// and the resulting connection pin it too.
pub fn nextcloud_begin(server: &cloudset::Endpoint, trusted: Option<[u8; 32]>) -> Result<cloudset::nextcloud::LoginFlow, SyncError> {
    let services = services()?;
    let pin = pin_for(server, trusted)?;
    run(&services, None, cloudset::nextcloud::begin_login(server, &ClientConfig::new(services.user_agent.clone())), pin)
}
/// `None` while the user has not finished in the browser.
pub fn nextcloud_poll(
    server: &cloudset::Endpoint,
    poll: &cloudset::nextcloud::LoginPoll,
    trusted: Option<[u8; 32]>,
) -> Result<Option<Connection>, SyncError> {
    let services = services()?;
    let pin = pin_for(server, trusted)?;
    let connection = run(&services, None, cloudset::nextcloud::poll_login(poll, &ClientConfig::new(services.user_agent.clone())), pin)?;
    match (connection, trusted) {
        (Some(connection), Some(t)) => {
            cloudset::PinnedCertificate::from_slice(&t).and_then(|c| connection.with_pinned_certificate(c)).map(Some).map_err(map_error)
        }
        (connection, _) => Ok(connection),
    }
}

/// Starts an OAuth sign-in (PKCE, public client) with 64 bytes from the system CSPRNG.
pub fn oauth_begin(
    provider: Provider,
    access: cloudset::AccessScope,
    client: cloudset::OAuthClient,
) -> Result<cloudset::Authorization, SyncError> {
    let kind = match provider {
        Provider::Dropbox => ProviderKind::Dropbox,
        Provider::Onedrive => ProviderKind::OneDrive,
        Provider::GoogleDrive => ProviderKind::GoogleDrive,
        _ => return Err(SyncError::Protocol),
    };
    let mut random = Zeroizing::new([0u8; 64]);
    getrandom::fill(random.as_mut()).map_err(|_| SyncError::Storage)?;
    cloudset::oauth::begin(kind, access, client, &random).map_err(map_error)
}
/// Exchanges the callback for tokens (the core checks the URL and `state` first), then checks and stores the connection.
pub fn oauth_complete(authorization: &cloudset::Authorization, callback: &str) -> Result<crate::sync::Location, SyncError> {
    let services = services()?;
    let operation = authorization.complete(callback, &ClientConfig::new(services.user_agent.clone())).map_err(map_error)?;
    let connection = run(&services, None, operation, None)?;
    connect(connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(b: u8) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(vec![b])
    }

    #[test]
    fn restore_sign_ins_are_kept_per_service_until_the_vault_exists() {
        let mut kept = RestoreSignIns::<&str>::default();
        // Signed in to Nextcloud; its connection is stored elsewhere while it is the restore location.
        assert_eq!(kept.signed_in(Provider::Nextcloud, Some("https://cloud.example")), None);
        assert_eq!(kept.form(Provider::Nextcloud), Some("https://cloud.example"));
        assert_eq!(kept.connection(Provider::Nextcloud), None);
        // No backup there: the connection comes back here instead of being revoked.
        kept.keep(Provider::Nextcloud, bytes(1));
        assert_eq!(kept.connection(Provider::Nextcloud), Some(bytes(1)));
        // Another service is independent.
        kept.signed_in(Provider::Dropbox, None);
        kept.keep(Provider::Dropbox, bytes(2));
        // A new Nextcloud sign-in replaces the kept one, handed back for revoking.
        assert_eq!(kept.signed_in(Provider::Nextcloud, Some("https://other.example")), Some(bytes(1)));
        assert_eq!(kept.connection(Provider::Nextcloud), None);
        kept.keep(Provider::Nextcloud, bytes(3));
        // A kept connection that stopped working is dropped, the form stays for the new sign-in.
        kept.invalid(Provider::Nextcloud);
        assert_eq!((kept.connection(Provider::Nextcloud), kept.form(Provider::Nextcloud)), (None, Some("https://other.example")));
        kept.keep(Provider::Nextcloud, bytes(4));
        // Restored from Dropbox: its grant is in use; the rest is revoked, and nothing is kept any more.
        assert_eq!(kept.release(Some(Provider::Dropbox)), vec![bytes(4)]);
        assert_eq!((kept.connection(Provider::Dropbox), kept.form(Provider::Nextcloud)), (None, None));
        assert!(kept.release(None).is_empty());
    }
}
