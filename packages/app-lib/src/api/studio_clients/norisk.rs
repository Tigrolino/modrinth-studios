//! NoRisk Client: a set of Fabric mods plus a NoRisk login token.
//!
//! Follows NoRisk's open-source launcher (github.com/NoRiskClient/
//! noriskclient-launcher, branch v3): log in to NoRisk's API with the
//! Minecraft session (request a server id, join it on Mojang's session server,
//! validate), read the pack definitions, download the pack's mods for the
//! instance's Minecraft version, and start the normal Fabric instance with
//! `-Dfabric.addMods=@<list>` and `-Dnorisk.token=...`.
//!
//! The pack's mods live outside the instance's mods folder, so they don't
//! show up in (or get removed by) Modrinth's content list, and any mods the
//! user adds from Modrinth load alongside them.

use super::{
    ClientInstall, ClientVersion, HTTP, Progress, client_root,
    download_checked, installation_id,
};
use crate::state::Credentials;
use anyhow::{Context, bail};
use futures::StreamExt;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

const API: &str = "https://api.norisk.gg/api/v1";

/// Packs whose in-game client downloads its own assets (from NoRisk's launcher).
const CLIENT_MANAGED_PACKS: &[&str] = &[
    "nrc-nightly",
    "nrc-standalone",
    "nrc-mini",
    "verity-nexus",
    "verity-emote",
];

/// NoRisk tokens are reused for a while so we don't log in on every click.
static TOKENS: std::sync::LazyLock<
    std::sync::Mutex<HashMap<String, (String, std::time::Instant)>>,
> = std::sync::LazyLock::new(Default::default);
const TOKEN_TTL: Duration = Duration::from_secs(30 * 60);
static LOGIN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Logs in to NoRisk with the Minecraft session and returns the token.
async fn token(creds: &Credentials) -> crate::Result<String> {
    let _guard = LOGIN_LOCK.lock().await;
    let key = creds.offline_profile.id.to_string();
    if let Ok(cache) = TOKENS.lock()
        && let Some((token, at)) = cache.get(&key)
        && at.elapsed() < TOKEN_TTL
    {
        return Ok(token.clone());
    }
    let token = login(creds).await?;
    if let Ok(mut cache) = TOKENS.lock() {
        cache.insert(key, (token.clone(), std::time::Instant::now()));
    }
    Ok(token)
}

async fn login(creds: &Credentials) -> crate::Result<String> {
    // 1. NoRisk hands out a server id ...
    let server: Value = HTTP
        .post(format!("{API}/launcher/auth/request-server-id"))
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .context("NoRisk's servers aren't reachable")?
        .error_for_status()
        .context("NoRisk refused the login")?
        .json()
        .await?;
    let server_id = server["serverId"]
        .as_str()
        .or(server["server_id"].as_str())
        .context("NoRisk didn't send a server id")?
        .to_string();

    // 2. ... we "join" it on Mojang's session server, proving we own the account
    // (same request and client the normal launch uses) ...
    let mut last_status = None;
    for attempt in 0..3u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(1500 * attempt)).await;
        }
        let join = crate::util::fetch::INSECURE_REQWEST_CLIENT
			.post("https://sessionserver.mojang.com/session/minecraft/join")
			.timeout(Duration::from_secs(20))
			.json(&json!({
				"accessToken": creds.access_token,
				"selectedProfile": creds.offline_profile.id.simple().to_string(),
				"serverId": server_id,
			}))
			.send()
			.await
			.context("Mojang's session server isn't reachable")?;
        let status = join.status();
        if status.is_success() {
            last_status = None;
            break;
        }
        let text = join.text().await.unwrap_or_default();
        if text.contains("InsufficientPrivileges") {
            bail!(
                "Your Microsoft account doesn't allow multiplayer (family or privacy settings), which NoRisk needs to log in"
            );
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            bail!(
                "Mojang didn't accept your Minecraft login. Try signing out and back in from the account menu in the top bar."
            );
        }
        last_status = Some(status);
    }
    if let Some(status) = last_status {
        bail!(
            "Mojang's session server is busy or down right now (HTTP {status}). Try again in a minute."
        );
    }

    // 3. ... and NoRisk checks that join and gives us a token.
    let hwid = installation_id().await;
    let res: Value = HTTP
        .post(format!("{API}/launcher/auth/validate/v2"))
        .timeout(Duration::from_secs(20))
        .query(&[
            ("force", "false"),
            ("hwid", hwid.as_str()),
            ("username", creds.offline_profile.name.as_str()),
            ("server_id", server_id.as_str()),
        ])
        .send()
        .await?
        .error_for_status()
        .context("NoRisk couldn't verify your Minecraft account")?
        .json()
        .await?;
    res["value"]
        .as_str()
        .or(res["token"].as_str())
        .or(res.as_str())
        .map(String::from)
        .context("NoRisk didn't send a token")
}

async fn api_get(path: &str, token: &str, uuid: &str) -> crate::Result<Value> {
    Ok(HTTP
        .get(format!("{API}/{path}"))
        .timeout(Duration::from_secs(60))
        .bearer_auth(token)
        .query(&[("uuid", uuid)])
        .send()
        .await?
        .error_for_status()
        .with_context(|| format!("NoRisk's {path} request failed"))?
        .json()
        .await?)
}

