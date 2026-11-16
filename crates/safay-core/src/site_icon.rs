//! Account websites and site icons (see SECURITY.md).
//!
//! - [`normalized_website`] turns user input (a URL or a host name) into the host stored in `Account::website`, and
//!   rejects anything that isn't a public DNS name.
//! - [`suggested_website`] proposes a website for a service name: the name itself when it is a host name, otherwise a
//!   lookup in the bundled index (2fa.directory names and domains plus a hand-maintained alias list). It never guesses.
//! - [`SiteIconFetch`] is a sans-IO state machine that fetches an icon from the site itself. The core builds every URL
//!   and checks every response; the platform only executes HTTPS requests and decodes images with the system decoder.
//!   It is independent of `cloudset`: it talks to arbitrary public sites, and every failure is silent.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use unicode_normalization::UnicodeNormalization;
use url::{Host, Url};

use crate::canonical::clean_text;

/// A stored icon: a PNG of at most this many bytes...
pub const MAX_ICON_BYTES: usize = 32 * 1024;
/// ...and at most this many pixels on each side.
pub const ICON_PIXELS: u32 = 96;
/// Downloaded images smaller than this on either side are rejected (a 16-pixel favicon looks worse than the monogram).
pub const MIN_SOURCE_PIXELS: u32 = 32;
/// Downloaded images larger than this on either side are rejected before decoding.
pub const MAX_SOURCE_PIXELS: u32 = 1024;
/// HTML read while looking for icon links; anything after this is ignored.
pub const MAX_PAGE_BYTES: u64 = 256 * 1024;
/// Downloaded image size; larger responses are rejected.
pub const MAX_IMAGE_BYTES: u64 = 512 * 1024;
/// Redirects followed per request.
pub const MAX_REDIRECTS: u32 = 3;
/// Timeout of a single request.
pub const REQUEST_TIMEOUT_SECONDS: u32 = 10;
/// Total time spent on one website before giving up.
pub const HOST_TIMEOUT_SECONDS: u32 = 15;
/// Websites fetched at the same time.
pub const MAX_CONCURRENT_HOSTS: u32 = 4;
/// Images decoded at the same time.
pub const MAX_CONCURRENT_DECODES: u32 = 2;
/// Fetched icons are saved at most this often (and once more when the queue drains).
pub const SAVE_INTERVAL_SECONDS: u32 = 5;

const MAX_INPUT_CHARS: usize = 2048;
const MAX_HOST_LENGTH: usize = 253;
/// Declared icons tried per website, before the two well-known paths.
const MAX_DECLARED_CANDIDATES: usize = 4;
/// Top-level labels that never name a public site.
const PRIVATE_SUFFIXES: [&str; 13] =
    ["arpa", "corp", "example", "home", "internal", "intranet", "invalid", "lan", "local", "localhost", "onion", "private", "test"];

// MARK: - Websites

/// Normalizes a URL or host name into a public host name: lowercase, IDNA (punycode), no scheme, port, path, credentials,
/// or trailing dot. Returns `None` for empty input, IP addresses, single-label names, and private suffixes such as
/// `.local` or `.internal`.
pub fn normalized_website(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_INPUT_CHARS || trimmed.chars().any(char::is_whitespace) {
        return None;
    }
    let lowered = trimmed.to_ascii_lowercase();
    let text = if lowered.starts_with("https://") || lowered.starts_with("http://") {
        trimmed.to_owned()
    } else if trimmed.contains("://") {
        return None;
    } else {
        format!("https://{trimmed}")
    };
    host_of(&Url::parse(&text).ok()?)
}

/// The public host of a URL, or `None` if it isn't one (same rules as [`normalized_website`]).
fn host_of(url: &Url) -> Option<String> {
    let Some(Host::Domain(domain)) = url.host() else { return None };
    let host = domain.trim_end_matches('.');
    if host.is_empty() || host.len() > MAX_HOST_LENGTH {
        return None;
    }
    let labels: Vec<&str> = host.split('.').collect();
    let top = *labels.last()?;
    if labels.len() < 2
        || labels.iter().any(|label| label.is_empty() || label.len() > 63)
        || top.chars().all(|c| c.is_ascii_digit())
        || PRIVATE_SUFFIXES.contains(&top)
    {
        return None;
    }
    Some(host.to_owned())
}

