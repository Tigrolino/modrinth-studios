//! Modrinth Studios addition: "client installations".
//!
//! An ordinary instance in the library that, instead of plain Minecraft,
//! starts a third-party client: Horizon (Minecraft rewritten in Rust),
//! NoRisk Client (a Fabric mod set), Lunar Client or LabyMod 4.
//!
//! The instance itself is created the normal way (vanilla, or Fabric for
//! NoRisk) so Modrinth downloads the game files, Java, assets and so on. What
//! makes it a client installation is a small sidecar file,
//! `<instance>/.studio/client.json`. `run_credentials` checks for it and hands
//! the launch to [`launch`] here, which either adds a few JVM arguments to the
//! normal launch (NoRisk) or starts the client's own process (the others) and
//! registers it with the process manager, so playtime, logs, the Stop button
//! and "running" state work like any other instance.
//!
//! Client files live in `<meta>/studio_clients/<client>/`.
//!
//! None of these clients publish an official API for third-party launchers.
//! Horizon and NoRisk are open source and their own launchers were used as the
//! reference; Lunar and LabyMod use the same endpoints their launchers use,
//! which can change without notice.

mod horizon;
mod import;
mod labymod;
mod lunar;
mod norisk;

pub use import::{ImportItem, ImportSource, import_from, import_sources};
pub use horizon::{
    HorizonProject, horizon_store_install, horizon_store_list,
    horizon_store_remove,
};

use crate::State;
use crate::state::{Credentials, InstanceLaunchContext, ProcessMetadata};
use anyhow::{Context, bail};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use tokio::process::Command;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientKind {
    Horizon,
    NoRisk,
    Lunar,
    LabyMod,
}

impl ClientKind {
    pub fn display_name(self) -> &'static str {
        match self {
            ClientKind::Horizon => "Horizon",
            ClientKind::NoRisk => "NoRisk Client",
            ClientKind::Lunar => "Lunar Client",
            ClientKind::LabyMod => "LabyMod",
        }
    }

    /// GitHub organisation whose avatar is the client's logo.
    pub fn github_org(self) -> &'static str {
        match self {
            ClientKind::Horizon => "horizon-rust",
            ClientKind::NoRisk => "NoRiskClient",
            ClientKind::Lunar => "LunarClient",
            ClientKind::LabyMod => "LabyMod",
        }
    }

    fn dir_name(self) -> &'static str {
        match self {
            ClientKind::Horizon => "horizon",
            ClientKind::NoRisk => "norisk",
            ClientKind::Lunar => "lunar",
            ClientKind::LabyMod => "labymod",
        }
    }
}

/// One choice in the "pick a version" list.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientVersion {
    /// Opaque id passed back to [`setup`].
    pub id: String,
    /// What the user sees, e.g. "1.8.9" or "Nightly 2026-11-07".
    pub name: String,
    /// Minecraft version the instance is created with.
    pub game_version: String,
    /// "vanilla" or "fabric": the loader the instance is created with.
    pub loader: String,
    /// Extra line under the name (module, pack, channel...).
    pub detail: Option<String>,
    pub recommended: bool,
}

/// What's saved in `<instance>/.studio/client.json`.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ClientInstall {
    pub client: ClientKind,
    pub version_id: String,
    pub version_name: String,
    #[serde(default)]
    pub variant_name: Option<String>,
    pub game_version: String,
    /// Set once the client's files have been downloaded at least once.
    #[serde(default)]
    pub prepared: bool,
    /// When the files were last checked for updates (Unix seconds).
    #[serde(default)]
    pub checked_at: i64,
}

fn sidecar_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".studio").join("client.json")
}

pub(crate) async fn client_root(kind: ClientKind) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let dir = state
        .directories
        .metadata_dir()
        .join("studio_clients")
        .join(kind.dir_name());
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// The client installed in this instance, if any.
pub async fn get(instance_id: &str) -> crate::Result<Option<ClientInstall>> {
    let dir = crate::api::instance::get_full_path(instance_id).await?;
    read_sidecar(&dir).await
}

async fn read_sidecar(dir: &Path) -> crate::Result<Option<ClientInstall>> {
    match tokio::fs::read(sidecar_path(dir)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .context("The client installation settings are damaged"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn write_sidecar(
    dir: &Path,
    install: &ClientInstall,
) -> crate::Result<()> {
    let path = sidecar_path(dir);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_vec_pretty(install)?).await?;
    Ok(())
}

/// Versions the user can pick for a client.
pub async fn list_versions(
    kind: ClientKind,
) -> crate::Result<Vec<ClientVersion>> {
    match kind {
        ClientKind::Horizon => horizon::list_versions().await,
        ClientKind::NoRisk => {
            let creds = default_credentials().await?;
            norisk::list_versions(&creds).await
        }
        ClientKind::Lunar => lunar::list_versions().await,
        ClientKind::LabyMod => labymod::list_versions().await,
    }
}

/// A news post or patch note shown on the instance's client tab.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NewsItem {
    pub title: String,
    pub date: Option<String>,
    pub url: Option<String>,
    pub image: Option<String>,
    /// One or two lines of plain text.
    pub summary: Option<String>,
    /// Markdown (patch notes); empty for posts that only link out.
    #[serde(default)]
    pub body: String,
}

/// Recent news for a client, from wherever that client publishes it.
pub async fn news(kind: ClientKind) -> crate::Result<Vec<NewsItem>> {
    match kind {
        ClientKind::Horizon => horizon::news().await,
        ClientKind::NoRisk => norisk::news().await,
        ClientKind::Lunar => {
            site_news(
                "https://www.lunarclient.com/news",
                "https://www.lunarclient.com",
                "/news/",
            )
            .await
        }
        ClientKind::LabyMod => {
            site_news(
                "https://www.labymod.net/en/blog",
                "https://www.labymod.net",
                "/blog/",
            )
            .await
        }
    }
}

/// News from a client's website. Tries the page's embedded Next.js data
/// first (posts with a title and a slug), then plain links under `path`.
async fn site_news(
    page: &str,
    origin: &str,
    path: &str,
) -> crate::Result<Vec<NewsItem>> {
    let response = HTTP
        .get(page)
        .timeout(Duration::from_secs(30))
        .send()
        .await?
        .error_for_status()?;
    // Sites move their blogs (labymod.net/en/blog now redirects to laby.net),
    // so links and images are resolved against where we actually ended up.
    let final_origin = {
        let url = response.url();
        format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default())
    };
    let origin = if final_origin.ends_with("://") { origin } else { final_origin.as_str() };
    let raw = response.text().await?;
    let mut out = scrape_posts(&raw, origin, path);
    out.truncate(12);

    // Posts found without a picture or summary: read them from each post's
    // own page (its og:image / og:description), a few at a time.
    let missing: Vec<usize> = out
        .iter()
        .enumerate()
        .filter(|(_, n)| n.image.is_none() || n.summary.is_none() || n.date.is_none())
        .map(|(i, _)| i)
        .take(12)
        .collect();
    let urls: Vec<(usize, String)> = missing
        .into_iter()
        .filter_map(|i| out[i].url.clone().map(|u| (i, u)))
        .collect();
    let metas: Vec<(usize, PageMeta)> = futures::stream::iter(urls)
        .map(|(i, url)| async move { (i, page_meta(&url).await) })
        .buffer_unordered(4)
        .collect()
        .await;
    for (i, meta) in metas {
        let item = &mut out[i];
        if item.image.is_none() {
            item.image = meta
                .image
                .and_then(|u| absolute_url(&u, origin))
                .filter(|u| !looks_like_logo(u));
        }
        if item.summary.is_none() {
            item.summary = meta.description.filter(|d| !looks_like_code(d));
        }
        if item.date.is_none() {
            item.date = meta.published;
        }
    }
    Ok(tidy_news(out))
}