fn str_field<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| v[*k].as_str())
}

pub(super) async fn list_versions(
    creds: &Credentials,
) -> crate::Result<Vec<ClientVersion>> {
    let token = token(creds).await?;
    let uuid = creds.offline_profile.id.to_string();
    let versions = api_get("launcher/versions-v3", &token, &uuid).await?;
    let packs = api_get("launcher/modpacks-v3", &token, &uuid).await.ok();

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for profile in versions["profiles"].as_array().into_iter().flatten() {
        let Some(gv) = str_field(profile, &["game_version", "gameVersion"])
        else {
            continue;
        };
        let loader = str_field(profile, &["loader"])
            .unwrap_or("fabric")
            .to_ascii_lowercase();
        if loader != "fabric" {
            // Modrinth instances here are set up for NoRisk's Fabric packs.
            continue;
        }
        let Some(pack) = str_field(
            profile,
            &[
                "selected_norisk_pack_id",
                "selectedNoriskPackId",
                "norisk_pack",
                "pack",
            ],
        ) else {
            continue;
        };
        if !seen.insert(format!("{pack}|{gv}")) {
            continue;
        }
        let pack_name = packs
            .as_ref()
            .and_then(|p| p["packs"][pack]["displayName"].as_str())
            .unwrap_or(pack);
        out.push(ClientVersion {
            id: format!("{pack}|{gv}"),
            name: gv.to_string(),
            game_version: gv.to_string(),
            loader: "fabric".into(),
            detail: Some(pack_name.to_string()),
            recommended: out.is_empty(),
        });
    }
    if out.is_empty() {
        bail!("NoRisk didn't list any Fabric versions");
    }
    Ok(out)
}

fn split_id(id: &str) -> (String, String) {
    match id.split_once('|') {
        Some((pack, gv)) => (pack.to_string(), gv.to_string()),
        None => ("norisk-prod".to_string(), id.to_string()),
    }
}

/// Mods of a pack after inheritance and exclusions (NoRisk's own rules).
fn resolve_mods(
    config: &Value,
    pack: &str,
    visited: &mut HashSet<String>,
) -> crate::Result<HashMap<String, Value>> {
    if !visited.insert(pack.to_string()) {
        bail!("NoRisk's pack '{pack}' inherits from itself");
    }
    let def = &config["packs"][pack];
    if def.is_null() {
        bail!("NoRisk has no pack called '{pack}' anymore");
    }
    let mut mods = HashMap::new();
    for parent in def["inheritsFrom"].as_array().into_iter().flatten() {
        if let Some(parent) = parent.as_str() {
            mods.extend(resolve_mods(config, parent, visited)?);
        }
    }
    for m in def["mods"].as_array().into_iter().flatten() {
        if let Some(id) = m["id"].as_str() {
            mods.insert(id.to_string(), m.clone());
        }
    }
    for ex in def["excludeMods"].as_array().into_iter().flatten() {
        if let Some(ex) = ex.as_str() {
            mods.remove(ex);
        }
    }
    visited.remove(pack);
    Ok(mods)
}

fn resolve_assets(
    config: &Value,
    pack: &str,
    visited: &mut HashSet<String>,
) -> Vec<String> {
    if !visited.insert(pack.to_string()) {
        return Vec::new();
    }
    let def = &config["packs"][pack];
    let mut out = Vec::new();
    for parent in def["inheritsFrom"].as_array().into_iter().flatten() {
        if let Some(parent) = parent.as_str() {
            out.extend(resolve_assets(config, parent, visited));
        }
    }
    for a in def["assets"].as_array().into_iter().flatten() {
        if let Some(a) = a.as_str()
            && !out.iter().any(|x| x == a)
        {
            out.push(a.to_string());
        }
    }
    out
}

