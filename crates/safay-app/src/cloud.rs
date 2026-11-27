//! Cloud storage backup (SECURITY.md, Network): `cloudset` builds every request; this module only runs its state machines
//! with an injected HTTP executor and keeps the serialized connection in an injected credential store (the system
//! keychain in the app, memory in tests). Remote files are the folder backend's, `vault.safayvault` and
//! `vault.metadata.json`, only ciphertext and non-secret metadata: at the root of Safay's app folder, or in the folder the
//! user picked on a WebDAV server, a Nextcloud account or a Synology NAS ([`backup_folder`]).
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
    Synology,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Webdav => "WebDAV",
            Provider::Nextcloud => "Nextcloud",
            Provider::Dropbox => "Dropbox",
            Provider::Onedrive => "OneDrive",
            Provider::GoogleDrive => "Google Drive",
            Provider::Synology => "Synology",
        }
    }
    /// The services in the order the location lists show them.
    pub const LISTED: [Provider; 6] =
        [Provider::Dropbox, Provider::Onedrive, Provider::GoogleDrive, Provider::Nextcloud, Provider::Synology, Provider::Webdav];
    /// A server of the user's own, set up with its address: WebDAV, Nextcloud and Synology.
    pub fn has_server(self) -> bool {
        matches!(self, Provider::Webdav | Provider::Nextcloud | Provider::Synology)
    }
    /// The location kind for a connection's provider; `None` for a provider this app does not offer.
    pub fn of(kind: ProviderKind) -> Option<Self> {
        Some(match kind {
            ProviderKind::WebDav => Provider::Webdav,
            ProviderKind::Nextcloud => Provider::Nextcloud,
            ProviderKind::Dropbox => Provider::Dropbox,
            ProviderKind::OneDrive => Provider::Onedrive,
            ProviderKind::GoogleDrive => Provider::GoogleDrive,
            ProviderKind::Synology => Provider::Synology,
            _ => return None,
        })
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

/// Sends one request as is: no redirects, no cookies, no cache, at most the configured body limit plus one byte.
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
    operation: BoxOperation<T>,
    pin: Option<cloudset::CertificatePin>,
) -> Result<T, SyncError> {
    drive(services, operation, pin, |connection| {
        if let Some(id) = credential {
            services.credentials.save(id, &connection.to_bytes())?;
        }
        Ok(())
    })
}

fn drive<T>(
    services: &Services,
    mut operation: BoxOperation<T>,
    pin: Option<cloudset::CertificatePin>,
    mut refreshed: impl FnMut(Connection) -> Result<(), SyncError>,
) -> Result<T, SyncError> {
    let mut response = None;
    loop {
        let step = operation.advance(response.take());
        if let Some(connection) = operation.take_connection() {
            refreshed(connection)?;
        }
        match step.map_err(map_error)? {
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
                if let Some(connection) = done.connection {
                    refreshed(connection)?;
                }
                return Ok(done.value);
            }
        }
    }
}

pub fn map_error(e: CloudError) -> SyncError {
    match e {
        CloudError::ReauthRequired | CloudError::AuthorizationDenied | CloudError::InvalidConnection => SyncError::SignIn,
        CloudError::OtpRequired => SyncError::OtpRequired,
        CloudError::Forbidden => SyncError::Forbidden,
        CloudError::NotFound => SyncError::NoRemote,
        CloudError::PreconditionFailed => SyncError::RemoteChanged,
        CloudError::UnsupportedPrecondition => SyncError::ManualWriteRequired,
        CloudError::CommitUnconfirmed => SyncError::CommitUnconfirmed,
        CloudError::QuotaExceeded => SyncError::Quota,
        CloudError::RateLimited { .. } => SyncError::RateLimited,
        CloudError::Server { status } => SyncError::Server(status),
        CloudError::TooLarge => SyncError::TooLarge,
        _ => SyncError::Protocol,
    }
}