/// Proposes a website for a service name: the bundled index first, then the name itself when it looks like a host name
/// (`github.com`). Returns `None` rather than guessing (no `<name>.com`).
pub fn suggested_website(issuer: &str) -> Option<String> {
    let issuer = clean_text(issuer);
    if let Some(domain) = index().get(&name_key(&issuer)) {
        return domain.clone();
    }
    if issuer.contains('.') && !issuer.chars().any(char::is_whitespace) {
        let host = normalized_website(&issuer)?;
        let top = host.rsplit('.').next()?;
        if top.len() >= 2 && (top.chars().all(|c| c.is_ascii_alphabetic()) || top.starts_with("xn--")) {
            return Some(host);
        }
    }
    None
}

/// Lookup key for a service name: NFKC, lowercase, letters and digits only.
fn name_key(name: &str) -> String {
    name.nfkc().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()).collect()
}

/// Name key → domain; `None` marks a key that maps to several domains and is therefore not suggested.
fn index() -> &'static HashMap<String, Option<String>> {
    static INDEX: OnceLock<HashMap<String, Option<String>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let aliases = rows(include_str!("../data/site-aliases.tsv"));
        let entries = rows(include_str!("../data/site-domains.tsv"));
        build_index(&[
            aliases.iter().map(|(name, domain)| (name_key(name), *domain)).collect(),
            entries.iter().map(|(name, domain)| (name_key(name), *domain)).collect(),
            // "X (Twitter)" also answers to "X".
            entries
                .iter()
                .filter(|(name, _)| name.contains('('))
                .map(|(name, domain)| (name_key(&strip_parentheses(name)), *domain))
                .collect(),
            // A two-label domain also answers to its first label: "outlook.com" for "Outlook".
            entries
                .iter()
                .filter_map(|(_, domain)| {
                    domain.split_once('.').filter(|(_, rest)| !rest.contains('.')).map(|(label, _)| (name_key(label), *domain))
                })
                .collect(),
        ])
    })
}

fn rows(text: &str) -> Vec<(&str, &str)> {
    text.lines().filter(|line| !line.starts_with('#')).filter_map(|line| line.split_once('\t')).collect()
}

fn strip_parentheses(name: &str) -> String {
    let mut depth = 0usize;
    name.chars()
        .filter(|c| {
            match c {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                _ => return depth == 0,
            }
            false
        })
        .collect()
}

/// Tiers in priority order: a key set by an earlier tier is never overridden, and a key with several domains within one
/// tier is recorded as ambiguous.
fn build_index(tiers: &[Vec<(String, &str)>]) -> HashMap<String, Option<String>> {
    let mut index: HashMap<String, Option<String>> = HashMap::new();
    for tier in tiers {
        let mut domains: HashMap<&str, HashSet<&str>> = HashMap::new();
        for (key, domain) in tier {
            if !key.is_empty() && !index.contains_key(key) {
                domains.entry(key.as_str()).or_default().insert(domain);
            }
        }
        for (key, set) in domains {
            let domain = if set.len() == 1 { set.into_iter().next().and_then(normalized_website) } else { None };
            index.insert(key.to_owned(), domain);
        }
    }
    index
}

// MARK: - Icons

/// Whether `data` can be stored as an icon: a PNG within [`MAX_ICON_BYTES`] whose header declares at most
/// [`ICON_PIXELS`] on each side.
pub fn is_valid_icon(data: &[u8]) -> bool {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if data.len() > MAX_ICON_BYTES || data.len() < 24 || data[..8] != SIGNATURE || &data[12..16] != b"IHDR" {
        return false;
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    (1..=ICON_PIXELS).contains(&width) && (1..=ICON_PIXELS).contains(&height)
}

/// A request for the platform to send: `GET url` over HTTPS, reading at most `max_bytes + 1` bytes of the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub max_bytes: u64,
}