/// Posts found in a news page: its Next.js data first, else links under `path`.
fn scrape_posts(raw: &str, origin: &str, path: &str) -> Vec<NewsItem> {
    let mut out: Vec<NewsItem> = Vec::new();
    // Next.js data lives in a <script>, so it's read from the raw page; the
    // link scraper below only sees the page with scripts removed, so inline
    // JS (`self.__next_f.push(...)`) can never become a card.
    let html = raw;
    let visible = strip_blocks(raw, &["script", "style", "noscript", "svg"]);

    if let Some(start) = html.find("id=\"__NEXT_DATA__\"")
        && let Some(open) = html[start..].find('>')
        && let Some(close) = html[start + open..].find("</script>")
        && let Ok(data) = serde_json::from_str::<serde_json::Value>(
            &html[start + open + 1..start + open + close],
        )
    {
        if let Some(items) = data
            .pointer("/props/pageProps/story/items")
            .and_then(|v| v.as_array())
        {
            for item in items {
                if let (Some(title), Some(slug)) = (
                    item.pointer("/content/title").and_then(|v| v.as_str()),
                    item.get("slug").and_then(|v| v.as_str()),
                ) {
                    out.push(NewsItem {
                        title: title.to_string(),
                        url: Some(format!("{origin}{path}{slug}")),
                        image: item
                            .pointer("/content/image/filename")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                        date: item
                            .get("first_published_at")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                        ..Default::default()
                    });
                }
            }
        }
        if out.is_empty() {
            collect_posts(&data, origin, path, &mut out);
        }
    }

    if out.is_empty() {
        // <a href="/news/slug" ...>Title</a>
        let mut rest = visible.as_str();
        while let Some(i) = rest.find("href=\"") {
            rest = &rest[i + 6..];
            let Some(end) = rest.find('"') else { break };
            let href = &rest[..end];
            let full = if href.starts_with('/') {
                format!("{origin}{href}")
            } else {
                href.to_string()
            };
            if !full.starts_with(origin)
                || !full.contains(path)
                || full
                    .trim_end_matches('/')
                    .ends_with(path.trim_end_matches('/'))
            {
                continue;
            }
            let Some(close) = rest.find("</a>") else {
                break;
            };
            let Some(open) = rest[..close].find('>') else {
                continue;
            };
            let body = &rest[open + 1..close];
            let heading = ["h1", "h2", "h3"].iter().find_map(|tag| {
                let start = body.find(&format!("<{tag}"))?;
                let content = &body[start..];
                let open = content.find('>')?;
                let end = content.find(&format!("</{tag}>"))?;
                Some(&content[open + 1..end])
            });
            let text = strip_tags(heading.unwrap_or(body));
            let text = text
                .trim_end_matches("Read Article")
                .trim_end_matches(['\u{2014}', '-', ' '])
                .to_string();
            let image = body.find("<img").and_then(|start| {
                let img = &body[start..body[start..].find('>').map_or(body.len(), |e| start + e)];
                // Lazy-loaded images keep the real URL in data-src or srcset.
                ["data-src=\"", "srcset=\"", "src=\""].iter().find_map(|attr| {
                    let at = img.find(attr)? + attr.len();
                    let end = img[at..].find('"')?;
                    let value = img[at..at + end].split([' ', ',']).next()?.trim();
                    absolute_url(value, origin)
                })
            });
            let image = image.filter(|u| !looks_like_logo(u));
            if text.len() < 4
                || text.len() > 200
                || looks_like_code(&text)
                || out.iter().any(|n| n.url.as_deref() == Some(full.as_str()))
            {
                continue;
            }
            out.push(NewsItem {
                title: text,
                image,
                url: Some(full),
                ..Default::default()
            });
            if out.len() >= 12 {
                break;
            }
        }
    }
    out
}

/// Drops cards that aren't real posts (logos, banners, code) and pictures
/// that are just the site's default share image.
fn tidy_news(mut out: Vec<NewsItem>) -> Vec<NewsItem> {
    // A picture used by several posts is the site's default share image
    // (usually its logo), not a cover: show the card without it.
    let shared: Vec<String> = out
        .iter()
        .filter_map(|n| n.image.clone())
        .filter(|img| out.iter().filter(|n| n.image.as_ref() == Some(img)).count() > 2)
        .collect();
    for item in &mut out {
        if item.image.as_ref().is_some_and(|i| shared.contains(i)) {
            item.image = None;
        }
    }
    out.retain(|n| !looks_like_code(&n.title));
    // A bare short link like "LABY" (a logo or nav link) isn't a post.
    out.retain(|n| {
        n.title.chars().count() >= 16 || n.image.is_some() || n.summary.is_some()
    });
    // Scraped site news: a real post has a picture or a summary. Banners and
    // promos (e.g. a sale with a countdown) come through as one run of glued
    // text with neither, so they're left out.
    let scraped = out.iter().any(|n| n.body.is_empty());
    if scraped {
        out.retain(|n| {
            let glued = n.title.contains("--:--")
                || n.title.as_bytes().windows(2).any(|w| {
                    w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase()
                }) && n.title.len() > 60;
            !glued && (n.image.is_some() || n.summary.is_some())
        });
    }
    out
}

