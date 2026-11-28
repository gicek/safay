//! Signing in through the official Dropbox app (`cloud::dropbox_app_complete`), against `cloudset`'s in-memory Dropbox:
//! the app's answer is checked before anything is sent, and a good one stores a connection like the browser's sign-in.
//! Only synthetic data; no network, no keychain. A test binary of its own: `cloud::install` sets process-wide services.
use cloudset::{
    AccessScope, CertificatePin, HttpRequest, HttpResponse, OAuthClient, ProviderKind,
    testing::oauth::{FakeProvider, MemoryCloud},
};
use safay_app::{
    cloud::{self, Credentials, DropboxAppAnswer, Http, Provider, Services, TransportError},
    sync::{Location, SyncError},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;

struct Server(Mutex<MemoryCloud>, Mutex<usize>);
impl Http for Server {
    fn execute(&self, request: HttpRequest, _pin: Option<&CertificatePin>) -> Result<HttpResponse, TransportError> {
        *self.1.lock().unwrap() += 1;
        Ok(self.0.lock().unwrap().handle(request))
    }
}
#[derive(Default)]
struct Memory(Mutex<HashMap<String, Vec<u8>>>);
impl Credentials for Memory {
    fn load(&self, id: &str) -> Option<Zeroizing<Vec<u8>>> {
        self.0.lock().unwrap().get(id).cloned().map(Zeroizing::new)
    }
    fn save(&self, id: &str, blob: &[u8]) -> Result<(), SyncError> {
        self.0.lock().unwrap().insert(id.into(), blob.to_vec());
        Ok(())
    }
    fn delete(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

#[test]
fn the_dropbox_app_answer_is_checked_then_stored() {
    let server = Arc::new(Server(Mutex::new(MemoryCloud::new(FakeProvider::Dropbox)), Mutex::new(0)));
    let credentials = Arc::new(Memory::default());
    cloud::install(Services { http: server.clone(), credentials: credentials.clone(), user_agent: "safay-test/1".into() });
    let begin = || {
        let client = OAuthClient::new("client", "db-client://2/token");
        cloudset::oauth::begin(ProviderKind::Dropbox, AccessScope::AppFolder, client, &[7; 64]).unwrap()
    };
    let authorization = begin();
    let state = authorization.dropbox_app_request().unwrap().auth_state;
    let extras = |token_type: &str, code: &str, state: &str| DropboxAppAnswer::Extras {
        token_type: token_type.into(),
        code: Zeroizing::new(code.into()),
        state: state.into(),
    };

    // Cancelled in the app (no code), or on iOS a return link other than `/connect`: nothing sent, nothing stored.
    assert_eq!(cloud::dropbox_app_complete(&authorization, extras("oauth2code:", "", &state)), Ok(None));
    let cancelled = DropboxAppAnswer::Callback("db-client://1/cancel".into());
    assert_eq!(cloud::dropbox_app_complete(&authorization, cancelled), Ok(None));
    // Another sign-in's answer is refused, again without a request.
    assert!(cloud::dropbox_app_complete(&authorization, extras("oauth2code:", "code", "another")).is_err());
    assert_eq!(*server.1.lock().unwrap(), 0, "a refused answer sends nothing");
    assert!(credentials.0.lock().unwrap().is_empty());

    // A code the provider issued for this sign-in: exchanged with its verifier, the connection stored.
    let callback = server.0.lock().unwrap().authorize(authorization.url());
    let query = callback.split_once('?').unwrap().1;
    let code = query.split('&').find_map(|pair| pair.strip_prefix("code=")).unwrap().to_owned();
    let location = cloud::dropbox_app_complete(&authorization, extras("oauth2code:", &code, &state)).unwrap().unwrap();
    let Location::Cloud { provider, credential, .. } = &location else { panic!("a cloud location") };
    assert_eq!(*provider, Provider::Dropbox);
    assert!(credentials.load(credential).is_some());
}

#[test]
fn ios_can_open_both_dropbox_apps_it_queries() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../../../nsh.json")).unwrap();
    let queried = manifest["ios"]["infoPlist"]["LSApplicationQueriesSchemes"].as_array().unwrap();
    let granted = manifest["capabilities"]["openUrl"]["schemes"].as_array().unwrap();
    for scheme in ["dbapi-2", "dbapi-8-emm"] {
        assert!(queried.iter().any(|entry| entry == scheme), "iOS must be able to detect {scheme}");
        assert!(granted.iter().any(|entry| entry == scheme), "NSH must allow opening {scheme}");
    }
}