/// What the platform does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Fetch(Request),
    /// Decode the body of the last response with the system decoder, check its size against [`MIN_SOURCE_PIXELS`] and
    /// [`MAX_SOURCE_PIXELS`] from the header first, scale it to fit [`ICON_PIXELS`], and re-encode it as PNG. Report
    /// [`SiteIconFetch::decode_failed`] if that fails or [`is_valid_icon`] rejects the result.
    Decode,
    /// Nothing more to try: keep the monogram.
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Page,
    Image,
}

/// Fetches one website's icon: the home page, then declared icons (`apple-touch-icon` first, largest first), then
/// `/apple-touch-icon.png` and `/favicon.ico`. Every URL is HTTPS on a host that passes [`normalized_website`]'s rules,
/// including redirect targets.
#[derive(Clone, Debug)]
pub struct SiteIconFetch {
    host: String,
    current: Url,
    purpose: Purpose,
    redirects: u32,
    candidates: VecDeque<Url>,
    tried: HashSet<String>,
}

impl SiteIconFetch {
    /// Starts with the home page of `website`; `None` if it isn't a valid website.
    pub fn start(website: &str) -> Option<(SiteIconFetch, Request)> {
        let host = normalized_website(website)?;
        let current = Url::parse(&format!("https://{host}/")).ok()?;
        let fetch = SiteIconFetch {
            host,
            current: current.clone(),
            purpose: Purpose::Page,
            redirects: 0,
            candidates: VecDeque::new(),
            tried: HashSet::from([current.to_string()]),
        };
        let request = fetch.request();
        Some((fetch, request))
    }

    /// The website this fetch is for (normalized).
    pub fn website(&self) -> &str {
        &self.host
    }

    fn request(&self) -> Request {
        let max_bytes = match self.purpose {
            Purpose::Page => MAX_PAGE_BYTES,
            Purpose::Image => MAX_IMAGE_BYTES,
        };
        Request { url: self.current.to_string(), max_bytes }
    }

    /// Handles the response to the last request. `location` is the `Location` header of a redirect; `body` is at most
    /// `max_bytes + 1` bytes.
    pub fn handle_response(&mut self, status: u16, location: Option<&str>, body: &[u8]) -> Step {
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            return match location.and_then(|location| self.redirect_target(location)) {
                Some(target) => {
                    self.redirects += 1;
                    self.tried.insert(target.to_string());
                    self.current = target;
                    Step::Fetch(self.request())
                }
                None => self.request_failed(),
            };
        }
        match self.purpose {
            Purpose::Page => {
                if status == 200 {
                    let length = body.len().min(MAX_PAGE_BYTES as usize);
                    let html = String::from_utf8_lossy(&body[..length]);
                    let declared = icon_links(&html, &self.current);
                    self.queue(declared.into_iter().take(MAX_DECLARED_CANDIDATES));
                }
                self.queue_well_known();
                self.next_candidate()
            }
            Purpose::Image => {
                if status == 200 && !body.is_empty() && body.len() as u64 <= MAX_IMAGE_BYTES && is_raster_image(body) {
                    Step::Decode
                } else {
                    self.next_candidate()
                }
            }
        }
    }

    /// The last request failed without a response (network error, timeout, TLS failure).
    pub fn transport_failed(&mut self) -> Step {
        self.request_failed()
    }

    /// The platform couldn't decode the last image, or the result failed [`is_valid_icon`].
    pub fn decode_failed(&mut self) -> Step {
        self.next_candidate()
    }

    fn request_failed(&mut self) -> Step {
        if self.purpose == Purpose::Page {
            self.queue_well_known();
        }
        self.next_candidate()
    }

    fn redirect_target(&self, location: &str) -> Option<Url> {
        if self.redirects >= MAX_REDIRECTS {
            return None;
        }
        let target = self.current.join(location.trim()).ok()?;
        (target.scheme() == "https" && target.port().is_none() && host_of(&target).is_some()).then_some(target)
    }

    /// `/apple-touch-icon.png` and `/favicon.ico` on the host the home page ended up on (or the original host).
    fn queue_well_known(&mut self) {
        let base = self.current.clone();
        let paths = ["/apple-touch-icon.png", "/favicon.ico"];
        let urls: Vec<Url> = paths.iter().filter_map(|path| base.join(path).ok()).collect();
        self.queue(urls.into_iter());
        if base.host_str() != Some(self.host.as_str())
            && let Ok(original) = Url::parse(&format!("https://{}/", self.host))
        {
            let urls: Vec<Url> = paths.iter().filter_map(|path| original.join(path).ok()).collect();
            self.queue(urls.into_iter());
        }
    }

    fn queue(&mut self, urls: impl Iterator<Item = Url>) {
        for url in urls {
            if url.scheme() == "https" && url.port().is_none() && host_of(&url).is_some() && self.tried.insert(url.to_string()) {
                self.candidates.push_back(url);
            }
        }
    }

    fn next_candidate(&mut self) -> Step {
        match self.candidates.pop_front() {
            Some(url) => {
                self.current = url;
                self.purpose = Purpose::Image;
                self.redirects = 0;
                Step::Fetch(self.request())
            }
            None => Step::Finished,
        }
    }
}