/// Where one mod comes from: (url, file name).
async fn mod_source(
    entry: &Value,
    gv: &str,
    config: &Value,
) -> crate::Result<Option<(String, String)>> {
    let target = &entry["compatibility"][gv]["fabric"];
    if target.is_null() {
        return Ok(None);
    }
    let identifier = target["identifier"]
        .as_str()
        .context("pack entry without identifier")?;
    let source = if target["source"].is_object() {
        &target["source"]
    } else {
        &entry["source"]
    };
    let filename = target["filename"].as_str().map(String::from);

    match source["type"].as_str() {
        Some("modrinth") => {
            // The identifier is a version number ("5.3-alpha-1") or a version id.
            let project = source["projectId"]
                .as_str()
                .or(source["projectSlug"].as_str())
                .context("Modrinth mod without a project")?;
            let versions: Value = super::get_json(&format!(
                "https://api.modrinth.com/v2/project/{project}/version?loaders=%5B%22fabric%22%5D"
            ))
            .await?;
            let list = versions.as_array().cloned().unwrap_or_default();
            let for_gv = |v: &&Value| {
                v["game_versions"]
                    .as_array()
                    .is_some_and(|g| g.iter().any(|x| x.as_str() == Some(gv)))
            };
            let matches = |v: &&Value| {
                v["id"].as_str() == Some(identifier)
                    || v["version_number"].as_str() == Some(identifier)
            };
            let found = list
                .iter()
                .find(|v| matches(v) && for_gv(v))
                .or_else(|| list.iter().find(|v| matches(v)))
                .cloned();
            let version = match found {
                Some(v) => v,
                // Not among the Fabric versions: ask Modrinth for that exact
                // version id/number (it may be tagged with another loader),
                // and failing that take the newest Fabric build for this
                // game version, as NoRisk's launcher would end up running.
                None => match super::get_json(&format!(
                    "https://api.modrinth.com/v2/project/{project}/version/{}",
                    urlencoding_lite(identifier)
                ))
                .await
                {
                    Ok(v) => v,
                    Err(e) => list.iter().find(|v| for_gv(v)).cloned().with_context(
                        || format!("Modrinth has no version {identifier} of {project}: {e:#}"),
                    )?,
                },
            };
            let files =
                version["files"].as_array().cloned().unwrap_or_default();
            let file = files
                .iter()
                .find(|f| f["primary"].as_bool() == Some(true))
                .or(files.first())
                .context("Modrinth version without files")?;
            let url = file["url"]
                .as_str()
                .context("file without url")?
                .to_string();
            let slug = source["projectSlug"].as_str().unwrap_or("mod");
            let name = filename
                .or_else(|| file["filename"].as_str().map(String::from))
                .unwrap_or_else(|| format!("{slug}-{identifier}.jar"));
            Ok(Some((url, name)))
        }
        Some("maven") => {
            let repo_ref = source["repositoryRef"].as_str().unwrap_or_default();
            let repo = config["repositories"][repo_ref]
                .as_str()
                .with_context(|| {
                    format!("unknown NoRisk repository '{repo_ref}'")
                })?
                .trim_end_matches('/');
            let group = source["groupId"]
                .as_str()
                .unwrap_or_default()
                .replace('.', "/");
            let artifact = source["artifactId"].as_str().unwrap_or_default();
            let jar = format!("{artifact}-{identifier}.jar");
            Ok(Some((
                format!("{repo}/{group}/{artifact}/{identifier}/{jar}"),
                filename.unwrap_or(jar),
            )))
        }
        Some("url") => Ok(Some((
            identifier.to_string(),
            filename.context("URL mod without a file name")?,
        ))),
        _ => bail!("Unsupported NoRisk mod source"),
    }
}

fn mods_list_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".studio").join("norisk-mods.txt")
}

pub(super) async fn prepare(
    install: &ClientInstall,
    instance_dir: &Path,
    creds: &Credentials,
    progress: &Progress<'_>,
) -> crate::Result<()> {
    let (pack, gv) = split_id(&install.version_id);
    let root = client_root(super::ClientKind::NoRisk).await?;
    let uuid = creds.offline_profile.id.to_string();

    progress.step(5.0, "Logging in to NoRisk");
    let token = token(creds).await?;
    progress.step(5.0, "Reading the NoRisk pack");
    let config = api_get("launcher/modpacks-v3", &token, &uuid).await?;

    let mods = resolve_mods(&config, &pack, &mut HashSet::new())?;
    let mods_dir = root.join("mods").join(&gv);
    tokio::fs::create_dir_all(&mods_dir).await?;

    // "<mod>@<identifier>" -> file name, so unchanged mods skip the lookup.
    let resolved_path = mods_dir.join("resolved.json");
    let mut resolved: HashMap<String, String> = tokio::fs::read(&resolved_path)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut paths = Vec::new();
    // Mods that couldn't be downloaded. NoRisk's own jars are required; a
    // third-party Modrinth mod (like Cubes Without Borders) only costs that
    // mod's feature, so the client still installs without it.
    let mut skipped: Vec<String> = Vec::new();
    let mut missing_optional: Vec<String> = Vec::new();
    let step = 60.0 / mods.len().max(1) as f64;
    for (id, entry) in &mods {
        progress.step(step, &format!("Downloading {id}"));
        let ident = entry["compatibility"][gv.as_str()]["fabric"]["identifier"]
            .as_str()
            .unwrap_or_default();
        let key = format!("{id}@{ident}");
        if let Some(name) = resolved.get(&key)
            && super::safe_file_name(name)
            && mods_dir.join(name).exists()
        {
            paths.push(mods_dir.join(name).to_string_lossy().to_string());
            continue;
        }
        // One unavailable mod shouldn't stop the whole client from installing.
        let third_party = {
            let target = &entry["compatibility"][gv.as_str()]["fabric"];
            let source = if target["source"].is_object() {
                &target["source"]
            } else {
                &entry["source"]
            };
            source["type"].as_str() == Some("modrinth")
        };
        let mut fail = |id: &String, why: String| {
            tracing::warn!("NoRisk mod {id} couldn't be downloaded: {why}");
            if third_party {
                missing_optional.push(id.clone());
            } else {
                skipped.push(format!("{id} ({why})"));
            }
        };
        // Modrinth sometimes rate-limits a burst of lookups: try twice.
        let mut found = mod_source(entry, &gv, &config).await;
        if found.is_err() {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            found = mod_source(entry, &gv, &config).await;
        }
        let (url, name) = match found {
            Ok(Some(found)) => found,
            Ok(None) => continue,
            Err(e) => {
                fail(id, format!("{e:#}"));
                continue;
            }
        };
        if !super::safe_file_name(&name) {
            fail(id, format!("unusable file name {name}"));
            continue;
        }
        let path = mods_dir.join(&name);
        if let Err(e) = download_checked(&url, &path, None).await {
            fail(id, format!("{e:#}"));
            continue;
        }
        paths.push(path.to_string_lossy().to_string());
        resolved.insert(key, name.clone());
    }
    let _ =
        tokio::fs::write(&resolved_path, serde_json::to_vec(&resolved)?).await;
    if paths.is_empty() {
        bail!("NoRisk's pack has no mods for Minecraft {gv}");
    }
    if !missing_optional.is_empty() {
        missing_optional.sort();
        tracing::warn!(
            "NoRisk installed without these optional mods: {}",
            missing_optional.join(", ")
        );
    }
    if !skipped.is_empty() {
        skipped.sort();
        bail!(
            "NoRisk could not download these required mods: {}. Retry the download; the previous installation has been kept.",
            skipped.join(", ")
        );
    }
    let list = mods_list_path(instance_dir);
    if let Some(parent) = list.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(
        instance_dir.join(".studio/norisk-pack.json"),
        serde_json::to_vec(&config)?,
    )
    .await?;
    tokio::fs::write(root.join("modpacks.json"), serde_json::to_vec(&config)?)
        .await?;
    let temporary_list = list.with_extension("tmp");
    tokio::fs::write(&temporary_list, paths.join("\n")).await?;
    tokio::fs::rename(&temporary_list, &list).await?;

    // Assets: newer packs download their own in game; older ones need them in
    // <instance>/NoRiskClient/assets.
    let assets = resolve_assets(&config, &pack, &mut HashSet::new());
    if !CLIENT_MANAGED_PACKS.contains(&pack.as_str()) {
        for asset in &assets {
            progress.step(
                20.0 / assets.len().max(1) as f64,
                &format!("Downloading NoRisk assets ({asset})"),
            );
            if let Err(e) =
                download_assets(asset, &token, &uuid, instance_dir).await
            {
                tracing::warn!("NoRisk asset group {asset} failed: {e:#}");
            }
        }
    }
    Ok(())
}