/// The version a sync compares: the provider's revision, or for Synology, which has none, the modification time (a write
/// there can't be checked against it, so a later upload waits for the user, as for any unchecked write).
fn version(entry: &cloudset::Entry) -> Option<RemoteVersion> {
    match (&entry.revision, entry.modified_at) {
        (Some(r), _) => Some(RemoteVersion::Revision { value: r.as_str().to_owned() }),
        (None, Some(at)) => Some(RemoteVersion::ModifiedAt { at }),
        (None, None) => None,
    }
}

pub struct CloudBackend {
    credential: String,
    /// WebDAV and Nextcloud: the folder picked for the backup (`Location::Cloud`'s `folder`).
    folder: Option<String>,
}
impl CloudBackend {
    pub fn new(credential: String, folder: Option<String>) -> Self {
        Self { credential, folder }
    }
    fn client(&self, services: &Services) -> Result<Client, SyncError> {
        let blob = services.credentials.load(&self.credential).ok_or(SyncError::SignIn)?;
        let connection = Connection::from_bytes(&blob).map_err(|_| SyncError::SignIn)?;
        let limits = cloudset::Limits::default().with_max_body_bytes(crate::storage::MAX_FILE_BYTES as usize);
        Ok(Client::new(connection, ClientConfig::new(services.user_agent.clone()).with_limits(limits)))
    }