/// PNG, ICO, JPEG, GIF, or WebP, judged by content rather than `Content-Type` (servers often send `favicon.ico` as
/// `text/plain`). SVG and anything else is rejected.
fn is_raster_image(data: &[u8]) -> bool {
    data.starts_with(&[0x89, b'P', b'N', b'G'])
        || data.starts_with(&[0x00, 0x00, 0x01, 0x00])
        || data.starts_with(&[0xFF, 0xD8, 0xFF])
        || data.starts_with(b"GIF8")
        || (data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP")
}

// MARK: - Badge layout

/// Alpha above this counts as visible content when finding the icon's bounds.
const VISIBLE_ALPHA: u8 = 16;
/// Alpha at or above this counts as solid when checking the edges of the content.
const SOLID_ALPHA: u8 = 240;
/// Share of each edge's middle third that must be solid for the icon to count as a tile.
const SOLID_EDGE_SHARE: f64 = 0.9;
/// Share of the content's bounding box that must be at least half opaque. A circle covers about 78.5% of its box; a
/// rounded square still covers over 86% with corners as round as 40% of its side.
const MIN_TILE_COVERAGE: f64 = 0.85;
/// Alpha at or above this counts as covered when measuring coverage.
const COVERED_ALPHA: u8 = 128;
/// The content must span at least this share of the image on both sides, and be roughly square.
const MIN_TILE_SHARE: f64 = 0.5;
const MAX_TILE_ASPECT: f64 = 1.25;

/// A pixel rectangle inside an icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Decides how a badge shows an icon, from its alpha channel (`width * height` bytes, row by row).
///
/// Returns the bounds of the content when the icon is a **tile**: an opaque square or rounded square that carries its own
/// background (most `apple-touch-icon`s, X, app-style logos). The badge then crops to these bounds and fills its whole
/// shape. Returns `None` for a **glyph**: a logo on a transparent background (Google's "G"), a circle, or anything else; the
/// badge shows it inset on its neutral background.
///
/// Rule: find the bounding box of visible pixels; it must be roughly square and cover at least half the image. One pixel
/// inside each edge of the box (skipping anti-aliasing), the middle third of that edge must be at least 90% solid, and
/// at least 85% of the box must be covered. Coverage accepts rounded squares with any common corner radius, and rejects a circle
/// at any resolution.
pub fn tile_bounds(alpha: &[u8], width: u32, height: u32) -> Option<Bounds> {
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || alpha.len() != w.checked_mul(h)? {
        return None;
    }
    let visible = |x: usize, y: usize| alpha[y * w + x] > VISIBLE_ALPHA;
    let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| visible(x, y))).collect();
    let columns: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| visible(x, y))).collect();
    let (top, bottom) = (*rows.first()?, *rows.last()?);
    let (left, right) = (*columns.first()?, *columns.last()?);
    let (box_width, box_height) = (right - left + 1, bottom - top + 1);
    let (short, long) = (box_width.min(box_height) as f64, box_width.max(box_height) as f64);
    if box_width < 8
        || box_height < 8
        || long / short > MAX_TILE_ASPECT
        || (box_width as f64) < w as f64 * MIN_TILE_SHARE
        || (box_height as f64) < h as f64 * MIN_TILE_SHARE
    {
        return None;
    }
    let solid = |x: usize, y: usize| alpha[y * w + x] >= SOLID_ALPHA;
    let middle = |start: usize, length: usize| (start + length / 3)..(start + length - length / 3);
    let share = |points: Vec<bool>| points.iter().filter(|&&p| p).count() as f64 / points.len().max(1) as f64;
    let edges = [
        share(middle(left, box_width).map(|x| solid(x, top + 1)).collect()),
        share(middle(left, box_width).map(|x| solid(x, bottom - 1)).collect()),
        share(middle(top, box_height).map(|y| solid(left + 1, y)).collect()),
        share(middle(top, box_height).map(|y| solid(right - 1, y)).collect()),
    ];
    let covered =
        (top..=bottom).flat_map(|y| (left..=right).map(move |x| (x, y))).filter(|&(x, y)| alpha[y * w + x] >= COVERED_ALPHA).count();
    let coverage = covered as f64 / (box_width * box_height) as f64;
    let is_tile = edges.iter().all(|&edge| edge >= SOLID_EDGE_SHARE) && coverage >= MIN_TILE_COVERAGE;
    is_tile.then_some(Bounds { x: left as u32, y: top as u32, width: box_width as u32, height: box_height as u32 })
}

