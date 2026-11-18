//! App self-update through NSH's `nsh-updater`: a signed manifest (`latest.json` + `.sig`), a
//! downloaded archive checked against its SHA-256 and size, and the installed `.app` swapped in place. NSH's
//! `nsh/updater` JS module is for TSX apps; a Rust app uses the crate and brings its own HTTP (`docs/dev/updater.md`).
//!
//! The source and the key come from the package, never from code: `scripts/package.py` writes
//! `Contents/Resources/nsh/assets/nsh-updater.json` (`url`, `publicKey`, id, version) and `nsh/app.json`, which
//! `Installation::current` requires; development builds have neither, so nothing here runs. Requests go only to that
//! URL and the archive it names (HTTPS, redirects HTTPS only, size limits); nothing about the vault is sent.
//!
//! One updater per process: the main window and Settings read [`state`] and call the actions; work runs on its own
//! threads. A verified download is unpacked and checked beside the app at once (`Installation::stage`, which includes
//! the new app's code signature), so a refused update is reported while the app can still say so. The staged app then
//! takes the installed one's place when the app quits (`install_on_quit`) or at once with [`Updater::install`]
//! followed by a restart; either way only two renames are left (`Staged::finish`).
use nsh_updater::{
    Available, Config, Download,
    install::{Installation, Staged},
};
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

/// A manifest or signature larger than this is refused.
const MAX_MANIFEST: u64 = 1024 * 1024;
/// An archive larger than this is refused even if the manifest says otherwise.
const MAX_ARCHIVE: u64 = 1024 * 1024 * 1024;
/// Where the updater is, as Settings shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A newer version; `notes` is the release's text.
    Available {
        version: String,
        notes: Option<String>,
    },
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    /// Downloaded, verified and staged: installs when the app quits, or now.
    Ready {
        version: String,
    },
    Installing,
    Failed(Failure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The server could not be reached.
    Network,
    /// The manifest or the archive failed its checks (signature, id, hash, size).
    Invalid,
    /// The app could not be replaced (permissions, the new app's id or signature).
    Install,
}

/// Fetches what the updater needs. The app's is HTTPS through `ureq`; tests use memory.
pub trait Fetch: Send + Sync {
    /// The whole body, at most `limit` bytes.
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, Failure>;
    /// The body into `sink`, at most `limit` bytes.
    fn stream(&self, url: &str, limit: u64, sink: &mut dyn FnMut(&[u8]) -> Result<(), Failure>) -> Result<(), Failure>;
}

/// One updater: its configuration, where downloads go, and its state.
pub struct Updater {
    config: Config,
    installation: Option<Installation>,
    downloads: PathBuf,
    fetch: Arc<dyn Fetch>,
    inner: Mutex<Inner>,
}
#[derive(Default)]
struct Inner {
    phase: Phase,
    available: Option<Available>,
    /// The downloaded update, unpacked and checked beside the app; `None` in builds without an installation.
    staged: Option<Staged>,
    busy: bool,
}