async fn download_assets(
    group: &str,
    token: &str,
    uuid: &str,
    instance_dir: &Path,
) -> crate::Result<()> {
    let manifest =
        api_get(&format!("launcher/pack/{group}"), token, uuid).await?;
    let Some(objects) = manifest["objects"].as_object() else {
        return Ok(());
    };
    let base = instance_dir.join("NoRiskClient").join("assets");
    let jobs: Vec<(String, String, PathBuf, bool)> = objects
        .iter()
        .filter_map(|(name, obj)| {
            let hash = obj["hash"].as_str()?.to_string();
            let (path, is_override) = asset_target(instance_dir, &base, name)?;
            Some((name.clone(), hash, path, is_override))
        })
        .collect();

    let results: Vec<crate::Result<()>> = futures::stream::iter(jobs)
        .map(|(name, hash, path, is_override)| async move {
            if is_override && path.exists() {
                return Ok::<(), crate::Error>(());
            }
            if !is_override
                && let Ok(bytes) = tokio::fs::read(&path).await
                && super::sha1_hex(&bytes).eq_ignore_ascii_case(&hash)
            {
                return Ok(());
            }
            let url = format!(
                "https://cdn.norisk.gg/assets/{group}/assets/{name}?v={hash}"
            );
            let bytes = super::get_bytes(&url).await?;
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(&path, bytes).await?;
            Ok(())
        })
        .buffer_unordered(12)
        .collect()
        .await;
    results.into_iter().collect::<crate::Result<Vec<()>>>()?;
    Ok(())
}

/// Where one file of a NoRisk asset group goes: `overrides/...` into the
/// game folder (only if missing), everything else under the assets folder.
/// Names that could leave those folders are skipped.
fn asset_target(
    instance_dir: &Path,
    assets_dir: &Path,
    name: &str,
) -> Option<(PathBuf, bool)> {
    let (root, rel, is_override) = match name.strip_prefix("overrides/") {
        Some(rel) => (instance_dir, rel, true),
        None => (assets_dir, name, false),
    };
    let safe = !rel.is_empty()
        && rel.split(['/', '\\']).all(super::safe_file_name);
    safe.then(|| (root.join(rel), is_override))
}

/// Extra JVM arguments for the normal Fabric launch.
pub(super) async fn jvm_args(
    install: &ClientInstall,
    instance_dir: &Path,
    creds: &Credentials,
) -> crate::Result<Vec<String>> {
    let (pack, _gv) = split_id(&install.version_id);
    let root = client_root(super::ClientKind::NoRisk).await?;
    let list = mods_list_path(instance_dir);
    if !list.exists() {
        bail!("NoRisk isn't downloaded yet");
    }
    let token = token(creds).await?;

    let mut args = vec![
        format!("-Dfabric.addMods=@{}", list.to_string_lossy()),
        format!("-Dnorisk.token={token}"),
        "-Dnorisk.experimental=false".to_string(),
        format!("-Dnorisk.pack={pack}"),
        format!("-Dnorisk.meta.dir={}", root.to_string_lossy()),
        "-Dnorisk.analytics.enabled=false".to_string(),
    ];
    if CLIENT_MANAGED_PACKS.contains(&pack.as_str()) {
        let assets_dir = root.join("assets");
        tokio::fs::create_dir_all(&assets_dir).await?;
        args.push(format!("-Dnrc.assets.dir={}", assets_dir.to_string_lossy()));
        if let Ok(bytes) = read_pack_config(instance_dir, &root).await
            && let Ok(config) = serde_json::from_slice::<Value>(&bytes)
        {
            let assets = resolve_assets(&config, &pack, &mut HashSet::new());
            if !assets.is_empty() {
                args.push(format!("-Dnrc.assets.bucket={}", assets.join(",")));
            }
        }
    }
    Ok(args)
}