// MARK: - HTML

#[derive(Debug)]
struct Link {
    rank: u8,
    size: u32,
    order: usize,
    url: Url,
}

/// Icon links declared in the `<head>` of `html`, best first: `apple-touch-icon` before `icon`, then larger declared sizes
/// first. SVG, `data:` URLs, and anything that doesn't resolve to HTTPS are skipped.
pub fn icon_links(html: &str, base: &Url) -> Vec<Url> {
    let lower = html.to_ascii_lowercase();
    let head_end = ["</head", "<body"].iter().filter_map(|marker| lower.find(marker)).min().unwrap_or(lower.len());
    let mut links = Vec::new();
    let mut position = 0;
    while let Some(offset) = lower[position..head_end].find("<link") {
        let start = position + offset + "<link".len();
        let Some(end) = tag_end(&html[start..]).map(|end| start + end) else { break };
        position = end;
        if !html[start..].starts_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let attributes = attributes(&html[start..end]);
        let value = |name: &str| attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str());
        let rel = value("rel").unwrap_or_default().to_ascii_lowercase();
        let tokens: Vec<&str> = rel.split_ascii_whitespace().collect();
        let rank = if tokens.iter().any(|token| token.starts_with("apple-touch-icon")) {
            0
        } else if tokens.contains(&"icon") {
            1
        } else {
            continue;
        };
        let href = decode_entities(value("href").unwrap_or_default().trim());
        let is_svg = value("type").is_some_and(|kind| kind.to_ascii_lowercase().contains("svg"));
        if href.is_empty() || is_svg || href.to_ascii_lowercase().starts_with("data:") {
            continue;
        }
        let Ok(mut url) = base.join(&href) else { continue };
        if url.scheme() != "https" || url.path().to_ascii_lowercase().ends_with(".svg") {
            continue;
        }
        url.set_fragment(None);
        let size = value("sizes").map(largest_size).unwrap_or(if rank == 0 { 180 } else { MIN_SOURCE_PIXELS });
        links.push(Link { rank, size, order: links.len(), url });
    }
    links.sort_by(|a, b| a.rank.cmp(&b.rank).then(b.size.cmp(&a.size)).then(a.order.cmp(&b.order)));
    let mut seen = HashSet::new();
    links.into_iter().filter(|link| seen.insert(link.url.to_string())).map(|link| link.url).collect()
}

/// Index of the `>` closing a tag, skipping quoted attribute values.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (index, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

