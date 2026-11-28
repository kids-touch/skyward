//! A deliberately small HTML reader for the recovery's built-in browser.
//!
//! This is not a web engine: it extracts readable text, links and raster-image
//! resources without executing JavaScript, CSS, plug-ins or embedded content.

use crate::http::{self, Servers};
use anyhow::{Context, Result, bail};
use std::io::Read;

/// Maximum HTML document accepted by the recovery browser (2 MiB).
pub const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
/// Maximum size of an individual image (8 MiB).
pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum images fetched from one document.
pub const MAX_IMAGES: usize = 12;
/// Maximum number of clickable links retained from one page.
pub const MAX_LINKS: usize = 64;
/// Maximum readable text retained from one page (Unicode scalar values).
pub const MAX_TEXT_CHARS: usize = 16 * 1024;
/// Maximum combined image payload retained for one page (16 MiB).
pub const MAX_TOTAL_IMAGE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserLink {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserImage {
    pub alt: String,
    pub url: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserPage {
    pub url: String,
    pub title: String,
    pub text: String,
    pub links: Vec<BrowserLink>,
    pub images: Vec<BrowserImage>,
}

/// Fetch a page and its first few PNG/JPEG images using the recovery's DNS
/// configuration. Network work is synchronous; callers should run it on a
/// worker, never on the UI/render thread.
pub fn open(url: &str, servers: Servers) -> Result<BrowserPage> {
    let url = normalize_url(url)?;
    let agent = http::agent(servers);
    let mut response = agent.get(&url).call().with_context(|| format!("open {url}"))?;
    let mime = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_ascii_lowercase();
    if !mime.contains("text/html") && !mime.contains("application/xhtml+xml") {
        bail!("页面不是 HTML 文档（{mime}）");
    }
    let mut body = Vec::new();
    response.body_mut().with_config().limit(MAX_DOCUMENT_BYTES as u64).reader()
        .read_to_end(&mut body).context("读取网页")?;
    if body.len() >= MAX_DOCUMENT_BYTES { bail!("网页超过 2 MiB，已停止加载"); }
    let html = String::from_utf8_lossy(&body);
    let mut page = parse_html(&html, &url);
    let mut images = Vec::new();
    let mut total_image_bytes = 0usize;
    for (alt, image_url) in page.images.drain(..).map(|i| (i.alt, i.url)).take(MAX_IMAGES) {
        if let Ok(image) = fetch_image(&agent, &image_url, alt) {
            if total_image_bytes.saturating_add(image.bytes.len()) > MAX_TOTAL_IMAGE_BYTES {
                break;
            }
            total_image_bytes += image.bytes.len();
            images.push(image);
        }
    }
    page.images = images;
    Ok(page)
}

fn fetch_image(agent: &ureq::Agent, url: &str, alt: String) -> Result<BrowserImage> {
    let parsed: ureq::http::Uri = url.parse().context("图片网址格式不正确")?;
    if !matches!(parsed.scheme_str(), Some("http" | "https")) || parsed.host().is_none() {
        bail!("图片只允许从 HTTP/HTTPS 地址读取");
    }
    let mut response = agent.get(url).call().with_context(|| format!("读取图片 {url}"))?;
    let mime = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/jpg") {
        bail!("不支持的图片格式：{mime}");
    }
    let mut bytes = Vec::new();
    response.body_mut().with_config().limit(MAX_IMAGE_BYTES as u64).reader()
        .read_to_end(&mut bytes).context("读取图片内容")?;
    if bytes.len() >= MAX_IMAGE_BYTES { bail!("图片超过 8 MiB"); }
    Ok(BrowserImage { alt, url: url.to_owned(), mime, bytes })
}

pub fn normalize_url(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() { bail!("请输入网址"); }
    let candidate = if input.contains("://") { input.to_owned() } else { format!("https://{input}") };
    let uri: ureq::http::Uri = candidate.parse().context("网址格式不正确")?;
    let scheme = uri.scheme_str().unwrap_or("");
    if !matches!(scheme, "http" | "https") || uri.host().is_none() {
        bail!("只支持 HTTP 和 HTTPS 网页");
    }
    Ok(candidate)
}

/// Resolve an HTML resource URL against the document URL. Data URLs and
/// non-network schemes are intentionally rejected.
pub fn resolve_url(base: &str, target: &str) -> Option<String> {
    let target = target.trim();
    if target.is_empty() || target.starts_with('#') { return None; }
    if target.starts_with("http://") || target.starts_with("https://") { return Some(target.to_owned()); }
    if target.starts_with("//") {
        let scheme = base.split_once("://")?.0;
        return Some(format!("{scheme}:{target}"));
    }
    // Reject javascript:, data:, file: and other non-web schemes.
    if target.split_once(':').is_some_and(|(head, _)| !head.contains('/') && !head.contains('?')) { return None; }
    let (origin, base_path) = split_origin(base)?;
    let (target_path, suffix) = split_suffix(target);
    let path = if target_path.is_empty() {
        base_path
    } else if target_path.starts_with('/') {
        target_path.to_owned()
    } else {
        let directory = base_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        format!("{directory}/{target_path}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part { "" | "." => {}, ".." => { parts.pop(); }, other => parts.push(other) }
    }
    Some(format!("{origin}/{}{}", parts.join("/"), suffix))
}

fn split_suffix(value: &str) -> (&str, &str) {
    let at = value.find(['?', '#']).unwrap_or(value.len());
    (&value[..at], &value[at..])
}

fn split_origin(url: &str) -> Option<(String, String)> {
    let scheme_end = url.find("://")? + 3;
    let host_end = url[scheme_end..].find(['/', '?', '#']).map(|n| scheme_end + n).unwrap_or(url.len());
    let origin = url[..host_end].to_owned();
    let path = if url[host_end..].starts_with('/') {
        url[host_end..].split(['?', '#']).next().unwrap_or("/").to_owned()
    } else {
        "/".to_owned()
    };
    Some((origin, path))
}

/// Extract a simple reading view. Tags and attributes are parsed without
/// executing page content; script/style/template bodies are discarded.
pub fn parse_html(html: &str, base: &str) -> BrowserPage {
    let mut title = String::new();
    let mut text = String::new();
    let mut links = Vec::new();
    let mut images = Vec::new();
    let mut anchor: Option<(String, String)> = None;
    let mut in_title = false;
    let mut hidden_depth = 0usize;
    let mut cursor = 0usize;
    while let Some(open_rel) = html[cursor..].find('<') {
        let open = cursor + open_rel;
        if hidden_depth == 0 { append_text(&html[cursor..open], &mut text, &mut title, in_title, &mut anchor); }
        if html[open..].starts_with("<!--") {
            if let Some(end) = html[open + 4..].find("-->") { cursor = open + 4 + end + 3; continue; }
            break;
        }
        let Some(close_rel) = html[open..].find('>') else { break; };
        let close = open + close_rel;
        let raw = html[open + 1..close].trim();
        let closing = raw.starts_with('/');
        let body = raw.trim_start_matches('/').trim();
        let name = body.split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
        if closing {
            match name.as_str() {
                "script" | "style" | "template" => hidden_depth = hidden_depth.saturating_sub(1),
                "title" => in_title = false,
                "a" => if let Some((url, label)) = anchor.take() {
                    let label = clean_text(&label);
                    if !label.is_empty() && links.len() < MAX_LINKS { links.push(BrowserLink { label, url }); }
                },
                _ => {}
            }
        } else {
            match name.as_str() {
                "script" | "style" | "template" => hidden_depth += 1,
                "title" => in_title = true,
                "br" | "p" | "div" | "li" | "h1" | "h2" | "h3" | "tr" => if hidden_depth == 0 { text.push('\n'); },
                "a" if hidden_depth == 0 => {
                    if let Some(href) = attribute(body, "href").and_then(|v| resolve_url(base, &v)) { anchor = Some((href, String::new())); }
                }
                "img" if hidden_depth == 0 => {
                    let source = attribute(body, "src").or_else(|| attribute(body, "data-src"))
                        .or_else(|| first_srcset(attribute(body, "srcset").as_deref()));
                    if images.len() < MAX_IMAGES {
                        if let Some(src) = source.and_then(|v| resolve_url(base, &v)) {
                            if !images.iter().any(|image: &BrowserImage| image.url == src) {
                                images.push(BrowserImage { alt: attribute(body, "alt").unwrap_or_default(), url: src, mime: String::new(), bytes: Vec::new() });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        cursor = close + 1;
    }
    if hidden_depth == 0 { append_text(&html[cursor..], &mut text, &mut title, in_title, &mut anchor); }
    if let Some((url, label)) = anchor.take() { let label = clean_text(&label); if !label.is_empty() && links.len() < MAX_LINKS { links.push(BrowserLink { label, url }); } }
    let mut readable = clean_text(&text);
    if readable.chars().count() > MAX_TEXT_CHARS {
        readable = readable.chars().take(MAX_TEXT_CHARS).collect();
        readable.push_str("\n\n[内容过长，已截断]");
    }
    links.truncate(MAX_LINKS);
    images.truncate(MAX_IMAGES);
    BrowserPage { url: base.to_owned(), title: clean_text(&title), text: readable, links, images }
}

/// Pick the first URL from a simple `srcset` list. Resolution is left to
/// `resolve_url`; descriptors such as `2x` and `640w` are ignored.
fn first_srcset(srcset: Option<&str>) -> Option<String> {
    srcset?.split(',').next()?.split_whitespace().next().map(str::to_owned)
}

fn append_text(raw: &str, text: &mut String, title: &mut String, in_title: bool, anchor: &mut Option<(String, String)>) {
    let decoded = decode_entities(raw);
    if in_title { title.push_str(&decoded); }
    else {
        text.push_str(&decoded);
        if let Some((_, label)) = anchor.as_mut() { label.push_str(&decoded); }
    }
}

fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    let mut newline = false;
    for c in s.chars() {
        if c == '\n' || c == '\r' {
            space = false;
            while out.ends_with(' ') { out.pop(); }
            if !out.is_empty() && !out.ends_with('\n') { out.push('\n'); }
            newline = true;
        } else if c.is_whitespace() {
            if !newline && !space && !out.is_empty() { out.push(' '); }
            space = true;
        } else {
            if newline && out.ends_with('\n') && c.is_whitespace() { continue; }
            out.push(c);
            space = false;
            newline = false;
        }
    }
    out.trim().to_owned()
}

fn attribute(tag: &str, key: &str) -> Option<String> {
    let bytes = tag.as_bytes(); let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') { i += 1; }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'/' { i += 1; }
        if start == i { i += 1; continue; }
        let name = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() || bytes[i] != b'=' { continue; }
        i += 1; while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i >= bytes.len() { break; }
        let value = if bytes[i] == b'\'' || bytes[i] == b'"' {
            let quote = bytes[i]; i += 1; let start = i;
            while i < bytes.len() && bytes[i] != quote { i += 1; }
            let value = tag[start..i].to_owned(); if i < bytes.len() { i += 1; } value
        } else {
            let start = i; while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' { i += 1; }
            tag[start..i].to_owned()
        };
        if name.eq_ignore_ascii_case(key) { return Some(decode_entities(&value)); }
    }
    None
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 1..];
        let Some(end) = tail.find(';') else { out.push('&'); rest = tail; continue; };
        let entity = &tail[..end];
        let decoded = match entity {
            "nbsp" => Some(' '), "amp" => Some('&'), "lt" => Some('<'),
            "gt" => Some('>'), "quot" => Some('"'), "apos" | "#39" => Some('\''),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32),
            _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(ch) = decoded { out.push(ch); } else { out.push('&'); out.push_str(entity); out.push(';'); }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_readable_content_and_resources() {
        let page = parse_html("<html><head><title>Hello &amp; world</title><script>bad()</script></head><body><h1>Hi</h1><p>Read <a href='/next'>next page</a></p><img src='../a.png' alt='A'></body></html>", "https://example.com/docs/start");
        assert_eq!(page.title, "Hello & world");
        assert!(page.text.contains("Hi"));
        assert!(!page.text.contains("bad"));
        assert_eq!(page.links[0].url, "https://example.com/next");
        assert_eq!(page.images[0].url, "https://example.com/a.png");
    }
    #[test]
    fn keeps_block_boundaries_for_readable_paragraphs() {
        let page = parse_html("<p>First paragraph</p><p>Second paragraph</p>", "https://example.com");
        assert!(page.text.contains('\n'));
        assert!(page.text.contains("First paragraph"));
        assert!(page.text.contains("Second paragraph"));
    }

    #[test]
    fn only_http_schemes_are_accepted() {
        assert_eq!(normalize_url("example.com").unwrap(), "https://example.com");
        assert!(normalize_url("file:///etc/passwd").is_err());
    }
    #[test]
    fn resolves_query_only_and_parent_relative_urls() {
        assert_eq!(resolve_url("https://example.com/a/b.html", "?q=1").as_deref(), Some("https://example.com/a/b.html?q=1"));
        assert_eq!(resolve_url("https://example.com/a/b.html", "../img/a.png").as_deref(), Some("https://example.com/img/a.png"));
    }

    #[test]
    fn reads_lazy_images_and_numeric_entities_but_skips_comments() {
        let page = parse_html("<!-- <img src='bad.png'> --><p>&#20013;&#x6587; &amp; ok</p><img data-src='/lazy.png' alt='photo'>", "https://example.com/index.html");
        assert!(page.text.contains("中文 & ok"));
        assert_eq!(page.images.len(), 1);
        assert_eq!(page.images[0].url, "https://example.com/lazy.png");
    }

    #[test]
    fn decodes_apostrophe_entities_and_uses_srcset_fallback() {
        let page = parse_html(
            r#"<p>it&#39;s&nbsp;ok</p><img srcset="/small.jpg 1x, /large.jpg 2x" alt="it&#39;s a photo">"#,
            "https://example.com/page",
        );
        assert!(page.text.contains("it's ok"));
        assert_eq!(page.images[0].url, "https://example.com/small.jpg");
        assert_eq!(page.images[0].alt, "it's a photo");
    }

    #[test]
    fn caps_page_text_links_and_images_for_recovery_memory() {
        let mut html = String::from("<body>");
        for n in 0..(MAX_LINKS + 20) {
            html.push_str(&format!("<a href='/p{n}'>link{n}</a>"));
        }
        for n in 0..(MAX_IMAGES + 20) {
            html.push_str(&format!("<img src='/i{n}.png'>"));
        }
        html.push_str(&format!("<p>{}</p></body>", "x".repeat(MAX_TEXT_CHARS + 100)));
        let page = parse_html(&html, "https://example.com/");
        assert_eq!(page.links.len(), MAX_LINKS);
        assert_eq!(page.images.len(), MAX_IMAGES);
        assert!(page.text.chars().count() <= MAX_TEXT_CHARS + "\n\n[内容过长，已截断]".chars().count());
        assert!(page.text.ends_with("[内容过长，已截断]"));
    }

}