/// Removes `<tag ...>...</tag>` blocks (scripts, styles, inline SVG).
fn strip_blocks(html: &str, tags: &[&str]) -> String {
    let mut out = html.to_string();
    for tag in tags {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(start) = rest.find(&open) {
            result.push_str(&rest[..start]);
            match rest[start..].find(&close) {
                Some(end) => rest = &rest[start + end + close.len()..],
                None => {
                    rest = "";
                    break;
                }
            }
        }
        result.push_str(rest);
        out = result;
    }
    out
}

fn looks_like_code(text: &str) -> bool {
    ["self.__next", "function(","=>", "{\"", "});", "window.", "document."]
        .iter()
        .any(|m| text.contains(m))
}

fn looks_like_logo(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or_default();
    file.contains("logo")
        || file.contains("favicon")
        || file.contains("icon-")
        || file.starts_with("og-default")
        || lower.ends_with(".svg")
}

fn absolute_url(value: &str, origin: &str) -> Option<String> {
    let value = value.trim();
    if value.starts_with("https://") {
        Some(value.to_string())
    } else if value.starts_with("//") {
        Some(format!("https:{value}"))
    } else if value.starts_with('/') {
        Some(format!("{origin}{value}"))
    } else {
        None
    }
}

#[derive(Default)]
struct PageMeta {
    image: Option<String>,
    description: Option<String>,
    published: Option<String>,
}

/// og:image, og:description and the publish date from a page's <head>.
async fn page_meta(url: &str) -> PageMeta {
    let Ok(res) = HTTP.get(url).timeout(Duration::from_secs(15)).send().await else {
        return PageMeta::default();
    };
    let Ok(html) = res.text().await else {
        return PageMeta::default();
    };
    let mut cut = html.find("</head>").unwrap_or(html.len()).min(400_000);
    while !html.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &html[..cut];
    let meta = |names: &[&str]| -> Option<String> {
        let mut rest = head;
        while let Some(i) = rest.find("<meta") {
            rest = &rest[i + 5..];
            let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
            let attr = |name: &str| -> Option<&str> {
                let key = format!("{name}=\"");
                let at = tag.find(&key)? + key.len();
                let end = tag[at..].find('"')?;
                Some(&tag[at..at + end])
            };
            let key = attr("property").or_else(|| attr("name")).unwrap_or_default();
            if names.contains(&key)
                && let Some(content) = attr("content")
                && !content.is_empty()
            {
                return Some(content.to_string());
            }
        }
        None
    };
    PageMeta {
        image: meta(&["og:image", "twitter:image"]),
        description: meta(&["og:description", "description", "twitter:description"])
            .map(|d| strip_tags(&d)),
        published: meta(&["article:published_time", "og:published_time"]),
    }
}

fn collect_posts(
    value: &serde_json::Value,
    origin: &str,
    path: &str,
    out: &mut Vec<NewsItem>,
) {
    if out.len() >= 12 {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            let title = map.get("title").and_then(|t| {
                t.as_str()
                    .map(String::from)
                    .or_else(|| t["rendered"].as_str().map(String::from))
            });
            let slug = map.get("slug").and_then(|s| s.as_str());
            if let (Some(title), Some(slug)) = (title, slug) {
                let url = format!("{origin}{path}{slug}");
                if !out.iter().any(|n| n.url.as_deref() == Some(url.as_str())) {
                    let pick = |keys: &[&str]| -> Option<String> {
                        keys.iter().find_map(|k| match map.get(*k) {
                            Some(serde_json::Value::String(s))
                                if !s.is_empty() =>
                            {
                                Some(s.clone())
                            }
                            Some(serde_json::Value::Object(o)) => o
                                .get("url")
                                .and_then(|u| u.as_str())
                                .map(String::from),
                            _ => None,
                        })
                    };
                    out.push(NewsItem {
                        title: strip_tags(&title),
                        date: pick(&[
                            "date",
                            "publishedAt",
                            "published_at",
                            "createdAt",
                            "created_at",
                        ]),
                        image: pick(&[
                            "image",
                            "thumbnail",
                            "cover",
                            "coverImage",
                            "banner",
                            "featuredImage",
                        ]),
                        summary: pick(&[
                            "excerpt",
                            "description",
                            "summary",
                            "subtitle",
                        ])
                        .map(|s| strip_tags(&s)),
                        url: Some(url),
                        body: String::new(),
                    });
                }
                return;
            }
            for v in map.values() {
                collect_posts(v, origin, path, out);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                collect_posts(v, origin, path, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&#8217;", "'")
        .replace("&#8211;", "-")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Modrinth Studios news: an article or index from GitHub, fetched here so
/// the webview's CSP/CORS never gets in the way. Only the two article
/// sources are allowed.
pub async fn news_text(url: &str) -> crate::Result<String> {
    const ALLOWED: &[&str] = &[
        "https://raw.githubusercontent.com/Tigrolino/modrinth-studios/",
        "https://raw.githubusercontent.com/modrinth/code/main/packages/blog/",
    ];
    if !ALLOWED.iter().any(|p| url.starts_with(p)) || url.contains("..") {
        bail!("That address isn't a news source");
    }
    let res = HTTP
        .get(url)
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .with_context(|| format!("requesting {url}"))?;
    let status = res.status();
    if !status.is_success() {
        bail!("{url} returned HTTP {status}");
    }
    Ok(res.text().await?)
}

/// A video (or other media) of a Modrinth news article, for the in-app
/// article viewer, which plays it from memory.
pub async fn news_media(url: &str) -> crate::Result<Vec<u8>> {
    if !url.starts_with("https://modrinth.com/news/") || url.contains("..") {
        bail!("That address isn't a news file");
    }
    Ok(get_bytes(url).await?.to_vec())
}

/// Downloads the client's logo once and returns its path, for the instance icon.
pub async fn icon(kind: ClientKind) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let path = state
        .directories
        .metadata_dir()
        .join("studio_clients")
        .join("icons")
        .join(format!("{}-v2.png", kind.dir_name()));
    if !path.exists() {
        let url = if kind == ClientKind::NoRisk {
            "https://norisk.gg/icon-192.png".to_string()
        } else {
            format!("https://github.com/{}.png?size=256", kind.github_org())
        };
        let bytes = get_bytes(&url).await?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, bytes).await?;
    }
    Ok(path)
}

pub use norisk::{
    NoRiskCape, NoRiskCapePage, NoRiskMod, cape_equip as norisk_cape_equip,
    cape_texture as norisk_cape_texture, cape_unequip as norisk_cape_unequip,
    cape_upload as norisk_cape_upload, capes_browse as norisk_capes_browse,
    capes_mine as norisk_capes_mine, pack_mods as norisk_pack_mods,
    sign_in as norisk_sign_in, signed_in as norisk_signed_in,
};

/// Marks a freshly created instance as a client installation and starts
/// downloading the client's files in the background.
pub async fn setup(
    instance_id: &str,
    kind: ClientKind,
    version: ClientVersion,
) -> crate::Result<ClientInstall> {
    if version.id.split('|').any(|part| !safe_file_name(part))
        || !safe_file_name(&version.game_version)
    {
        bail!("Invalid client version");
    }
    let dir = crate::api::instance::get_full_path(instance_id).await?;
    let install = ClientInstall {
        client: kind,
        version_id: version.id,
        version_name: version.name,
        variant_name: version.detail,
        game_version: version.game_version,
        prepared: false,
        checked_at: 0,
    };
    write_sidecar(&dir, &install).await?;
    // The frontend starts the download right after (so it shows in the
    // download manager); launching downloads anything still missing.
    Ok(install)
}

pub(super) fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
                )
        })
}