impl Updater {
    pub fn new(config: Config, installation: Option<Installation>, downloads: PathBuf, fetch: Arc<dyn Fetch>) -> Self {
        Self { config, installation, downloads, fetch, inner: Mutex::new(Inner::default()) }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn phase(&self) -> Phase {
        self.lock().phase.clone()
    }
    fn set(&self, phase: Phase) {
        self.lock().phase = phase;
    }
    /// Claims the updater for one operation; `false` while another runs.
    fn begin(&self) -> bool {
        let mut inner = self.lock();
        if inner.busy {
            return false;
        }
        inner.busy = true;
        true
    }
    fn end(&self) {
        self.lock().busy = false;
    }

    /// Reads the manifest and its signature and decides. Blocking.
    pub fn check(&self) -> Phase {
        if !self.begin() {
            return self.phase();
        }
        // A verified download of the same version stays ready.
        if matches!(self.phase(), Phase::Ready { .. }) {
            self.end();
            return self.phase();
        }
        self.set(Phase::Checking);
        let result = (|| {
            let manifest = self.fetch.get(&self.config.manifest_url(), MAX_MANIFEST)?;
            let signature = match self.config.signature_url() {
                Some(url) => Some(self.fetch.get(&url, MAX_MANIFEST)?),
                None => None,
            };
            self.config.check(&manifest, signature.as_deref(), &nsh_updater::platform()).map_err(|_| Failure::Invalid)
        })();
        let phase = match result {
            Ok(Some(available)) => {
                let phase = Phase::Available { version: available.version.clone(), notes: available.notes.clone() };
                self.lock().available = Some(available);
                phase
            }
            Ok(None) => Phase::UpToDate,
            Err(f) => Phase::Failed(f),
        };
        self.set(phase.clone());
        self.end();
        phase
    }

    /// Downloads the available version into an exclusively created file, checks it and stages it beside the app.
    /// Blocking.
    pub fn download(&self) -> Phase {
        let Some(available) = self.lock().available.clone() else { return self.phase() };
        // A manifest entry without a hash is a page to visit, not an archive (the app opens it instead).
        let Some(sha256) = available.sha256.clone() else { return self.phase() };
        if !self.begin() {
            return self.phase();
        }
        let total = available.size;
        self.set(Phase::Downloading { received: 0, total });
        let result = (|| {
            fs::create_dir_all(&self.downloads).map_err(|_| Failure::Install)?;
            let name = available.url.rsplit('/').next().filter(|n| !n.is_empty() && !n.contains("..")).unwrap_or("update.tar.gz");
            let path = self.downloads.join(format!("{}-{name}", available.version));
            let _ = fs::remove_file(&path);
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|_| Failure::Install)?;
            let mut check = Download::new(&sha256, total);
            let limit = total.unwrap_or(MAX_ARCHIVE).min(MAX_ARCHIVE);
            let streamed = self.fetch.stream(&available.url, limit, &mut |chunk| {
                let received = check.update(chunk).map_err(|_| Failure::Invalid)?;
                file.write_all(chunk).map_err(|_| Failure::Install)?;
                self.set(Phase::Downloading { received, total });
                Ok(())
            });
            let verified = streamed.and_then(|_| check.finish().map_err(|_| Failure::Invalid));
            if let Err(f) = verified {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(f);
            }
            file.sync_all().map_err(|_| Failure::Install)?;
            drop(file);
            // Unpacked and checked now; the archive is not needed after that. Without an installation (a development
            // build) there is nothing to stage, and installing fails later.
            let staged = self.installation.as_ref().map(|i| i.stage(&path).map_err(|_| Failure::Install)).transpose();
            let _ = fs::remove_file(&path);
            staged
        })();
        let phase = match result {
            Ok(staged) => {
                self.lock().staged = staged;
                Phase::Ready { version: available.version.clone() }
            }
            Err(f) => Phase::Failed(f),
        };
        self.set(phase.clone());
        self.end();
        phase
    }

    /// Swaps the installed app for the downloaded one; on `Ok` the caller restarts (`nsh_core::element::desktop::restart`),
    /// which starts the new version once this process has exited. The old app stays if the swap fails.
    pub fn install(&self) -> Result<(), Failure> {
        if !self.begin() {
            return Err(Failure::Install);
        }
        // Taken out: the quit that follows must not install it a second time.
        let Some(staged) = self.lock().staged.take() else {
            self.end();
            return Err(Failure::Install);
        };
        self.set(Phase::Installing);
        // The restart starts the app at the path the user knows: after the swap this process' executable is the old
        // one, moved aside.
        nsh_core::restart::remember_program();
        let result = staged.finish().map_err(|_| Failure::Install);
        if let Err(f) = result {
            self.set(Phase::Failed(f));
        }
        self.end();
        result
    }

    /// The notes of the version last offered (the one `Ready` names once it is downloaded).
    pub fn offered_notes(&self) -> Option<String> {
        self.lock().available.as_ref().and_then(|a| a.notes.clone())
    }

    /// The app is quitting: a downloaded update takes its place now, for the next launch.
    pub fn install_on_quit(&self) {
        if self.begin()
            && let Some(staged) = self.lock().staged.take()
        {
            let _ = staged.finish();
        }
    }
}

// ------------------------------------------------------------------ the app's updater