/// Attributes of a tag body as (lowercase name, raw value).
fn attributes(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        while index < chars.len() && (chars[index].is_whitespace() || chars[index] == '/') {
            index += 1;
        }
        let start = index;
        while index < chars.len() && !chars[index].is_whitespace() && !matches!(chars[index], '=' | '/') {
            index += 1;
        }
        let name: String = chars[start..index].iter().collect::<String>().to_ascii_lowercase();
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let mut value = String::new();
        if index < chars.len() && chars[index] == '=' {
            index += 1;
            while index < chars.len() && chars[index].is_whitespace() {
                index += 1;
            }
            if index < chars.len() && matches!(chars[index], '"' | '\'') {
                let quote = chars[index];
                index += 1;
                let start = index;
                while index < chars.len() && chars[index] != quote {
                    index += 1;
                }
                value = chars[start..index].iter().collect();
                index += 1;
            } else {
                let start = index;
                while index < chars.len() && !chars[index].is_whitespace() {
                    index += 1;
                }
                value = chars[start..index].iter().collect();
            }
        }
        if name.is_empty() {
            index += 1;
        } else {
            result.push((name, value));
        }
    }
    result
}

/// The largest side declared in `sizes` (`"16x16 180x180"`); 0 for `any` or nothing readable.
fn largest_size(sizes: &str) -> u32 {
    sizes
        .split_ascii_whitespace()
        .filter_map(|size| {
            let (width, height) = size.to_ascii_lowercase().split_once('x').map(|(w, h)| (w.to_owned(), h.to_owned()))?;
            Some(width.parse::<u32>().ok()?.min(height.parse::<u32>().ok()?))
        })
        .max()
        .unwrap_or(0)
}