static PREPARE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Downloads (or updates) the client's files. Safe to call repeatedly:
/// everything already in place is skipped.
pub async fn prepare(instance_id: &str) -> crate::Result<()> {
    let _guard = PREPARE_LOCK.lock().await;
    let state = State::get().await?;
    let context =
        crate::state::instances::commands::get_instance_launch_context(
            instance_id,
            &state.pool,
        )
        .await?
        .context("Instance not found")?;
    if context.instance.install_stage
        != crate::state::InstanceInstallStage::Installed
    {
        bail!(
            "Wait for Minecraft to finish installing before downloading client files"
        );
    }
    if crate::state::instance_has_running_process(instance_id, &state).await? {
        bail!("Stop the instance before updating client files");
    }
    let dir = crate::api::instance::get_full_path(instance_id).await?;
    let Some(mut install) = read_sidecar(&dir).await? else {
        return Ok(());
    };
    let name = instance_name(instance_id).await.unwrap_or_default();
    let bar = crate::event::emit::init_loading(
        crate::event::LoadingBarType::MinecraftDownload {
            instance_id: instance_id.to_string(),
            instance_name: name,
        },
        100.0,
        &format!("Preparing {}", install.client.display_name()),
    )
    .await
    .ok();
    let progress = Progress { bar: bar.as_ref() };

    let result = match install.client {
        ClientKind::Horizon => {
            horizon::prepare(&install, &dir, &progress).await
        }
        ClientKind::NoRisk => {
            let creds = default_credentials().await?;
            norisk::prepare(&install, &dir, &creds, &progress).await
        }
        ClientKind::Lunar => lunar::prepare(&install, &progress).await,
        ClientKind::LabyMod => {
            labymod::prepare(&install, &dir, &progress).await
        }
    };
    result?;
    progress.finish();

    install.prepared = true;
    install.checked_at = chrono::Utc::now().timestamp();
    write_sidecar(&dir, &install).await?;
    Ok(())
}

pub(crate) struct Progress<'a> {
    bar: Option<&'a crate::event::LoadingBarId>,
}

impl Progress<'_> {
    /// `fraction` is the share of the whole bar this step adds (0..100).
    pub(crate) fn step(&self, amount: f64, message: &str) {
        if let Some(bar) = self.bar {
            let _ =
                crate::event::emit::emit_loading(bar, amount, Some(message));
        }
    }

    fn finish(&self) {
        if let Some(bar) = self.bar {
            let _ = crate::event::emit::emit_loading(bar, 100.0, Some("Done"));
        }
    }
}

async fn instance_name(instance_id: &str) -> Option<String> {
    let state = State::get().await.ok()?;
    crate::state::instances::adapters::sqlite::instance_rows::get_instance_by_id(
        instance_id,
        &state.pool,
    )
    .await
    .ok()
    .flatten()
    .map(|i| i.name)
}

async fn default_credentials() -> crate::Result<Credentials> {
    let state = State::get().await?;
    Ok(Credentials::get_default_credential(&state.pool)
        .await?
        .ok_or(crate::ErrorKind::NotLoggedIn)?)
}

/// Everything `run_credentials` already worked out for a launch.
pub(crate) struct LaunchSettings<'a> {
    pub java_args: &'a [String],
    pub env_args: &'a [(String, String)],
    pub wrapper: &'a Option<String>,
    pub memory: crate::state::MemorySettings,
    pub resolution: crate::state::WindowSize,
    pub post_exit_hook: Option<String>,
}

/// What `run_credentials` should do with an instance.
pub(crate) enum LaunchPlan {
    /// Not a client installation, launch normally.
    Normal,
    /// Launch normally, with these extra JVM arguments (NoRisk).
    ExtraJvmArgs(Vec<String>),
    /// The client was started here.
    Started(ProcessMetadata),
}