/// The package's updater configuration: the file bytes and their parsed form, `None` in builds without one.
fn package_config() -> Option<(Vec<u8>, Config)> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    #[cfg(target_os = "macos")]
    let file = dir.join("../Resources/nsh/assets").join(nsh_updater::CONFIG_ASSET);
    #[cfg(not(target_os = "macos"))]
    let file = dir.join("nsh/assets").join(nsh_updater::CONFIG_ASSET);
    let bytes = fs::read(file).ok()?;
    let config = Config::parse(&bytes).ok()?;
    Some((bytes, config))
}

/// The app's updater; `None` in builds without an update source (development, other desktops for now).
pub fn updater() -> Option<&'static Updater> {
    static UPDATER: OnceLock<Option<Updater>> = OnceLock::new();
    UPDATER
        .get_or_init(|| {
            if !super::supports::UPDATES {
                return None;
            }
            let (bytes, config) = package_config()?;
            let installation = Installation::current(&config.id, &bytes);
            if let Some(i) = &installation {
                i.remove_leftovers();
            }
            let downloads = super::data_directory()?.join("updates");
            // Unfinished or stale downloads from an earlier run are never installed.
            let _ = fs::remove_dir_all(&downloads);
            Some(Updater::new(config, installation, downloads, Arc::new(Https::new())))
        })
        .as_ref()
}

/// The running version, as the package's updater configuration says.
pub fn current_version() -> Option<String> {
    updater().map(|u| u.config.version.clone())
}

/// `--update-check` (debug builds): check, download and install without launching, printing each phase. No vault is
/// opened and no UI is shown.
#[cfg(debug_assertions)]
pub fn command_line_check() -> i32 {
    let Some(u) = updater() else {
        println!("this build has no update source");
        return 1;
    };
    println!("installation: {}", if u.installation.is_some() { "found" } else { "none" });
    println!("check: {:?}", u.check());
    let downloaded = u.download();
    println!("download: {downloaded:?}");
    if !matches!(downloaded, Phase::Ready { .. }) {
        return 1;
    }
    let Some(staged) = u.lock().staged.take() else { return 1 };
    match staged.finish() {
        Ok(()) => {
            println!("install: replaced");
            0
        }
        Err(e) => {
            println!("install: {e}");
            1
        }
    }
}