    // Each operation starts with the connection persisted by the previous one.
    fn file_operation<T>(&self, services: &Services, make: impl FnOnce(&Client, &RemotePath) -> BoxOperation<T>) -> Result<T, SyncError> {
        let client = self.client(services)?;
        let folder = backup_folder(client.connection(), self.folder.as_deref())?;
        run(services, Some(&self.credential), make(&client, &folder), client.connection().certificate_pin())
    }
    /// Moves a QuickConnect connection to the address that answers now and stores it. `false` when the connection has no
    /// QuickConnect ID or already uses that address.
    fn relocate(&self, services: &Services) -> Result<bool, SyncError> {
        let client = self.client(services)?;
        let Some((id, region)) = client.connection().quickconnect() else { return Ok(false) };
        let endpoint = synology_find(id, region)?.1;
        if client.connection().endpoint() == Some(&endpoint) {
            return Ok(false);
        }
        let moved = client.connection().clone().with_endpoint(endpoint).map_err(map_error)?;
        services.credentials.save(&self.credential, &moved.to_bytes())?;
        Ok(true)
    }
}
/// A failure that a QuickConnect lookup may get past: no answer, or a gateway (a relay) with nothing behind it.
fn unreachable(e: SyncError) -> bool {
    matches!(e, SyncError::Network | SyncError::Server(502..=504))
}
/// The folder that holds the backup. On Dropbox, OneDrive and Google Drive the sign-in is to Safay's app folder
/// ("Apps › Safay", or the folder named for it in My Drive), so the backup lives at its root. A Synology connection's
/// root is the folder the user picked for it. On WebDAV and Nextcloud it is the folder the user picked (`picked`, the
/// location's `folder`); a location set up before folders could be picked keeps a `Safay` folder at the top.
fn backup_folder(connection: &Connection, picked: Option<&str>) -> Result<RemotePath, SyncError> {
    Ok(match (connection.provider(), connection.access_scope()) {
        (ProviderKind::Synology, _) | (_, Some(cloudset::AccessScope::AppFolder)) => RemotePath::root(),
        // Never the top instead of a folder that no longer parses.
        _ => match picked {
            Some(folder) => RemotePath::parse(folder).map_err(|_| SyncError::FolderUnavailable)?,
            None => RemotePath::parse(DEFAULT_FOLDER).expect("static path"),
        },
    })
}
fn path(folder: &RemotePath, name: &str) -> RemotePath {
    folder.join(name).expect("static file name")
}
impl Backend for CloudBackend {
    fn read(&self) -> Result<Option<Remote>, SyncError> {
        let services = services()?;
        let read = || self.file_operation(&services, |client, folder| client.read(path(folder, VAULT_FILE_NAME)));
        // Every sync starts here: a NAS found through QuickConnect that stopped answering (the device left the network it
        // signed in on) is looked up again, and the sync goes on at the address that answers now.
        let file = match read() {
            // A lookup that fails too leaves the first error: "no backup there" would be wrong.
            Err(e) if unreachable(e) && self.relocate(&services).unwrap_or(false) => read(),
            result => result,
        };
        let Some(file) = file? else {
            return Ok(None);
        };
        if !vault::is_valid_envelope(&file.data) {
            return Err(SyncError::Corrupt);
        }
        let device = self
            .file_operation(&services, |client, folder| client.read(path(folder, METADATA_FILE_NAME)))
            .ok()
            .flatten()
            .and_then(|m| BackupMetadata::device_name(&m.data));
        let version = version(&file.entry);
        Ok(Some(Remote { data: file.data, version, device }))
    }
    fn write(&self, data: &[u8], metadata: &BackupMetadata, expect: &Expect) -> Result<Option<RemoteVersion>, SyncError> {
        if data.len() as u64 > crate::storage::MAX_FILE_BYTES {
            return Err(SyncError::TooLarge);
        }
        let services = services()?;
        let wanted = match expect {
            Expect::Any => Some(Precondition::None),
            Expect::Absent => Some(Precondition::Absent),
            Expect::Unchanged(RemoteVersion::Revision { value }) => Some(Precondition::Matches(Revision::new(value.clone()))),
            Expect::Unchanged(RemoteVersion::ModifiedAt { .. }) => None,
        };
        // Where the provider can't hold the write to the condition (Google Drive, Synology, WebDAV with weak ETags,
        // OneDrive upload sessions), the condition is checked just before the upload and the result read back after it:
        // a change from another device found either way is reported as one, and the sync compares again.
        let client = self.client(&services)?;
        let checked = match &wanted {
            Some(p) => !client.supports_write_precondition(data.len(), p),
            None => true,
        };
        let precondition = if checked { Precondition::None } else { wanted.unwrap_or(Precondition::None) };
        if checked {
            let current = self.file_operation(&services, |client, folder| client.stat(path(folder, VAULT_FILE_NAME)));
            let unchanged = match (expect, current) {
                (Expect::Absent, Ok(None) | Err(SyncError::NoRemote)) => true,
                (Expect::Unchanged(expected), Ok(Some(entry))) => version(&entry).as_ref() == Some(expected),
                (_, Err(e)) => return Err(e),
                _ => false,
            };
            if !unchanged {
                return Err(SyncError::RemoteChanged);
            }
        }
        let upload = || {
            self.file_operation(&services, |client, folder| {
                client.write(path(folder, VAULT_FILE_NAME), data.to_vec(), precondition.clone())
            })
        };
        let result = match upload() {
            Err(SyncError::NoRemote) => {
                let folder = backup_folder(client.connection(), self.folder.as_deref())?;
                if folder.is_root() {
                    return Err(SyncError::NoRemote);
                }
                let client = self.client(&services)?;
                run(&services, Some(&self.credential), client.create_dir(folder), client.connection().certificate_pin())?;
                upload()
            }
            result => result,
        };
        let read_back = || self.file_operation(&services, |client, folder| client.read(path(folder, VAULT_FILE_NAME)));
        let version = match result {
            Ok(entry) if !checked => version(&entry),
            // Another device's upload right after this one would show here: compare again rather than record it.
            Ok(_) => match read_back() {
                Ok(Some(file)) if file.data == data => version(&file.entry),
                Ok(_) => return Err(SyncError::RemoteChanged),
                Err(_) => return Err(SyncError::CommitUnconfirmed),
            },
            Err(SyncError::CommitUnconfirmed | SyncError::Network) => match read_back() {
                Ok(Some(file)) if file.data == data => version(&file.entry),
                _ => return Err(SyncError::CommitUnconfirmed),
            },
            Err(e) => return Err(e),
        };
        let _ = self.file_operation(&services, |client, folder| {
            client.write(path(folder, METADATA_FILE_NAME), metadata.encode(), Precondition::None)
        });
        Ok(version)
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
/// A connection kept from an earlier sign-in, held in memory again to pick a folder on ([`list_folder`]); nothing is
/// stored.
pub fn held(connection: &[u8]) -> Result<Connection, SyncError> {
    Connection::from_bytes(connection).map_err(map_error)
}

/// The cloud sign-ins made during first-run restore, one per service, so choosing the service again needs no new
/// sign-in: after "no backup found", an error, or going back from the password step. Memory only (never the keychain or
/// disk) and gone when the app quits. For WebDAV and Nextcloud the form submitted (`F`, with the WebDAV password) is kept
/// too, to prefill it. Every sign-in to a service is to the same app folder, so one per service is enough.
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
    connect_in(connection, None)
}
/// [`connect`], the backup in `folder` (WebDAV and Nextcloud: the folder picked; `None` keeps the fixed one).
fn connect_in(mut connection: Connection, folder: Option<RemotePath>) -> Result<crate::sync::Location, SyncError> {
    let provider = Provider::of(connection.provider()).ok_or(SyncError::Protocol)?;
    let services = services()?;
    let pin = connection.certificate_pin();
    let client = Client::new(connection.clone(), ClientConfig::new(services.user_agent.clone()));
    drive(&services, client.stat(RemotePath::root()), pin, |latest| {
        connection = latest;
        Ok(())
    })?;
    let credential = uuid::Uuid::new_v4().to_string();
    services.credentials.save(&credential, &connection.to_bytes())?;
    let account = match connection.provider() {
        ProviderKind::WebDav | ProviderKind::Nextcloud => format!("{} · {}", connection.account(), connection.host()),
        // The QuickConnect ID stays the same when the address changes (another network, the relay).
        ProviderKind::Synology => match (connection.quickconnect(), connection.root_folder()) {
            (Some((id, _)), Some(root)) => format!("{} · {} › {}", connection.account(), id, root),
            (None, Some(root)) => format!("{} · {} › {}", connection.account(), connection.host(), root),
            _ => format!("{} · {}", connection.account(), connection.host()),
        },
        _ => connection.account().to_owned(),
    };
    // The folder picked shows after the account, as a Synology NAS's does.
    let account = match folder.as_ref().filter(|f| !f.is_root()) {
        Some(folder) => format!("{account} › {}", folder.segments().join(" › ")),
        None => account,
    };
    let pinned = connection.certificate_pin().is_some();
    let folder = folder.map(|f| f.segments().join("/"));
    Ok(crate::sync::Location::Cloud { provider, account, credential, pinned, folder })
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

/// What the user typed for a Synology NAS: a QuickConnect ID (bare, or a `quickconnect.to` / `quickconnect.cn` address,
/// whose region it names), or the address of the NAS itself.
#[derive(Clone, Debug)]
pub enum SynologyAddress {
    QuickConnect(cloudset::synology::quickconnect::QuickConnectId, Option<cloudset::synology::quickconnect::Region>),
    Direct(cloudset::Endpoint),
}
pub fn synology_address(input: &str, allow_http: bool) -> Result<SynologyAddress, AddressError> {
    use cloudset::synology::quickconnect::{QuickConnectId, Region};
    if let Ok(id) = QuickConnectId::parse(input) {
        return Ok(SynologyAddress::QuickConnect(id, Region::from_address(input)));
    }
    endpoint(input, allow_http).map(SynologyAddress::Direct)
}

/// How long the addresses of a NAS get to answer, all at once.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Finds the NAS with a QuickConnect ID: asks the control host of `region` (the other region's when that one can't be
/// reached), probes every address it lists at once, and opens Synology's relay when none answers. Returns the region
/// that knew the NAS and the address to use. [`SyncError::NoRemote`]: no NAS has this ID; [`SyncError::Network`]: it
/// is offline.
pub fn synology_find(
    id: &cloudset::synology::quickconnect::QuickConnectId,
    region: cloudset::synology::quickconnect::Region,
) -> Result<(cloudset::synology::quickconnect::Region, cloudset::Endpoint), SyncError> {
    use cloudset::synology::quickconnect;
    let services = services()?;
    let config = ClientConfig::new(services.user_agent.clone());
    let resolve = |region| run(&services, None, quickconnect::resolve(id, region, &config), None);
    let resolution = match resolve(region) {
        Err(SyncError::Network | SyncError::Protocol) => resolve(region.other())?,
        result => result?,
    };
    if let Some(endpoint) = first_answering(&services, &resolution, resolution.candidates(), &config) {
        return Ok((resolution.region(), endpoint));
    }
    let relays = run(&services, None, resolution.request_relay(&config), None)?;
    // The tunnel can take a moment to come up.
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        if let Some(endpoint) = first_answering(&services, &resolution, &relays, &config) {
            return Ok((resolution.region(), endpoint));
        }
    }
    Err(SyncError::Network)
}

/// Probes the candidates at once and returns the first, in list order, that answered as this NAS within
/// [`PROBE_TIMEOUT`]. A transport failure counts as no answer; a probe still running afterwards is left to finish.
fn first_answering(
    services: &Services,
    resolution: &cloudset::synology::quickconnect::Resolution,
    candidates: &[cloudset::synology::quickconnect::Candidate],
    config: &ClientConfig,
) -> Option<cloudset::Endpoint> {
    if candidates.is_empty() {
        return None;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    for (i, candidate) in candidates.iter().enumerate() {
        let (sender, services, resolution, config) = (sender.clone(), services.clone(), resolution.clone(), config.clone());
        let endpoint = candidate.endpoint.clone();
        std::thread::spawn(move || {
            let answered = run(&services, None, resolution.probe(&endpoint, &config), None).unwrap_or(false);
            let _ = sender.send((i, answered));
        });
    }
    drop(sender);
    let deadline = std::time::Instant::now() + PROBE_TIMEOUT;
    let mut answers: Vec<Option<bool>> = vec![None; candidates.len()];
    loop {
        // The first candidate that answered, once every earlier one has failed.
        if let Some(i) = answers.iter().position(|a| *a != Some(false))
            && answers[i] == Some(true)
        {
            return Some(candidates[i].endpoint.clone());
        }
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match receiver.recv_timeout(left) {
            Ok((i, answered)) => answers[i] = Some(answered),
            // Out of time, or every probe is done: the earliest that answered.
            Err(_) => return answers.iter().position(|a| *a == Some(true)).map(|i| candidates[i].endpoint.clone()),
        }
    }
}

/// What a Synology sign-in needs. `Debug` is deliberately absent: it holds the password.
pub struct SynologyLogin {
    pub endpoint: cloudset::Endpoint,
    /// The QuickConnect ID the address was found with, and the region that knew it.
    pub quickconnect: Option<(cloudset::synology::quickconnect::QuickConnectId, cloudset::synology::quickconnect::Region)>,
    pub username: String,
    pub password: Zeroizing<String>,
    /// The code from the authenticator app, after a sign-in reported [`SyncError::OtpRequired`].
    pub otp_code: Option<String>,
    /// A certificate the user confirmed (`https://` addresses typed by the user only).
    pub trusted: Option<[u8; 32]>,
}
/// Signs in to DSM, to the whole NAS: the user picks the folder next ([`list_folder`], [`use_folder`]).
/// [`SyncError::OtpRequired`]: the account uses two-step verification and needs a code (or the one given was wrong);
/// [`SyncError::SignIn`]: wrong user name or password; [`SyncError::Forbidden`]: the account can't use File Station.
/// The connection is kept in memory only until a folder is picked.
pub fn synology_sign_in(login: SynologyLogin) -> Result<Connection, SyncError> {
    let services = services()?;
    let pin = pin_for(&login.endpoint, login.trusted)?;
    let mut request =
        cloudset::synology::SignIn::new(login.endpoint, RemotePath::root(), login.username, login.password.as_str().to_owned());
    if let Some(code) = login.otp_code {
        request = request.with_otp_code(code);
    }
    if let Some((id, region)) = login.quickconnect {
        request = request.with_quickconnect(id, region);
    }
    let connection = run(&services, None, cloudset::synology::sign_in(request, &ClientConfig::new(services.user_agent.clone())), pin)?;
    match login.trusted {
        Some(t) => cloudset::PinnedCertificate::from_slice(&t).and_then(|c| connection.with_pinned_certificate(c)).map_err(map_error),
        None => Ok(connection),
    }
}

/// A folder of the server while the user picks one: its subfolders, and whether it can hold the backup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderListing {
    /// Names of the subfolders, sorted.
    pub folders: Vec<String>,
    /// A Safay backup is already here.
    pub has_backup: bool,
    /// When that backup was last written (Unix milliseconds), if the server says.
    pub backup_saved_ms: Option<i64>,
    /// Nothing is here but system items (names starting with `.`, `#` or `@`, such as `#recycle` and `@eaDir`).
    pub empty: bool,
}
impl FolderListing {
    /// Whether a backup may go here: an empty folder, or one that already holds a Safay backup.
    pub fn usable(&self) -> bool {
        self.empty || self.has_backup
    }
}
/// Runs an operation on a connection held in memory, keeping a renewed session in `connection`.
fn run_held<T>(connection: &mut Connection, make: impl FnOnce(&Client) -> BoxOperation<T>) -> Result<T, SyncError> {
    let services = services()?;
    let pin = connection.certificate_pin();
    let client = Client::new(connection.clone(), ClientConfig::new(services.user_agent.clone()));
    drive(&services, make(&client), pin, |latest| {
        *connection = latest;
        Ok(())
    })
}
/// Lists a folder of the server (a Synology NAS's top level: its shared folders), on a connection held while the user
/// picks the backup's folder (WebDAV, Nextcloud, Synology). Listing the top first also checks the sign-in.
pub fn list_folder(connection: &mut Connection, folder: &RemotePath) -> Result<FolderListing, SyncError> {
    let entries = run_held(connection, |client| client.list(folder.clone()))?;
    let system = |name: &str| name.starts_with(['.', '#', '@']);
    let name = |e: &cloudset::Entry| e.path.name().unwrap_or_default().to_owned();
    let mut folders: Vec<String> = entries.iter().filter(|e| e.is_dir).map(name).filter(|n| !system(n)).collect();
    folders.sort_by_key(|n| n.to_lowercase());
    let backup = entries.iter().find(|e| !e.is_dir && e.path.name() == Some(VAULT_FILE_NAME));
    Ok(FolderListing {
        folders,
        has_backup: backup.is_some(),
        backup_saved_ms: backup.and_then(|e| e.modified_at).map(crate::sync::to_ms),
        empty: entries.iter().all(|e| system(&name(e))),
    })
}
/// Creates a folder on the server (on a Synology NAS not at its top level: shared folders are made in DSM).
pub fn create_folder(connection: &mut Connection, folder: &RemotePath) -> Result<(), SyncError> {
    run_held(connection, |client| client.create_dir(folder.clone()))
}
/// Uses `folder` for the backup: checks and stores the connection, as [`connect`] does. Synology: the connection moves
/// there, its session carried over, so no new sign-in or code is needed (not its top level, the shared folders). WebDAV
/// and Nextcloud: the location keeps the folder, the top included.
pub fn use_folder(connection: Connection, folder: RemotePath) -> Result<crate::sync::Location, SyncError> {
    if connection.provider() != ProviderKind::Synology {
        return connect_in(connection, Some(folder));
    }
    if folder.is_root() {
        return Err(SyncError::Forbidden);
    }
    connect(connection.with_root_folder(folder).map_err(map_error)?)
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