pub(crate) async fn launch(
    context: &InstanceLaunchContext,
    credentials: &Credentials,
    settings: LaunchSettings<'_>,
) -> crate::Result<LaunchPlan> {
    let state = State::get().await?;
    let instance_dir = state
        .directories
        .instances_dir()
        .join(&context.instance.path);
    let Some(install) = read_sidecar(&instance_dir).await? else {
        return Ok(LaunchPlan::Normal);
    };

    if context.instance.install_stage
        != crate::state::InstanceInstallStage::Installed
    {
        bail!(crate::ErrorKind::LauncherError(
            "instance is still installing".to_string()
        ));
    }
    if crate::state::instance_has_running_process(&context.instance.id, &state)
        .await?
    {
        bail!(crate::ErrorKind::LauncherError(format!(
            "instance {} is already running",
            context.instance.id
        )));
    }

    // Bring files up to date, at most every few hours so pressing Play stays
    // fast. A failure here is only fatal when nothing was ever downloaded.
    let elapsed = chrono::Utc::now().timestamp() - install.checked_at;
    let fresh = install.prepared && (0..6 * 3600).contains(&elapsed);
    if !fresh && let Err(e) = prepare(&context.instance.id).await {
        if !install.prepared {
            return Err(e).context(format!(
                "couldn't download {}",
                install.client.display_name()
            ));
        }
        tracing::warn!(
            "Updating {} failed, launching what's there: {e:#}",
            install.client.display_name()
        );
    }

    let command = match install.client {
        ClientKind::NoRisk => {
            let args =
                norisk::jvm_args(&install, &instance_dir, credentials).await?;
            return Ok(LaunchPlan::ExtraJvmArgs(args));
        }
        ClientKind::Horizon => {
            horizon::command(&install, &instance_dir, credentials, &settings)
                .await?
        }
        ClientKind::Lunar => {
            lunar::command(&install, &instance_dir, credentials, &settings)
                .await?
        }
        ClientKind::LabyMod => {
            labymod::command(&install, &instance_dir, credentials, &settings)
                .await?
        }
    };

    let metadata =
        start_process(context, install.client, command, &settings, &state)
            .await?;
    Ok(LaunchPlan::Started(metadata))
}

async fn start_process(
    context: &InstanceLaunchContext,
    kind: ClientKind,
    mut command: Command,
    settings: &LaunchSettings<'_>,
    state: &std::sync::Arc<State>,
) -> crate::Result<ProcessMetadata> {
    let instance = &context.instance;
    command.env_remove("_JAVA_OPTIONS");
    command.envs(settings.env_args.iter().cloned());

    crate::state::instances::commands::set_instance_last_played(
        &instance.id,
        chrono::Utc::now(),
        &state.pool,
    )
    .await?;

    #[cfg(feature = "tauri")]
    {
        if let Ok(Some(window)) = crate::EventState::get_main_window().await
            && let Ok(app_settings) =
                crate::state::Settings::get(&state.pool).await
            && app_settings.hide_on_process_start
        {
            let _ = window.minimize();
        }
    }

    let _ = state
        .presence
        .discord_rpc
        .set_instance_activity(
            &crate::state::DiscordActivityContext {
                instance_name: instance.name.clone(),
                loader: kind.display_name().to_string(),
                game_version: context.applied_content_set.game_version.clone(),
            },
            chrono::Utc::now(),
            true,
            &state.pool,
        )
        .await;

    let keep_alive = tempfile::tempdir()?;
    let rpc_server = crate::util::rpc::RpcServerBuilder::new().launch().await?;

    state
        .process_manager
        .insert_new_process(
            &instance.id,
            &instance.path,
            &instance.name,
            command,
            settings.post_exit_hook.clone(),
            settings.env_args.to_vec(),
            state.directories.instance_logs_dir(&instance.path),
            false,
            keep_alive,
            rpc_server,
            state,
            async |_: &ProcessMetadata,
                   _: &crate::util::rpc::RpcServer|
                   -> crate::Result<()> { Ok(()) },
        )
        .await
}

// ---------- shared helpers ----------

/// A plain HTTP client with a browser-ish user agent; some of these CDNs turn
/// away unknown agents.
pub(crate) static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    // reqwest is built without a TLS provider of its own; the app installs
    // one at startup, but this client can be used before that (tests).
    crate::install_crypto_provider();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent(concat!(
            "ModrinthStudios/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Tigrolino/modrinth-studios)"
        ))
        .build()
        .expect("client configuration should be valid")
});

pub(crate) async fn get_json(url: &str) -> crate::Result<serde_json::Value> {
    let mut attempt = 0u64;
    loop {
        attempt += 1;
        let res = HTTP
            .get(url)
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .with_context(|| format!("requesting {url}"));
        let retryable = match &res {
            Err(_) => true,
            Ok(r) => r.status().as_u16() == 429 || r.status().is_server_error(),
        };
        if retryable && attempt < 3 {
            tokio::time::sleep(Duration::from_millis(800 * attempt)).await;
            continue;
        }
        let res = res?;
        let status = res.status();
        if !status.is_success() {
            bail!("{url} returned HTTP {status}");
        }
        return Ok(res.json().await.with_context(|| format!("reading {url}"))?);
    }
}

pub(crate) async fn get_bytes(url: &str) -> crate::Result<bytes::Bytes> {
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(600 * attempt)).await;
        }
        match HTTP.get(url).timeout(Duration::from_secs(600)).send().await {
            Ok(res) if res.status().is_success() => match res.bytes().await {
                Ok(b) => return Ok(b),
                Err(e) => last = Some(anyhow::anyhow!(e)),
            },
            Ok(res) => {
                last = Some(anyhow::anyhow!(
                    "{url} returned HTTP {}",
                    res.status()
                ))
            }
            Err(e) => last = Some(anyhow::anyhow!(e)),
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!("download failed: {url}")))
}

pub(crate) fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Downloads `url` to `path` unless a file with the right SHA-1 is already
/// there. Returns true when something was downloaded.
pub(crate) async fn download_checked(
    url: &str,
    path: &Path,
    sha1: Option<&str>,
) -> crate::Result<bool> {
    if let Ok(existing) = tokio::fs::read(path).await {
        match sha1 {
            Some(expected) if !expected.is_empty() => {
                if sha1_hex(&existing).eq_ignore_ascii_case(expected) {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
        }
    }
    let bytes = get_bytes(url).await?;
    if let Some(expected) = sha1
        && !expected.is_empty()
        && !sha1_hex(&bytes).eq_ignore_ascii_case(expected)
    {
        bail!("checksum mismatch for {url}");
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Append rather than replace the extension, so `a.json` and `a.png`
    // downloading at the same time don't share one temp file.
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(true)
}

/// Extracts a zip archive into `dest` (blocking work moved off the runtime).
pub(crate) async fn extract_zip(
    bytes: Vec<u8>,
    dest: PathBuf,
) -> crate::Result<()> {
    tokio::task::spawn_blocking(move || -> crate::Result<()> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        std::fs::create_dir_all(&dest)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else {
                continue;
            };
            // Never write outside `dest`.
            let rel: PathBuf = name
                .split('/')
                .filter(|part| !part.is_empty() && *part != ".")
                .collect();
            if rel.as_os_str().is_empty()
                || name.starts_with('/')
                || name
                    .split('/')
                    .any(|part| part == ".." || part.contains(':'))
            {
                continue;
            }
            let out = dest.join(rel);
            if name.ends_with('/') {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut file)?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    &out,
                    std::fs::Permissions::from_mode(mode),
                );
            }
        }
        Ok(())
    })
    .await?
}

