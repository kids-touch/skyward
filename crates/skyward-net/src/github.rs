//! Skyward's own releases on GitHub: which is the newest, and the zip in
//! it for one phone, as the release workflow names each phone's.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;

/// Where GitHub answers which release of Skyward is the newest.
pub const LATEST: &str = "https://api.github.com/repos/kids-touch/skyward/releases?per_page=1";

/// The version of GitHub's API the answer is read as.
const API_VERSION: &str = "2022-11-28";

/// The newest release, as the build of Skyward that asked sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newest {
    /// Its version, such as 0.1.4.
    pub version: String,
    /// It is newer than the build that asked.
    pub newer: bool,
    /// Its zip for the phone; none when it has none.
    pub build: Option<Build>,
}

/// A phone's zip in a release, which holds Skyward's image for the phone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    /// The zip's own name, such as
    /// skyward-v0.1.4-ishtar-OS3.0.308.0.WMACNXM.zip.
    pub name: String,
    /// Where GitHub's API hands it out, which [`locate`] asks for where the
    /// file itself is.
    pub url: String,
    /// The release's own link to it on github.com.
    pub link: String,
    pub size: u64,
    /// Its SHA-256 as GitHub gives it, in hex.
    pub sha256: String,
}

impl Build {
    /// Where to download the zip from, in the order to try: the file's own
    /// short-lived address, where GitHub's API gave one, then the release's
    /// link on github.com, which some networks do not reach.
    pub fn sources(&self, located: Option<String>) -> Vec<String> {
        located.into_iter().chain([self.link.clone()]).collect()
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

/// Why GitHub turned a question down, as it says.
#[derive(Deserialize)]
struct Message {
    message: String,
}

/// The newest release returned by GitHub, including pre-releases. The
/// release title carries the Cargo version; the tag identifies the build
/// commit and is part of every uploaded ZIP's name.
pub fn newest(answer: &str, codename: &str, running: &str) -> Result<Newest> {
    let mut releases: Vec<Release> = serde_json::from_str(answer).context("read GitHub's releases")?;
    let release = releases.drain(..).next().context("GitHub has no published releases")?;
    let tag = release.tag_name;
    let version = release.name.split_whitespace().last().unwrap_or(&tag);
    let version = version.strip_prefix('v').unwrap_or(version);
    let newest = semver::Version::parse(version)
        .with_context(|| format!("the newest release is named {:?}, which does not end in a version", release.name))?;
    let this = semver::Version::parse(running).with_context(|| format!("this build's version is {running}"))?;
    // Test releases use tags such as test-e0e0be4, so the ZIP name is
    // matched against the tag, not the semantic version in the title.
    let start = format!("skyward-{tag}-{codename}-");
    let build = match release.assets.into_iter().find(|a| a.name.starts_with(&start) && a.name.ends_with(".zip")) {
        None => None,
        Some(a) => {
            let sha256 = a.digest.as_deref().and_then(|d| d.strip_prefix("sha256:"))
                .with_context(|| format!("GitHub gives no SHA-256 of {}", a.name))?;
            ensure!(sha256.len() == 64 && sha256.bytes().all(|b| b.is_ascii_hexdigit()), "GitHub gives {} the SHA-256 {sha256}", a.name);
            Some(Build { sha256: sha256.to_lowercase(), name: a.name, url: a.url, link: a.browser_download_url, size: a.size })
        }
    };
    // A matching build of the same version is offered too: test builds
    // deliberately keep the Cargo version while their Git commit changes.
    let newer = newest >= this && build.is_some();
    Ok(Newest { version: version.to_string(), newer, build })
}

/// Ask GitHub for Skyward's newest release, and its zip for the phone
/// `codename` names, as the build of version `running`.
pub fn ask(agent: &ureq::Agent, codename: &str, running: &str) -> Result<Newest> {
    let mut response = agent
        .get(LATEST)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", format!("Skyward/{running}"))
        .call()
        .with_context(|| format!("ask {LATEST}"))?;
    let status = response.status();
    let body = response.body_mut().read_to_string().with_context(|| format!("read the answer of {LATEST}"))?;
    if !status.is_success() {
        bail!(refusal(status, &body));
    }
    newest(&body, codename, running)
}

/// Where the file a build's `url` hands out is now, as GitHub's API sends
/// the build of version `running` that asks for it on: a short-lived
/// address of the file itself, away from github.com.
pub fn locate(agent: &ureq::Agent, url: &str, running: &str) -> Result<String> {
    let mut response = agent
        .get(url)
        .header("Accept", "application/octet-stream")
        .header("X-GitHub-Api-Version", API_VERSION)
        .header("User-Agent", format!("Skyward/{running}"))
        .config()
        .max_redirects(0)
        .build()
        .call()
        .with_context(|| format!("ask {url}"))?;
    let status = response.status();
    if status.is_redirection() {
        let location = response.headers().get(ureq::http::header::LOCATION).with_context(|| format!("GitHub answered {status}, and names no address"))?;
        return Ok(location.to_str().context("GitHub names an address that is not text")?.to_string());
    }
    ensure!(!status.is_success(), "GitHub sends the file itself in its answer, at no address of its own");
    let body = response.body_mut().read_to_string().with_context(|| format!("read the answer of {url}"))?;
    bail!(refusal(status, &body))
}

/// Why GitHub turned a question down, as it says, as when too many questions
/// came from the address that asked.
fn refusal(status: ureq::http::StatusCode, body: &str) -> String {
    match serde_json::from_str::<Message>(body) {
        Ok(m) => format!("GitHub answered {status}: {}", m.message),
        Err(_) => format!("GitHub answered {status}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"[{"tag_name":"test-e0e0be4","name":"Skyward Recovery 1.0.0-Beta","assets":[{"url":"https://api.github.com/repos/kids-touch/skyward/releases/assets/123","name":"skyward-e0e0be4-shennong-OS3.0.309.0.WNBCNXM.zip","size":42739608,"digest":"sha256:62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534","browser_download_url":"https://github.com/kids-touch/skyward/releases/download/test-e0e0be4/skyward-e0e0be4-shennong-OS3.0.309.0.WNBCNXM.zip"}]}]"#;

    #[test]
    fn newest_includes_pre_releases_and_matches_the_phone_zip_by_tag() {
        let n = newest(ANSWER, "shennong", "1.0.0-Beta").unwrap();
        assert!(n.newer);
        assert_eq!(n.version, "1.0.0-Beta");
        let build = n.build.unwrap();
        assert_eq!(build.name, "skyward-e0e0be4-shennong-OS3.0.309.0.WNBCNXM.zip");
        assert_eq!(build.size, 42739608);
        assert_eq!(build.sha256, "62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534");
        assert_eq!(build.link, "https://github.com/kids-touch/skyward/releases/download/test-e0e0be4/skyward-e0e0be4-shennong-OS3.0.309.0.WNBCNXM.zip");
    }

    #[test]
    fn no_zip_for_this_phone_means_no_update() {
        let n = newest(ANSWER, "ishtar", "1.0.0-Beta").unwrap();
        assert!(!n.newer);
        assert!(n.build.is_none());
    }

    #[test]
    fn a_release_must_have_a_version_and_a_zip_must_have_a_digest() {
        assert!(newest("[]", "shennong", "1.0.0-Beta").is_err());
        assert!(newest(ANSWER, "shennong", "1.0").is_err());
        let no_digest = ANSWER.replace("sha256:62b66f7d13a0eb6c9af3359b5ec6f9c223fd9a86b1f8abab3e9d2646cb2ce534", "md5:bad");
        assert!(newest(&no_digest, "shennong", "1.0.0-Beta").is_err());
    }

    #[test]
    fn downloads_from_github_s_asset_location_then_its_release_link() {
        let build = newest(ANSWER, "shennong", "1.0.0-Beta").unwrap().build.unwrap();
        assert_eq!(build.sources(None), [build.link.clone()]);
        assert_eq!(build.sources(Some("https://release-assets.githubusercontent.com/a?sig=1".into())), [
            "https://release-assets.githubusercontent.com/a?sig=1",
            build.link.as_str()
        ]);
    }
}