/// Debug builds without an update source: the phase of a simulated download, so the update window's progress and
/// "Install and Relaunch" can be seen (`SAFAY_PREVIEW_UPDATE`).
#[cfg(debug_assertions)]
static PREVIEW: Mutex<Option<Phase>> = Mutex::new(None);
#[cfg(debug_assertions)]
pub fn preview_phase() -> Option<Phase> {
    PREVIEW.lock().ok()?.clone()
}
/// Simulates downloading `version` over about 20 seconds, then reports it ready.
#[cfg(debug_assertions)]
pub fn preview_download(version: String) {
    std::thread::spawn(move || {
        let set = |phase| *PREVIEW.lock().unwrap_or_else(|e| e.into_inner()) = Some(phase);
        let total = 18_400_000;
        for step in 0..=100u64 {
            set(Phase::Downloading { received: total * step / 100, total: Some(total) });
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        set(Phase::Ready { version });
    });
}

/// The app has quit (`main` after NSH's runner returns): a downloaded update replaces it for the next launch.
pub fn install_on_quit() {
    if let Some(u) = updater() {
        u.install_on_quit();
    }
}

/// HTTPS through `ureq` with the system's certificate verification; redirects followed only to HTTPS (GitHub's
/// `releases/latest/download` redirects to its file host). No cookies, no cache.
struct Https {
    agent: ureq::Agent,
}
impl Https {
    fn new() -> Self {
        let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
        // Release builds speak HTTPS only; debug builds may also reach this machine (`nsh-updater` accepts plain HTTP
        // to loopback only), for a local end-to-end test.
        let config = ureq::Agent::config_builder()
            .https_only(!cfg!(debug_assertions))
            .max_redirects(5)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_body(Some(Duration::from_secs(120)))
            .user_agent(format!("Safay/{}", env!("CARGO_PKG_VERSION")))
            .tls_config(tls)
            .build();
        Self { agent: config.into() }
    }
}
impl Fetch for Https {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, Failure> {
        let mut body = Vec::new();
        self.stream(url, limit, &mut |chunk| {
            body.extend_from_slice(chunk);
            Ok(())
        })?;
        Ok(body)
    }
    fn stream(&self, url: &str, limit: u64, sink: &mut dyn FnMut(&[u8]) -> Result<(), Failure>) -> Result<(), Failure> {
        let mut response = self.agent.get(url).call().map_err(|_| Failure::Network)?;
        let mut reader = response.body_mut().as_reader().take(limit + 1);
        let mut buffer = vec![0u8; 64 * 1024];
        let mut received = 0u64;
        loop {
            let n = reader.read(&mut buffer).map_err(|_| Failure::Network)?;
            if n == 0 {
                return Ok(());
            }
            received += n as u64;
            if received > limit {
                return Err(Failure::Invalid);
            }
            sink(&buffer[..n])?;
        }
    }
}

/// Where downloads go, for a test.
#[cfg(test)]
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("safay-updates-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use std::collections::HashMap;

    /// A throwaway key pair for tests only (the seed is 32 bytes of 7): never a real signing key.
    fn keys() -> (String, String) {
        let seed = [7u8; 32];
        let public = ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes();
        let b64 = base64::engine::general_purpose::STANDARD;
        (b64.encode(seed), b64.encode(public))
    }

    struct Memory(HashMap<String, Vec<u8>>);
    impl Fetch for Memory {
        fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, Failure> {
            let body = self.0.get(url).cloned().ok_or(Failure::Network)?;
            if body.len() as u64 > limit { Err(Failure::Invalid) } else { Ok(body) }
        }
        fn stream(&self, url: &str, limit: u64, sink: &mut dyn FnMut(&[u8]) -> Result<(), Failure>) -> Result<(), Failure> {
            let body = self.get(url, limit)?;
            for chunk in body.chunks(3) {
                sink(chunk)?;
            }
            Ok(())
        }
    }

    const BASE: &str = "https://updates.example.invalid/safay/";
    fn config(public: &str) -> Config {
        let mut c = Config::default();
        c.id = "com.gicek.safay".into();
        c.version = "0.1.0".into();
        c.url = Some(format!("{BASE}latest.json"));
        c.public_key = Some(public.into());
        c
    }
    fn manifest(version: &str, archive: &[u8]) -> Vec<u8> {
        format!(
            r#"{{"id":"com.gicek.safay","version":"{version}","notes":"Fixes","platforms":{{"{}":{{"url":"Safay-{version}.tar.gz","sha256":"{}","size":{}}}}}}}"#,
            nsh_updater::platform(),
            nsh_updater::sha256(archive),
            archive.len()
        )
        .into_bytes()
    }
    fn server(secret: &str, version: &str, archive: &[u8], served: &[u8]) -> Memory {
        let m = manifest(version, archive);
        let sig = nsh_updater::sign(secret, &m).unwrap();
        Memory(HashMap::from([
            (format!("{BASE}latest.json"), m),
            (format!("{BASE}latest.json.sig"), sig.into_bytes()),
            (format!("{BASE}Safay-{version}.tar.gz"), served.to_vec()),
        ]))
    }

    #[test]
    fn finds_downloads_and_verifies_a_newer_signed_version() {
        let (secret, public) = keys();
        let archive = b"synthetic archive bytes".to_vec();
        let dir = temp_dir("ok");
        let u = Updater::new(config(&public), None, dir.clone(), Arc::new(server(&secret, "0.2.0", &archive, &archive)));
        assert_eq!(u.check(), Phase::Available { version: "0.2.0".into(), notes: Some("Fixes".into()) });
        assert_eq!(u.download(), Phase::Ready { version: "0.2.0".into() });
        // A verified download stays ready on the next check.
        assert_eq!(u.check(), Phase::Ready { version: "0.2.0".into() });
        // Without an installation (a development build) nothing is replaced.
        assert_eq!(u.install(), Err(Failure::Install));
        let _ = fs::remove_dir_all(dir);
    }

    /// `App/run` with `body`, archived as an update (`.tar.gz`, one directory).
    fn app_archive(dir: &std::path::Path, body: &str) -> Vec<u8> {
        use std::os::unix::fs::PermissionsExt as _;
        let src = dir.join("src");
        fs::create_dir_all(src.join("App")).unwrap();
        fs::write(src.join("App/run"), body).unwrap();
        fs::set_permissions(src.join("App/run"), fs::Permissions::from_mode(0o755)).unwrap();
        let out = dir.join("update.tar.gz");
        let status = std::process::Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .arg("-czf")
            .arg(&out)
            .arg("-C")
            .arg(&src)
            .arg("App")
            .status()
            .unwrap();
        assert!(status.success());
        fs::read(out).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_download_is_staged_at_once_and_replaces_the_app_only_when_installed() {
        let (secret, public) = keys();
        let dir = temp_dir("staged");
        let archive = app_archive(&dir, "new");
        let root = dir.join("installed/App");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("run"), "old").unwrap();
        let installation = Installation::directory(&root, &root.join("run")).unwrap();
        let downloads = dir.join("downloads");
        let u =
            Updater::new(config(&public), Some(installation), downloads.clone(), Arc::new(server(&secret, "0.2.0", &archive, &archive)));
        assert!(matches!(u.check(), Phase::Available { .. }));
        assert_eq!(u.download(), Phase::Ready { version: "0.2.0".into() });
        // Unpacked and checked beside the app; the archive is gone and the app untouched.
        assert_eq!(fs::read_dir(&downloads).unwrap().count(), 0);
        assert_eq!(fs::read_to_string(root.join("run")).unwrap(), "old");
        u.install_on_quit();
        assert_eq!(fs::read_to_string(root.join("run")).unwrap(), "new");
        // Installed once: nothing is left to install.
        assert_eq!(u.install(), Err(Failure::Install));
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn an_update_that_is_not_the_app_fails_before_the_quit() {
        let (secret, public) = keys();
        let dir = temp_dir("not-the-app");
        let archive = app_archive(&dir, "new");
        let root = dir.join("installed/App");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("main"), "old").unwrap();
        // The installed app's executable is `main`; the archive has none.
        let installation = Installation::directory(&root, &root.join("main")).unwrap();
        let u = Updater::new(
            config(&public),
            Some(installation),
            dir.join("downloads"),
            Arc::new(server(&secret, "0.2.0", &archive, &archive)),
        );
        assert!(matches!(u.check(), Phase::Available { .. }));
        assert_eq!(u.download(), Phase::Failed(Failure::Install));
        u.install_on_quit();
        assert_eq!(fs::read_to_string(root.join("main")).unwrap(), "old");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn same_or_older_versions_are_up_to_date() {
        let (secret, public) = keys();
        let u = Updater::new(config(&public), None, temp_dir("same"), Arc::new(server(&secret, "0.1.0", b"x", b"x")));
        assert_eq!(u.check(), Phase::UpToDate);
    }

    #[test]
    fn a_manifest_signed_by_another_key_is_refused() {
        let (_, public) = keys();
        let other = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let u = Updater::new(config(&public), None, temp_dir("sig"), Arc::new(server(&other, "0.2.0", b"x", b"x")));
        assert_eq!(u.check(), Phase::Failed(Failure::Invalid));
    }

    #[test]
    fn a_tampered_archive_is_discarded() {
        let (secret, public) = keys();
        let dir = temp_dir("tampered");
        let u = Updater::new(config(&public), None, dir.clone(), Arc::new(server(&secret, "0.2.0", b"good bytes", b"evil bytes")));
        assert!(matches!(u.check(), Phase::Available { .. }));
        assert_eq!(u.download(), Phase::Failed(Failure::Invalid));
        assert_eq!(fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing unverified is kept");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unreachable_server_is_a_network_failure() {
        let (_, public) = keys();
        let u = Updater::new(config(&public), None, temp_dir("offline"), Arc::new(Memory(HashMap::new())));
        assert_eq!(u.check(), Phase::Failed(Failure::Network));
    }
}