/// A Java runtime of this major version: the one set in Modrinth's settings,
/// else one Modrinth downloads.
pub(crate) async fn java_for(
    major: u32,
) -> crate::Result<crate::state::JavaVersion> {
    let state = State::get().await?;
    if let Some(java) =
        crate::state::JavaVersion::get(major, &state.pool).await?
        && let Ok(checked) =
            crate::api::jre::check_jre(PathBuf::from(&java.path)).await
    {
        return Ok(checked);
    }
    let path = crate::api::jre::auto_install_java(major).await?;
    let java = crate::api::jre::check_jre(path).await?;
    java.upsert(&state.pool).await?;
    Ok(java)
}

/// Starts building the Java command, honoring the wrapper hook.
pub(crate) fn java_command(
    java_path: &str,
    wrapper: &Option<String>,
) -> crate::Result<Command> {
    Ok(match wrapper.as_deref().and_then(shlex::split) {
        Some(parts) if !parts.is_empty() => {
            let mut it = parts.into_iter();
            let mut cmd = Command::new(it.next().unwrap_or_default());
            cmd.args(it);
            cmd.arg(java_path);
            cmd
        }
        _ => Command::new(java_path),
    })
}

/// Mojang's version JSON for a release, cached under the client root.
pub(crate) async fn mojang_version_json(
    game_version: &str,
    cache_dir: &Path,
) -> crate::Result<serde_json::Value> {
    let cached = cache_dir
        .join("mojang")
        .join(format!("{game_version}.json"));
    if let Ok(bytes) = tokio::fs::read(&cached).await
        && let Ok(value) = serde_json::from_slice(&bytes)
    {
        return Ok(value);
    }
    let manifest = get_json(
        "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    )
    .await?;
    let url = manifest["versions"]
        .as_array()
        .and_then(|v| v.iter().find(|x| x["id"].as_str() == Some(game_version)))
        .and_then(|x| x["url"].as_str())
        .with_context(|| {
            format!("Minecraft {game_version} isn't in Mojang's version list")
        })?
        .to_string();
    let bytes = get_bytes(&url).await?;
    if let Some(parent) = cached.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&cached, &bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Node-style platform names, used by Lunar's API.
pub(crate) fn node_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "win32",
        "macos" => "darwin",
        _ => "linux",
    }
}

pub(crate) fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86" => "ia32",
        _ => "x64",
    }
}

pub(crate) fn classpath_separator() -> &'static str {
    if cfg!(windows) { ";" } else { ":" }
}