async fn read_pack_config(
    instance_dir: &Path,
    root: &Path,
) -> std::io::Result<Vec<u8>> {
    match tokio::fs::read(instance_dir.join(".studio/norisk-pack.json")).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tokio::fs::read(root.join("modpacks.json")).await
        }
        result => result,
    }
}

/// One mod in the instance's NoRisk pack, for the client tab.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct NoRiskMod {
    pub id: String,
    pub name: String,
    /// Modrinth project id, when the mod comes from Modrinth.
    pub modrinth_id: Option<String>,
}

/// The mods NoRisk's pack puts in this instance (from the last download).
pub async fn pack_mods(instance_id: &str) -> crate::Result<Vec<NoRiskMod>> {
    let Some(install) = super::get(instance_id).await? else {
        return Ok(Vec::new());
    };
    let (pack, gv) = split_id(&install.version_id);
    let root = client_root(super::ClientKind::NoRisk).await?;
    let instance_dir = crate::api::instance::get_full_path(instance_id).await?;
    let Ok(bytes) = read_pack_config(&instance_dir, &root).await else {
        return Ok(Vec::new());
    };
    let config: Value = serde_json::from_slice(&bytes)?;
    let mods = resolve_mods(&config, &pack, &mut HashSet::new())?;
    let mut out: Vec<NoRiskMod> = mods
        .into_iter()
        .filter(|(_, m)| !m["compatibility"][gv.as_str()]["fabric"].is_null())
        .map(|(id, m)| {
            let target = &m["compatibility"][gv.as_str()]["fabric"];
            let source = if target["source"].is_object() {
                &target["source"]
            } else {
                &m["source"]
            };
            let modrinth_id = if source["type"].as_str() == Some("modrinth") {
                source["projectId"].as_str().map(String::from)
            } else {
                None
            };
            NoRiskMod {
                name: m["displayName"].as_str().unwrap_or(&id).to_string(),
                modrinth_id,
                id,
            }
        })
        .collect();
    out.sort_by_key(|m| m.name.to_lowercase());
    Ok(out)
}

/// NoRisk's blog (news and changelogs), the same feed their launcher shows.
pub(super) async fn news() -> crate::Result<Vec<super::NewsItem>> {
    let posts = super::get_json(
        "https://blog.norisk.gg/wp-json/wp/v2/posts?categories=21,2&per_page=10&page=1",
    )
    .await?;
    Ok(parse_blog_posts(&posts))
}

/// NoRisk's WordPress posts as news cards.
fn parse_blog_posts(posts: &Value) -> Vec<super::NewsItem> {
    posts
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let y = &p["yoast_head_json"];
            let title =
                y["title"].as_str().or(p["title"]["rendered"].as_str())?;
            let title = title
                .trim_end_matches(" - NoRisk Client Blog")
                .trim_end_matches(" - NoRisk Client")
                .trim_end_matches(" - NoRiskClient");
            Some(super::NewsItem {
                title: super::strip_tags(title),
                date: p["date"].as_str().map(String::from),
                url: y["og_url"]
                    .as_str()
                    .or(p["link"].as_str())
                    .map(String::from),
                image: y["og_image"][0]["url"].as_str().map(String::from),
                summary: y["og_description"]
                    .as_str()
                    .or(y["description"].as_str())
                    .map(super::strip_tags),
                body: String::new(),
            })
        })
        .collect()
}

/// Percent-encodes the characters that matter in a URL path segment.
fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '+' => "%2B".to_string(),
            ' ' => "%20".to_string(),
            '/' => "%2F".to_string(),
            '#' => "%23".to_string(),
            '?' => "%3F".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

/// Logs in to NoRisk now (used by the "Sign in" button) and returns the
/// Minecraft name it signed in as.
pub async fn sign_in() -> crate::Result<String> {
    let creds = super::default_credentials().await?;
    if let Ok(mut cache) = TOKENS.lock() {
        cache.remove(&creds.offline_profile.id.to_string());
    }
    token(&creds).await?;
    Ok(creds.offline_profile.name.clone())
}

/// Whether there's a recent NoRisk login for the current account.
pub async fn signed_in() -> crate::Result<Option<String>> {
    let creds = super::default_credentials().await?;
    let ok = TOKENS
        .lock()
        .ok()
        .and_then(|c| {
            c.get(&creds.offline_profile.id.to_string())
                .map(|(_, at)| at.elapsed() < TOKEN_TTL)
        })
        .unwrap_or(false);
    Ok(ok.then(|| creds.offline_profile.name.clone()))
}

// ---------- Capes (NoRisk's cosmetics API, as used by their launcher) ----------

