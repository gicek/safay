//! The loopback redirect of a browser sign-in (Google's "Desktop app" clients, RFC 8252 §7.3): Safay listens on a free
//! port of the loopback address for as long as a sign-in is in progress, and the provider redirects the browser there.
//!
//! Only the request that belongs to the sign-in ends it (`accepts`: the core compares the address and `state`); anything
//! else on the port gets a 404 and the wait goes on. Nothing is read beyond the request line, and nothing is served
//! but one fixed page.
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use super::Signed;

/// How long a sign-in may take before the port is given up.
const PATIENCE: Duration = Duration::from_secs(600);
/// A request line longer than this is not a redirect.
const MAX_REQUEST_LINE: u64 = 8 * 1024;
/// The sign-in being waited for; a newer one, or [`cancel`], ends the wait of an older one.
static CURRENT: AtomicU64 = AtomicU64::new(0);

/// A port held for one sign-in.
pub struct Loopback {
    listener: TcpListener,
    /// The redirect URI of this sign-in, with the port: `http://127.0.0.1:<port>/`.
    pub redirect_uri: String,
    turn: u64,
}

/// Whether a configured redirect URI is a loopback address, to be completed with a port.
pub fn is_loopback(redirect_uri: &str) -> bool {
    host(redirect_uri).is_some()
}

/// The loopback host of `http://127.0.0.1`, `http://[::1]` or `http://localhost`, with or without a trailing slash.
fn host(redirect_uri: &str) -> Option<&str> {
    let host = redirect_uri.strip_prefix("http://")?.trim_end_matches('/');
    matches!(host, "127.0.0.1" | "[::1]" | "localhost").then_some(host)
}

/// Takes a free port of the configured loopback address. `None` if the address is not one, or no port can be had.
pub fn bind(redirect_uri: &str) -> Option<Loopback> {
    let host = host(redirect_uri)?;
    // `localhost` is only a name in the redirect: the listener is on the address itself.
    let address = if host == "localhost" { "127.0.0.1" } else { host };
    let listener = TcpListener::bind(format!("{address}:0")).ok()?;
    listener.set_nonblocking(true).ok()?;
    let port = listener.local_addr().ok()?.port();
    let turn = CURRENT.fetch_add(1, Ordering::SeqCst) + 1;
    Some(Loopback { listener, redirect_uri: format!("http://{host}:{port}/"), turn })
}

/// Ends the wait of the sign-in in progress, if any (the user left its page).
pub fn cancel() {
    CURRENT.fetch_add(1, Ordering::SeqCst);
}

impl Loopback {
    /// Waits for the redirect: the callback URL of the first request `accepts` takes, which is answered with `page`.
    /// Blocks; [`Signed::Closed`] when canceled, superseded or out of patience.
    pub fn wait(self, accepts: impl Fn(&str) -> bool, page: &str) -> Signed {
        let deadline = Instant::now() + PATIENCE;
        while CURRENT.load(Ordering::SeqCst) == self.turn && Instant::now() < deadline {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(callback) = self.serve(stream, &accepts, page) {
                        return Signed::Callback(callback);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
                Err(_) => return Signed::Failed,
            }
        }
        Signed::Closed
    }

    /// Answers one connection; the callback URL if it carried the redirect of this sign-in.
    fn serve(&self, stream: TcpStream, accepts: &impl Fn(&str) -> bool, page: &str) -> Option<String> {
        stream.set_nonblocking(false).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
        stream.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
        let mut line = String::new();
        BufReader::new((&stream).take(MAX_REQUEST_LINE)).read_line(&mut line).ok()?;
        let mut parts = line.split_whitespace();
        let callback = match (parts.next(), parts.next()) {
            (Some("GET"), Some(target)) if target.starts_with('/') => {
                Some(format!("{}{target}", self.redirect_uri.trim_end_matches('/'))).filter(|callback| accepts(callback))
            }
            _ => None,
        };
        let (status, body) = if callback.is_some() { ("200 OK", page) } else { ("404 Not Found", "") };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        );
        drop((&stream).write_all(response.as_bytes()));
        callback
    }
}

/// The page the browser shows once the sign-in is back in Safay: the two lines of text, escaped.
pub fn page(title: &str, message: &str) -> String {
    let escape = |text: &str| text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{0}</title></head>\
         <body style=\"font:16px system-ui,sans-serif;margin:4em auto;max-width:28em;text-align:center\">\
         <h1 style=\"font-size:1.3em\">{0}</h1><p>{1}</p></body></html>",
        escape(title),
        escape(message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(port: &str, line: &str) -> String {
        let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
        stream.write_all(format!("{line}\r\nHost: 127.0.0.1\r\n\r\n").as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn only_the_redirect_of_this_sign_in_ends_the_wait_until_it_is_canceled() {
        assert!(is_loopback("http://127.0.0.1") && is_loopback("http://localhost/") && is_loopback("http://[::1]"));
        for other in ["https://127.0.0.1", "http://127.0.0.1:80", "http://example.com", "com.example.app://callback"] {
            assert!(!is_loopback(other) && bind(other).is_none(), "{other}");
        }
        let loopback = bind("http://127.0.0.1").unwrap();
        let redirect = loopback.redirect_uri.clone();
        let port = redirect.trim_end_matches('/').rsplit(':').next().unwrap().to_owned();
        assert_eq!(redirect, format!("http://127.0.0.1:{port}/"));
        let waiting = std::thread::spawn(move || loopback.wait(|callback| callback.contains("state=mine"), "<p>done & closed</p>"));
        // A browser's other requests, a foreign state and a non-GET are turned away, and the wait goes on.
        assert!(request(&port, "GET /favicon.ico HTTP/1.1").starts_with("HTTP/1.1 404"));
        assert!(request(&port, "GET /?code=x&state=theirs HTTP/1.1").starts_with("HTTP/1.1 404"));
        assert!(request(&port, "POST /?code=x&state=mine HTTP/1.1").starts_with("HTTP/1.1 404"));
        assert!(request(&port, "GET http://evil.example/?state=mine HTTP/1.1").starts_with("HTTP/1.1 404"));
        let answer = request(&port, "GET /?code=abc&state=mine HTTP/1.1");
        assert!(answer.starts_with("HTTP/1.1 200") && answer.ends_with("<p>done & closed</p>"), "{answer}");
        let Signed::Callback(callback) = waiting.join().unwrap() else { panic!("the redirect ends the wait") };
        assert_eq!(callback, format!("http://127.0.0.1:{port}/?code=abc&state=mine"));

        // Canceling ends a wait, and so does a newer sign-in.
        let loopback = bind("http://localhost").unwrap();
        assert!(loopback.redirect_uri.starts_with("http://localhost:"));
        let waiting = std::thread::spawn(move || loopback.wait(|_| true, ""));
        cancel();
        assert!(matches!(waiting.join().unwrap(), Signed::Closed));
        let older = bind("http://127.0.0.1").unwrap();
        let _newer = bind("http://127.0.0.1").unwrap();
        assert!(matches!(older.wait(|_| true, ""), Signed::Closed));
        let page = page("Signed in", "<b>Safay & you</b>");
        assert!(page.contains("&lt;b&gt;Safay &amp; you&lt;/b&gt;") && page.contains("<title>Signed in</title>"));
    }
}