/// A stable random id for this computer (some APIs want an installation id).
pub(crate) async fn installation_id() -> String {
    // Only read the saved id once the app has started; `client_root` would
    // otherwise wait for a startup that never comes (tests, early calls).
    let path = if State::initialized() {
        match client_root(ClientKind::Lunar).await {
            Ok(root) => root.parent().map(|p| p.join("installation-id")),
            Err(_) => None,
        }
    } else {
        None
    };
    if let Some(path) = &path
        && let Ok(text) = tokio::fs::read_to_string(path).await
        && !text.trim().is_empty()
    {
        return text.trim().to_string();
    }
    let id = uuid::Uuid::new_v4().to_string();
    if let Some(path) = &path {
        let _ = tokio::fs::write(path, &id).await;
    }
    id
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::io::Write;

    /// A zip archive in memory with these files.
    pub(crate) fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_file_names_stay_in_their_directory() {
        for name in [
            "",
            ".",
            "..",
            "../outside",
            "a/b",
            "a\\b",
            "C:outside",
            "file:stream",
            "trailing.",
            "trailing ",
        ] {
            assert!(!safe_file_name(name), "{name}");
        }
        for name in ["26.3", "nightly-2026-11-16", "fabric-api.jar"] {
            assert!(safe_file_name(name), "{name}");
        }
    }

    #[test]
    fn older_client_installations_deserialize() {
        let install: ClientInstall = serde_json::from_value(serde_json::json!({
			"client": "norisk", "version_id": "pack|1.21.10", "version_name": "1.21.10", "game_version": "1.21.10", "prepared": true
		})).unwrap();
        assert_eq!(install.checked_at, 0);
        assert!(install.prepared);
    }
    #[tokio::test]
    async fn damaged_sidecars_are_not_plain_instances() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_sidecar(dir.path()).await.unwrap().is_none());
        tokio::fs::create_dir(dir.path().join(".studio"))
            .await
            .unwrap();
        tokio::fs::write(sidecar_path(dir.path()), b"{")
            .await
            .unwrap();
        assert!(read_sidecar(dir.path()).await.is_err());
    }

    use serde_json::json;
    use super::test_util::zip_bytes;

    const ALL: [ClientKind; 4] = [
        ClientKind::Horizon,
        ClientKind::NoRisk,
        ClientKind::Lunar,
        ClientKind::LabyMod,
    ];

    #[test]
    fn client_kinds_have_names_folders_and_stable_ids() {
        for kind in ALL {
            assert!(!kind.display_name().is_empty());
            assert!(safe_file_name(kind.dir_name()));
            assert!(safe_file_name(kind.github_org()));
            let id = serde_json::to_value(kind).unwrap();
            assert_eq!(serde_json::from_value::<ClientKind>(id).unwrap(), kind);
        }
        // These ids are saved in instances and used by the frontend.
        assert_eq!(serde_json::to_value(ClientKind::LabyMod).unwrap(), "labymod");
        assert_eq!(serde_json::to_value(ClientKind::NoRisk).unwrap(), "norisk");
    }

    #[tokio::test]
    async fn sidecars_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let install = ClientInstall {
            client: ClientKind::Lunar,
            version_id: "1.8.9|forge".into(),
            version_name: "1.8.9".into(),
            variant_name: Some("Forge".into()),
            game_version: "1.8.9".into(),
            prepared: true,
            checked_at: 42,
        };
        write_sidecar(dir.path(), &install).await.unwrap();
        let back = read_sidecar(dir.path()).await.unwrap().unwrap();
        assert_eq!(back.client, ClientKind::Lunar);
        assert_eq!(back.version_id, "1.8.9|forge");
        assert_eq!(back.variant_name.as_deref(), Some("Forge"));
        assert_eq!(back.checked_at, 42);
    }

    #[tokio::test]
    async fn setup_rejects_versions_that_could_escape_folders() {
        for (id, gv) in [("../x", "1.8.9"), ("1.8.9|../x", "1.8.9"), ("ok", "a/b"), ("", "1.8.9")] {
            let version = ClientVersion {
                id: id.into(),
                name: "x".into(),
                game_version: gv.into(),
                loader: "vanilla".into(),
                detail: None,
                recommended: false,
            };
            assert!(setup("instance", ClientKind::Lunar, version).await.is_err(), "{id} {gv}");
        }
    }

    #[test]
    fn html_is_turned_into_plain_text() {
        assert_eq!(strip_tags("<p>Hello <b>world</b></p>"), "Hello world");
        assert_eq!(strip_tags("Fish &amp; chips&nbsp;&#8211; it&#8217;s &quot;good&quot;"), "Fish & chips - it's \"good\"");
        assert_eq!(strip_tags("  lots \n of\t space "), "lots of space");
        assert_eq!(
            strip_blocks("a<script>x</script>b<style a>y</style>c<svg>z", &["script", "style", "svg"]),
            "abc"
        );
    }

    #[test]
    fn code_logos_and_relative_urls_are_recognised() {
        assert!(looks_like_code("self.__next_f.push([1])"));
        assert!(looks_like_code("() => x"));
        assert!(!looks_like_code("Season 4 is here!"));
        for logo in ["https://x/logo.png", "https://x/favicon.ico", "https://x/icon-192.png", "https://x/og-default.jpg", "https://x/a.svg"] {
            assert!(looks_like_logo(logo), "{logo}");
        }
        assert!(!looks_like_logo("https://x/covers/season-4.webp"));
        assert_eq!(absolute_url("/img/a.png", "https://site.test").as_deref(), Some("https://site.test/img/a.png"));
        assert_eq!(absolute_url("//cdn.test/a.png", "https://site.test").as_deref(), Some("https://cdn.test/a.png"));
        assert_eq!(absolute_url(" https://a.test/x ", "o").as_deref(), Some("https://a.test/x"));
        assert_eq!(absolute_url("http://insecure.test/x", "o"), None);
        assert_eq!(absolute_url("relative.png", "o"), None);
    }

    #[test]
    fn news_pages_with_storyblok_data_are_read() {
        let page = r#"<html><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"story":{"items":[
            {"slug":"big-update","first_published_at":"2026-11-01","content":{"title":"Big update","image":{"filename":"https://a.storyblok.com/x.png"}}},
            {"slug":"no-title","content":{}}
        ]}}}}</script></html>"#;
        let posts = scrape_posts(page, "https://www.lunarclient.com", "/news/");
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "Big update");
        assert_eq!(posts[0].url.as_deref(), Some("https://www.lunarclient.com/news/big-update"));
        assert_eq!(posts[0].image.as_deref(), Some("https://a.storyblok.com/x.png"));
        assert_eq!(posts[0].date.as_deref(), Some("2026-11-01"));
    }

    #[test]
    fn news_pages_with_other_next_data_are_searched() {
        let page = r#"<script id="__NEXT_DATA__">{"props":{"x":{"posts":[
            {"title":{"rendered":"WP &amp; post"},"slug":"wp-post","excerpt":"<p>Hi</p>","featuredImage":{"url":"https://x/i.png"},"date":"2026"}
        ]}}}</script>"#;
        let posts = scrape_posts(page, "https://laby.net", "/blog/");
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "WP & post");
        assert_eq!(posts[0].summary.as_deref(), Some("Hi"));
        assert_eq!(posts[0].image.as_deref(), Some("https://x/i.png"));
        assert_eq!(posts[0].url.as_deref(), Some("https://laby.net/blog/wp-post"));
    }

    #[test]
    fn news_links_are_scraped_from_the_visible_page() {
        let page = r#"
            <nav><a href="/news/">News</a></nav>
            <script>var a = '<a href="/news/fake">self.__next_f.push(x)</a>';</script>
            <a class="card" href="/news/update-1"><img data-src="/img/cover1.png"><h2>Update 1 is here today</h2>Read Article</a>
            <a href="https://other.test/news/x">External post title here</a>
            <a href="/news/update-1">Duplicate link text here</a>
            <a href="/news/a">abc</a>
        "#;
        let posts = scrape_posts(page, "https://site.test", "/news/");
        assert_eq!(posts.len(), 1, "{posts:?}");
        assert_eq!(posts[0].title, "Update 1 is here today");
        assert_eq!(posts[0].url.as_deref(), Some("https://site.test/news/update-1"));
        assert_eq!(posts[0].image.as_deref(), Some("https://site.test/img/cover1.png"));
        assert!(scrape_posts("<html>nothing</html>", "https://site.test", "/news/").is_empty());
    }

    #[test]
    fn news_cards_that_are_not_posts_are_dropped() {
        let card = |title: &str, image: Option<&str>, summary: Option<&str>| NewsItem {
            title: title.into(),
            image: image.map(String::from),
            summary: summary.map(String::from),
            url: Some(format!("https://site.test/{}", title.len())),
            ..Default::default()
        };
        let shared = Some("https://site.test/share.png");
        let out = tidy_news(vec![
            card("A long enough post title one", shared, None),
            card("A long enough post title two", shared, None),
            card("A long enough post title three", shared, None),
            card("Short", Some("https://site.test/own.png"), None),
            card("self.__next_f.push(1)", None, Some("x")),
            card("SALE 50% --:--:--", Some("https://site.test/sale.png"), None),
            card(
                "BuyNowAndSaveBigOnEverythingInTheStoreTodayOnlyWhileStocksLastForever",
                None,
                Some("promo"),
            ),
            card("Has a summary but no picture", None, Some("Real text")),
        ]);
        let titles: Vec<&str> = out.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["Short", "Has a summary but no picture"]);
    }

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn files_already_downloaded_are_not_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.jar");
        tokio::fs::write(&path, b"abc").await.unwrap();
        // The URL can't be reached, so these only pass without a download.
        let url = "https://unreachable.invalid/a.jar";
        assert!(!download_checked(url, &path, Some("A9993E364706816ABA3E25717850C26C9CD0D89D")).await.unwrap());
        assert!(!download_checked(url, &path, None).await.unwrap());
        assert!(!download_checked(url, &path, Some("")).await.unwrap());
    }

    #[tokio::test]
    async fn zips_never_write_outside_their_folder() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("out");
        let bytes = zip_bytes(&[
            ("good/a.txt", &b"a"[..]),
            ("./b.txt", &b"b"[..]),
            ("../evil.txt", &b"x"[..]),
            ("nested/../../evil2.txt", &b"x"[..]),
            ("/abs.txt", &b"x"[..]),
            ("C:/drive.txt", &b"x"[..]),
        ]);
        extract_zip(bytes, dest.clone()).await.unwrap();
        assert_eq!(std::fs::read(dest.join("good/a.txt")).unwrap(), b"a");
        assert_eq!(std::fs::read(dest.join("b.txt")).unwrap(), b"b");
        assert!(!root.path().join("evil.txt").exists());
        assert!(!root.path().join("evil2.txt").exists());
        assert!(!dest.join("abs.txt").exists());
        assert!(!dest.join("drive.txt").exists());
        assert!(extract_zip(b"not a zip".to_vec(), dest).await.is_err());
    }

    #[test]
    fn java_commands_respect_the_wrapper_hook() {
        let plain = java_command("/java/bin/java", &None).unwrap();
        assert_eq!(plain.as_std().get_program(), "/java/bin/java");
        let wrapped = java_command("/java/bin/java", &Some("prime-run --flag".into())).unwrap();
        let cmd = wrapped.as_std();
        assert_eq!(cmd.get_program(), "prime-run");
        let args: Vec<&std::ffi::OsStr> = cmd.get_args().collect();
        assert_eq!(args, ["--flag", "/java/bin/java"]);
        let empty = java_command("/java/bin/java", &Some("   ".into())).unwrap();
        assert_eq!(empty.as_std().get_program(), "/java/bin/java");
    }

    #[test]
    fn platform_names_match_what_the_apis_expect() {
        assert!(["win32", "darwin", "linux"].contains(&node_os()));
        assert!(["x64", "arm64", "ia32"].contains(&node_arch()));
        assert_eq!(classpath_separator(), if cfg!(windows) { ";" } else { ":" });
    }

    #[tokio::test]
    async fn news_downloads_only_go_to_known_sources() {
        for url in [
            "https://evil.test/studio-news/index.json",
            "https://raw.githubusercontent.com/someone-else/repo/main/x.md",
            "https://raw.githubusercontent.com/Tigrolino/modrinth-studios/../../other/x",
        ] {
            assert!(news_text(url).await.is_err(), "{url}");
        }
        for url in ["https://evil.test/news/x.mp4", "https://modrinth.com/news/../admin"] {
            assert!(news_media(url).await.is_err(), "{url}");
        }
    }

    // ---- live checks: `cargo test -- --ignored` ----

    #[tokio::test]
    #[ignore = "live: talks to lunarclient.com and labymod.net"]
    async fn live_client_websites_still_have_news() {
        for kind in [ClientKind::Lunar, ClientKind::LabyMod] {
            let items = news(kind).await.unwrap_or_else(|e| panic!("{kind:?}: {e:#}"));
            assert!(!items.is_empty(), "{kind:?}: no news found (the page layout may have changed)");
            for item in &items {
                assert!(item.url.as_deref().is_some_and(|u| u.starts_with("https://")), "{kind:?}: {}", item.title);
                assert!(!looks_like_code(&item.title), "{kind:?}: {}", item.title);
            }
        }
    }

    #[tokio::test]
    #[ignore = "live: talks to GitHub"]
    async fn live_studios_news_index_loads() {
        let text = news_text(
            "https://raw.githubusercontent.com/Tigrolino/modrinth-studios/studio/studio-news/index.json",
        )
        .await
        .expect("Studios news index");
        let index: serde_json::Value = serde_json::from_str(&text).expect("index.json isn't JSON");
        assert!(index.is_array() || index.is_object());
    }

    #[tokio::test]
    #[ignore = "live: talks to Mojang"]
    async fn live_mojang_version_files_are_cached() {
        let dir = tempfile::tempdir().unwrap();
        let v = mojang_version_json("1.8.9", dir.path()).await.expect("Mojang's 1.8.9 file");
        assert_eq!(v["id"], "1.8.9");
        assert!(dir.path().join("mojang/1.8.9.json").exists());
        // Second time it comes from the cache.
        let again = mojang_version_json("1.8.9", dir.path()).await.unwrap();
        assert_eq!(again, v);
        assert!(mojang_version_json("0.0.0-nope", dir.path()).await.is_err());
    }

    #[tokio::test]
    #[ignore = "live: talks to GitHub"]
    async fn live_client_logos_are_reachable() {
        for kind in ALL.into_iter().filter(|k| *k != ClientKind::NoRisk) {
            let bytes = get_bytes(&format!("https://github.com/{}.png?size=64", kind.github_org()))
                .await
                .unwrap_or_else(|e| panic!("{kind:?}: {e:#}"));
            assert!(bytes.len() > 100, "{kind:?}");
        }
    }

    #[test]
    fn next_data_search_stops_at_twelve_posts() {
        let posts: Vec<serde_json::Value> = (0..20)
            .map(|i| json!({ "title": format!("Post number {i}"), "slug": format!("p{i}") }))
            .collect();
        let mut out = Vec::new();
        collect_posts(&json!({ "a": posts }), "https://o.test", "/n/", &mut out);
        assert_eq!(out.len(), 12);
    }
}