const COSMETICS: &str = "https://api.norisk.gg/api/v1/cosmetics";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct NoRiskCape {
    pub hash: String,
    pub uses: i64,
    pub elytra: bool,
    pub accepted: bool,
    pub creation_date: i64,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct NoRiskCapePage {
    pub capes: Vec<NoRiskCape>,
    pub next_cursor: Option<String>,
}

fn parse_cape(v: &Value) -> Option<NoRiskCape> {
    Some(NoRiskCape {
        hash: v["_id"].as_str().or(v["hash"].as_str())?.to_string(),
        uses: v["uses"].as_i64().unwrap_or_default(),
        elytra: v["elytra"].as_bool().unwrap_or(true),
        accepted: v["accepted"].as_bool().unwrap_or(true),
        creation_date: v["creationDate"].as_i64().unwrap_or_default(),
    })
}

/// Browse everyone's capes. `sort` is "mostUsed", "newest" or "oldest";
/// `time_frame` is "weekly", "monthly" or empty for all time.
pub async fn capes_browse(
    sort: &str,
    time_frame: Option<&str>,
    cursor: Option<&str>,
) -> crate::Result<NoRiskCapePage> {
    let creds = super::default_credentials().await?;
    let token = token(&creds).await?;
    let uuid = creds.offline_profile.id.to_string();

    // Newer endpoint with cursors; the old one pages with "page:N".
    let legacy_page = cursor
        .and_then(|c| c.strip_prefix("page:"))
        .and_then(|p| p.parse::<u32>().ok());
    if legacy_page.is_none() {
        let mut query: Vec<(&str, String)> =
            vec![("limit", "30".into()), ("sortBy", sort.into())];
        if let Some(c) = cursor {
            query.push(("cursor", c.into()));
        }
        if let Some(t) = time_frame.filter(|t| !t.is_empty()) {
            query.push(("timeFrame", t.into()));
        }
        let res = HTTP
            .get(format!("{COSMETICS}/cape/browse/v2"))
            .timeout(Duration::from_secs(30))
            .bearer_auth(&token)
            .query(&query)
            .send()
            .await?;
        if res.status().is_success() {
            let body: Value = res.json().await?;
            return Ok(NoRiskCapePage {
                capes: body["capes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(parse_cape)
                    .collect(),
                next_cursor: body["nextCursor"].as_str().map(String::from),
            });
        }
        if res.status().as_u16() != 404 {
            bail!("NoRisk's cape list returned HTTP {}", res.status());
        }
    }
    let page = legacy_page.unwrap_or(0);
    let mut query: Vec<(&str, String)> = vec![
        ("uuid", uuid),
        ("page", page.to_string()),
        ("pageSize", "30".into()),
        ("sortBy", sort.into()),
    ];
    if let Some(t) = time_frame.filter(|t| !t.is_empty()) {
        query.push(("timeFrame", t.into()));
    }
    let body: Value = HTTP
        .get(format!("{COSMETICS}/cape/browse"))
        .timeout(Duration::from_secs(30))
        .bearer_auth(&token)
        .query(&query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let total_pages = body["pagination"]["totalPages"].as_i64().unwrap_or(0);
    Ok(NoRiskCapePage {
        capes: body["capes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(parse_cape)
            .collect(),
        next_cursor: (i64::from(page) + 1 < total_pages)
            .then(|| format!("page:{}", page + 1)),
    })
}

/// Capes this player made.
pub async fn capes_mine() -> crate::Result<Vec<NoRiskCape>> {
    let creds = super::default_credentials().await?;
    let token = token(&creds).await?;
    let uuid = creds.offline_profile.id.to_string();
    let body: Value = HTTP
        .get(format!("{COSMETICS}/cape/user/{uuid}"))
        .timeout(Duration::from_secs(30))
        .bearer_auth(&token)
        .query(&[("uuid", uuid.as_str())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(body
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_cape)
        .collect())
}

pub async fn cape_equip(hash: &str) -> crate::Result<()> {
    let creds = super::default_credentials().await?;
    let token = token(&creds).await?;
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("invalid cape");
    }
    HTTP.post(format!("{COSMETICS}/cape/{hash}/equip"))
        .timeout(Duration::from_secs(30))
        .bearer_auth(&token)
        .query(&[("uuid", creds.offline_profile.id.to_string())])
        .send()
        .await?
        .error_for_status()
        .context("NoRisk couldn't equip that cape")?;
    Ok(())
}

pub async fn cape_unequip() -> crate::Result<()> {
    let creds = super::default_credentials().await?;
    let token = token(&creds).await?;
    HTTP.delete(format!("{COSMETICS}/cape/unequip"))
        .timeout(Duration::from_secs(30))
        .bearer_auth(&token)
        .query(&[("uuid", creds.offline_profile.id.to_string())])
        .send()
        .await?
        .error_for_status()
        .context("NoRisk couldn't take your cape off")?;
    Ok(())
}

/// Uploads a cape PNG; it's reviewed by NoRisk before others can see it.
pub async fn cape_upload(path: &Path) -> crate::Result<String> {
    let creds = super::default_credentials().await?;
    let token = token(&creds).await?;
    let bytes = tokio::fs::read(path).await?;
    let hash = HTTP
		.post(format!("{COSMETICS}/cape"))
		.timeout(Duration::from_secs(60))
		.bearer_auth(&token)
		.query(&[("uuid", creds.offline_profile.id.to_string())])
		.body(bytes)
		.send()
		.await?
		.error_for_status()
		.context(
			"NoRisk didn't accept that image (capes must be a 64×32 or 512×256 PNG)",
		)?
		.text()
		.await?;
    Ok(hash.trim().trim_matches('"').to_string())
}

/// A cape's PNG as a data URL, so the 3D preview can use it without CORS.
pub async fn cape_texture(hash: &str, review: bool) -> crate::Result<String> {
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_alphanumeric()) {
        bail!("invalid cape");
    }
    let bucket = if review { "review" } else { "prod" };
    let bytes = super::get_bytes(&format!(
        "https://cdn.norisk.gg/capes/{bucket}/{hash}.png"
    ))
    .await?;
    use base64::Engine;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_inheritance_overrides_and_excludes_mods() {
        let config = json!({"packs": {
            "base": {"mods": [{"id": "keep", "displayName": "old"}, {"id": "remove"}]},
            "child": {"inheritsFrom": ["base"], "mods": [{"id": "keep", "displayName": "new"}], "excludeMods": ["remove"]}
        }});
        let mods = resolve_mods(&config, "child", &mut HashSet::new()).unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods["keep"]["displayName"], "new");
    }

    #[test]
    fn cyclic_pack_inheritance_is_rejected() {
        let config = json!({"packs": {"a": {"inheritsFrom": ["b"]}, "b": {"inheritsFrom": ["a"]}}});
        assert!(resolve_mods(&config, "a", &mut HashSet::new()).is_err());
    }

    #[test]
    fn missing_packs_are_an_error_and_diamonds_are_fine() {
        let config = json!({"packs": {
            "base": {"mods": [{"id": "core"}], "assets": ["shared"]},
            "left": {"inheritsFrom": ["base"], "assets": ["left", "shared"]},
            "right": {"inheritsFrom": ["base"], "assets": ["right"]},
            "top": {"inheritsFrom": ["left", "right"], "mods": [{"id": "extra"}]}
        }});
        let mods = resolve_mods(&config, "top", &mut HashSet::new()).unwrap();
        let mut ids: Vec<&str> = mods.keys().map(String::as_str).collect();
        ids.sort();
        assert_eq!(ids, ["core", "extra"]);
        assert!(resolve_mods(&config, "gone", &mut HashSet::new()).is_err());
        // Assets: parents first, no duplicates, cycles just stop.
        assert_eq!(
            resolve_assets(&config, "top", &mut HashSet::new()),
            ["shared", "left", "right"]
        );
        let cyclic = json!({"packs": {"a": {"inheritsFrom": ["a"], "assets": ["x"]}}});
        assert_eq!(resolve_assets(&cyclic, "a", &mut HashSet::new()), ["x"]);
    }

    #[test]
    fn version_ids_split_into_pack_and_game_version() {
        assert_eq!(split_id("norisk-bughunter|1.21.10"), ("norisk-bughunter".into(), "1.21.10".into()));
        assert_eq!(split_id("1.21.4"), ("norisk-prod".into(), "1.21.4".into()));
    }

    #[test]
    fn fields_are_read_under_either_spelling() {
        let v = json!({"gameVersion": "1.21.10", "pack": "p"});
        assert_eq!(str_field(&v, &["game_version", "gameVersion"]), Some("1.21.10"));
        assert_eq!(str_field(&v, &["missing"]), None);
    }

    #[tokio::test]
    async fn maven_and_url_mods_resolve_without_network() {
        let config = json!({"repositories": {"norisk": "https://maven.norisk.gg/repository/norisk-production/"}});
        let maven = json!({
            "source": {"type": "maven", "repositoryRef": "norisk", "groupId": "gg.norisk", "artifactId": "nrc-client"},
            "compatibility": {"1.21.10": {"fabric": {"identifier": "1.2.3"}}}
        });
        assert_eq!(
            mod_source(&maven, "1.21.10", &config).await.unwrap(),
            Some((
                "https://maven.norisk.gg/repository/norisk-production/gg/norisk/nrc-client/1.2.3/nrc-client-1.2.3.jar".to_string(),
                "nrc-client-1.2.3.jar".to_string()
            ))
        );
        let url = json!({
            "source": {"type": "url"},
            "compatibility": {"1.21.10": {"fabric": {"identifier": "https://example.com/a.jar", "filename": "a.jar"}}}
        });
        assert_eq!(
            mod_source(&url, "1.21.10", &config).await.unwrap(),
            Some(("https://example.com/a.jar".to_string(), "a.jar".to_string()))
        );
        // Not available for this game version: skipped, not an error.
        assert_eq!(mod_source(&url, "1.8.9", &config).await.unwrap(), None);
        // Unknown repository or source type: errors.
        let bad_repo = json!({
            "source": {"type": "maven", "repositoryRef": "nope"},
            "compatibility": {"1.21.10": {"fabric": {"identifier": "1"}}}
        });
        assert!(mod_source(&bad_repo, "1.21.10", &config).await.is_err());
        let odd = json!({"source": {"type": "ftp"}, "compatibility": {"1.21.10": {"fabric": {"identifier": "1"}}}});
        assert!(mod_source(&odd, "1.21.10", &config).await.is_err());
        let nameless = json!({"source": {"type": "url"}, "compatibility": {"1.21.10": {"fabric": {"identifier": "https://x"}}}});
        assert!(mod_source(&nameless, "1.21.10", &config).await.is_err());
    }

    #[test]
    fn asset_files_stay_inside_their_folders() {
        let game = Path::new("game");
        let assets = Path::new("game/NoRiskClient/assets");
        assert_eq!(
            asset_target(game, assets, "nrc/textures/a.png"),
            Some((assets.join("nrc/textures/a.png"), false))
        );
        assert_eq!(
            asset_target(game, assets, "overrides/options.txt"),
            Some((game.join("options.txt"), true))
        );
        for bad in [
            "../escape.png",
            "overrides/../../escape",
            "/etc/passwd",
            "C:/Windows/evil",
            "overrides/",
            "",
            "a//b",
        ] {
            assert_eq!(asset_target(game, assets, bad), None, "{bad}");
        }
    }

    #[test]
    fn blog_posts_become_news_cards() {
        let posts = json!([
            {
                "date": "2026-11-01T10:00:00",
                "link": "https://blog.norisk.gg/x",
                "title": {"rendered": "Raw &amp; title"},
                "yoast_head_json": {
                    "title": "Update 2.0 - NoRisk Client Blog",
                    "og_url": "https://blog.norisk.gg/update-2",
                    "og_image": [{"url": "https://blog.norisk.gg/img.png"}],
                    "og_description": "<p>Big   update</p>"
                }
            },
            {"title": {"rendered": "Only a title"}, "link": "https://blog.norisk.gg/y"},
            {"no": "title"}
        ]);
        let news = parse_blog_posts(&posts);
        assert_eq!(news.len(), 2);
        assert_eq!(news[0].title, "Update 2.0");
        assert_eq!(news[0].url.as_deref(), Some("https://blog.norisk.gg/update-2"));
        assert_eq!(news[0].image.as_deref(), Some("https://blog.norisk.gg/img.png"));
        assert_eq!(news[0].summary.as_deref(), Some("Big update"));
        assert_eq!(news[1].title, "Only a title");
        assert_eq!(news[1].url.as_deref(), Some("https://blog.norisk.gg/y"));
        assert!(parse_blog_posts(&json!({"code": "rest_error"})).is_empty());
    }

    #[test]
    fn capes_parse_from_both_api_versions() {
        let new = parse_cape(&json!({"_id": "abc123", "uses": 5, "elytra": false, "creationDate": 9})).unwrap();
        assert_eq!((new.hash.as_str(), new.uses, new.elytra, new.accepted, new.creation_date), ("abc123", 5, false, true, 9));
        let old = parse_cape(&json!({"hash": "def"})).unwrap();
        assert_eq!(old.hash, "def");
        assert!(parse_cape(&json!({"uses": 1})).is_none());
    }

    #[tokio::test]
    async fn cape_hashes_are_checked_before_any_request() {
        for bad in ["", "../x", "a/b", "abc?x=1"] {
            assert!(cape_texture(bad, false).await.is_err(), "{bad}");
        }
    }

    #[test]
    fn version_identifiers_are_escaped_for_urls() {
        assert_eq!(urlencoding_lite("1.0+mc1.21 beta/2#x?y"), "1.0%2Bmc1.21%20beta%2F2%23x%3Fy");
        assert_eq!(urlencoding_lite("plain-1.0"), "plain-1.0");
    }

    #[tokio::test]
    async fn the_pack_config_falls_back_to_the_shared_copy() {
        let instance = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        assert!(read_pack_config(instance.path(), root.path()).await.is_err());
        tokio::fs::write(root.path().join("modpacks.json"), b"shared").await.unwrap();
        assert_eq!(read_pack_config(instance.path(), root.path()).await.unwrap(), b"shared");
        tokio::fs::create_dir_all(instance.path().join(".studio")).await.unwrap();
        tokio::fs::write(instance.path().join(".studio/norisk-pack.json"), b"own").await.unwrap();
        assert_eq!(read_pack_config(instance.path(), root.path()).await.unwrap(), b"own");
    }

    // ---- live checks against NoRisk / Modrinth: `cargo test -- --ignored` ----

    #[tokio::test]
    #[ignore = "live: talks to NoRisk's blog"]
    async fn live_norisk_news_loads() {
        let news = news().await.expect("NoRisk's blog");
        assert!(!news.is_empty());
        for item in &news {
            assert!(!item.title.is_empty());
            assert!(item.url.as_deref().is_some_and(|u| u.starts_with("https://")), "{}", item.title);
        }
    }

    #[tokio::test]
    #[ignore = "live: talks to Modrinth's API"]
    async fn live_modrinth_mods_fall_back_to_the_newest_build() {
        let entry = json!({
            "source": {"type": "modrinth", "projectSlug": "sodium"},
            "compatibility": {"1.21.1": {"fabric": {"identifier": "no-such-version"}}}
        });
        let (url, name) = mod_source(&entry, "1.21.1", &json!({})).await.unwrap().unwrap();
        assert!(url.starts_with("https://cdn.modrinth.com/"), "{url}");
        assert!(name.ends_with(".jar") && super::super::safe_file_name(&name), "{name}");
    }

    #[tokio::test]
    #[ignore = "live: talks to NoRisk's CDN"]
    async fn live_norisk_icon_is_reachable() {
        let bytes = super::super::get_bytes("https://norisk.gg/icon-192.png").await.unwrap();
        assert!(bytes.starts_with(b"\x89PNG"), "not a PNG");
    }
}