/// Decodes the character references that appear in URLs: `&amp;`, `&quot;`, `&apos;`, `&lt;`, `&gt;`, and numeric ones.
fn decode_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        rest = &rest[index..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                _ => {
                    let number = entity.strip_prefix('#')?;
                    let code = match number.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => number.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            }?;
            Some((c, end + 1))
        });
        match decoded {
            Some((c, length)) => {
                output.push(c);
                rest = &rest[length..];
            }
            None => {
                output.push('&');
                rest = &rest[1..];
            }
        }
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, length: usize) -> Vec<u8> {
        let mut data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        data.extend_from_slice(b"IHDR");
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.resize(length.max(data.len()), 0);
        data
    }

    fn fetch_url(step: Step) -> String {
        match step {
            Step::Fetch(request) => request.url,
            other => panic!("expected a fetch, got {other:?}"),
        }
    }

    #[test]
    fn normalizes_websites() {
        assert_eq!(normalized_website("GitHub.com").as_deref(), Some("github.com"));
        assert_eq!(normalized_website(" https://user:pw@Login.Example.org:8443/a?b#c ").as_deref(), Some("login.example.org"));
        assert_eq!(normalized_website("http://github.com.").as_deref(), Some("github.com"));
        assert_eq!(normalized_website("bücher.de").as_deref(), Some("xn--bcher-kva.de"));
        for invalid in [
            "",
            "GitHub",
            "github com",
            "ftp://github.com",
            "192.168.1.1",
            "[::1]",
            "nas.local",
            "router.lan",
            "x.home.arpa",
            "localhost",
            "a..com",
            "1.2.3.4.5",
        ] {
            assert_eq!(normalized_website(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn suggests_websites_without_guessing() {
        assert_eq!(suggested_website("GitHub").as_deref(), Some("github.com"));
        assert_eq!(suggested_website("google").as_deref(), Some("google.com"));
        assert_eq!(suggested_website("Amazon Web Services").as_deref(), Some("aws.amazon.com"));
        assert_eq!(suggested_website("X").as_deref(), Some("x.com"));
        assert_eq!(suggested_website("Outlook").as_deref(), Some("outlook.com"), "first label of a two-label domain");
        assert_eq!(suggested_website("login.example.org").as_deref(), Some("login.example.org"));
        assert_eq!(suggested_website("Unheard Of Service"), None);
        assert_eq!(suggested_website("Node.js 2024"), None);
    }

    #[test]
    fn ambiguous_index_keys_suggest_nothing() {
        let index =
            build_index(&[vec![("a".into(), "one.com"), ("a".into(), "two.com"), ("b".into(), "b.com")], vec![("a".into(), "three.com")]]);
        assert_eq!(index.get("a"), Some(&None));
        assert_eq!(index.get("b"), Some(&Some("b.com".to_owned())));
    }

    #[test]
    fn validates_stored_icons() {
        assert!(is_valid_icon(&png(96, 96, 4000)));
        assert!(!is_valid_icon(&png(97, 96, 4000)));
        assert!(!is_valid_icon(&png(0, 96, 4000)));
        assert!(!is_valid_icon(&png(64, 64, MAX_ICON_BYTES + 1)));
        assert!(!is_valid_icon(b"GIF89a..................."));
    }

    #[test]
    fn ranks_declared_icons() {
        let base = Url::parse("https://example.com/en/").unwrap();
        let html = r#"<html><head>
            <link rel="icon" href="/favicon-16.png" sizes="16x16">
            <link rel="icon" type="image/svg+xml" href="/icon.svg">
            <LINK REL="shortcut icon" HREF='favicon.ico'>
            <link rel="mask-icon" href="/mask.png">
            <link rel=icon href=//cdn.example.net/i.png?a=1&amp;b=2 sizes="192x192">
            <link rel="apple-touch-icon" href="/touch.png">
            <link rel="icon" href="http://example.com/insecure.png" sizes="512x512">
            <link rel="icon" href="data:image/png;base64,AAAA">
            </head><body><link rel="apple-touch-icon" href="/body.png"></body>"#;
        let urls: Vec<String> = icon_links(html, &base).into_iter().map(|url| url.to_string()).collect();
        assert_eq!(
            urls,
            [
                "https://example.com/touch.png",
                "https://cdn.example.net/i.png?a=1&b=2",
                "https://example.com/en/favicon.ico",
                "https://example.com/favicon-16.png",
            ]
        );
    }

    #[test]
    fn fetches_declared_then_well_known_icons() {
        let (mut fetch, request) = SiteIconFetch::start("https://Example.com/login").unwrap();
        assert_eq!(request, Request { url: "https://example.com/".into(), max_bytes: MAX_PAGE_BYTES });
        let step = fetch.handle_response(301, Some("https://www.example.com/"), b"");
        assert_eq!(fetch_url(step), "https://www.example.com/");
        let html = br#"<head><link rel="apple-touch-icon" href="/t.png"></head>"#;
        assert_eq!(fetch_url(fetch.handle_response(200, None, html)), "https://www.example.com/t.png");
        assert_eq!(fetch_url(fetch.handle_response(404, None, b"")), "https://www.example.com/apple-touch-icon.png");
        assert_eq!(fetch.handle_response(200, None, &png(180, 180, 500)), Step::Decode);
        assert_eq!(fetch_url(fetch.decode_failed()), "https://www.example.com/favicon.ico");
        assert_eq!(fetch_url(fetch.transport_failed()), "https://example.com/apple-touch-icon.png");
        assert_eq!(
            fetch.handle_response(200, None, b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Step::Fetch(Request { url: "https://example.com/favicon.ico".into(), max_bytes: MAX_IMAGE_BYTES })
        );
        assert_eq!(fetch.handle_response(200, None, &[0, 0, 1, 0, 1, 0]), Step::Decode);
        assert_eq!(fetch.decode_failed(), Step::Finished);
    }

    #[test]
    fn rejects_unsafe_redirects_and_oversized_images() {
        let (mut fetch, _) = SiteIconFetch::start("example.com").unwrap();
        assert_eq!(fetch_url(fetch.handle_response(302, Some("http://example.com/"), b"")), "https://example.com/apple-touch-icon.png");
        assert_eq!(fetch_url(fetch.handle_response(302, Some("https://10.0.0.1/a.png"), b"")), "https://example.com/favicon.ico");
        let oversized =
            vec![0x89, b'P', b'N', b'G'].into_iter().chain(std::iter::repeat_n(0, MAX_IMAGE_BYTES as usize)).collect::<Vec<_>>();
        assert_eq!(fetch.handle_response(200, None, &oversized), Step::Finished);

        let (mut fetch, _) = SiteIconFetch::start("example.com").unwrap();
        for hop in 1..=MAX_REDIRECTS {
            assert_eq!(fetch_url(fetch.handle_response(302, Some(&format!("/r{hop}")), b"")), format!("https://example.com/r{hop}"));
        }
        assert_eq!(
            fetch_url(fetch.handle_response(302, Some("/r4"), b"")),
            "https://example.com/apple-touch-icon.png",
            "the fourth redirect fails the home page"
        );
        assert_eq!(SiteIconFetch::start("printer.local").map(|(_, request)| request), None);
    }

    /// Builds an alpha map from rows of `#` (opaque), `+` (half), and `.` (transparent).
    fn alpha_map(rows: &[&str]) -> (Vec<u8>, u32, u32) {
        let alpha = rows
            .iter()
            .flat_map(|row| {
                row.chars().map(|c| match c {
                    '#' => 255,
                    '+' => 128,
                    _ => 0,
                })
            })
            .collect();
        (alpha, rows[0].len() as u32, rows.len() as u32)
    }

    #[test]
    fn tells_tiles_from_glyphs() {
        let square = vec![255u8; 32 * 32];
        assert_eq!(tile_bounds(&square, 32, 32), Some(Bounds { x: 0, y: 0, width: 32, height: 32 }));
        let mut padded_rounded = vec![0u8; 40 * 40];
        for y in 4..36 {
            for x in 4..36 {
                let corner = !(7..=32).contains(&x) && !(7..=32).contains(&y);
                padded_rounded[y * 40 + x] = if corner { 0 } else { 255 };
            }
        }
        assert_eq!(tile_bounds(&padded_rounded, 40, 40), Some(Bounds { x: 4, y: 4, width: 32, height: 32 }), "margin cropped");
        let mut circle = vec![0u8; 40 * 40];
        for y in 0..40 {
            for x in 0..40 {
                let (dx, dy) = (x as f64 - 19.5, y as f64 - 19.5);
                circle[y * 40 + x] = if dx * dx + dy * dy <= 400.0 { 255 } else { 0 };
            }
        }
        assert_eq!(tile_bounds(&circle, 40, 40), None, "a circle is shown inset");
        let mut round_tile = vec![0u8; 96 * 96];
        for y in 0..96 {
            for x in 0..96 {
                let (cx, cy) = ((x as f64).clamp(29.0, 66.0), (y as f64).clamp(29.0, 66.0));
                let (dx, dy) = (x as f64 - cx, y as f64 - cy);
                round_tile[y * 96 + x] = if dx * dx + dy * dy <= 29.5 * 29.5 { 255 } else { 0 };
            }
        }
        assert!(tile_bounds(&round_tile, 96, 96).is_some(), "corners as round as 30% of the side still make a tile");
        let (glyph, w, h) = alpha_map(&[
            "..........",
            "..######..",
            ".##....##.",
            ".#........",
            ".#...####.",
            ".#......#.",
            ".##....##.",
            "..######..",
            "..........",
            "..........",
        ]);
        assert_eq!(tile_bounds(&glyph, w, h), None);
        let wide = vec![255u8; 40 * 20];
        assert_eq!(tile_bounds(&wide, 40, 20), None, "not square");
        let mut small = vec![0u8; 64 * 64];
        for y in 0..20 {
            for x in 0..20 {
                small[y * 64 + x] = 255;
            }
        }
        assert_eq!(tile_bounds(&small, 64, 64), None, "too small within the image");
        assert_eq!(tile_bounds(&[255; 10], 4, 4), None, "length mismatch");
    }

    #[test]
    fn decodes_entities_in_urls() {
        assert_eq!(decode_entities("a&amp;b&#47;c&#x2F;d&bogus;e&"), "a&b/c/d&bogus;e&");
    }
}
