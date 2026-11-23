//! Horizon: Minecraft: Java Edition 26.3 rewritten in Rust
//! (github.com/horizon-rust/horizon, MIT).
//!
//! Horizon ships no game files. Its builds come from the project's GitHub
//! releases, and `horizon-prepare` (shipped with every build) turns the
//! player's own 26.3 files into Horizon's resources. Modrinth already
//! downloads those files for the 26.3 instance this is attached to, so they
//! are handed to the preparer one by one (`--version-json`, `--client-jar`,
//! `--libraries-dir`, `--assets-dir`), the way Horizon's docs describe for
//! "a launcher whose files are laid out its own way". The game is then
//! `horizon-client --resources <dir>` with the instance folder as its home.
//!
//! Horizon has its own mod system (Rust compiled to WebAssembly) and its own
//! registry, so Java mods from Modrinth don't run in it; [`horizon_store_list`]
//! and [`horizon_store_install`] read that registry instead.

use super::{
    ClientInstall, ClientVersion, LaunchSettings, Progress, client_root,
    download_checked, extract_zip, get_bytes, get_json, java_command, java_for,
    mojang_version_json, sha256_hex,
};
use crate::State;
use crate::state::Credentials;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const REPO: &str = "horizon-rust/horizon";
const GAME_VERSION: &str = "26.3";
const INDEX_URL: &str =
    "https://horizon-rust.github.io/horizon-mods/index.json";

fn asset_name() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("horizon-windows-x64.zip"),
        ("windows", "aarch64") => Some("horizon-windows-arm64.zip"),
        ("macos", "aarch64") => Some("horizon-macos-arm64.zip"),
        ("macos", "x86_64") => Some("horizon-macos-x64.zip"),
        _ => None,
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub(super) async fn list_versions() -> crate::Result<Vec<ClientVersion>> {
    let Some(asset) = asset_name() else {
        bail!("Horizon isn't available for this system here yet");
    };
    let releases = get_json(&format!(
        "https://api.github.com/repos/{REPO}/releases?per_page=40"
    ))
    .await
    .context("Horizon's releases couldn't be loaded")?;
    let out = versions_from_releases(&releases, asset);
    if out.is_empty() {
        bail!("No Horizon builds were found");
    }
    Ok(out)
}

fn is_game_release(tag: &str) -> bool {
    tag == "nightly" || tag.starts_with("nightly-") || tag.starts_with('v')
}

/// Game builds (not launcher releases or drafts) that have a download for
/// this system.
fn versions_from_releases(releases: &Value, asset: &str) -> Vec<ClientVersion> {
    let mut out = Vec::new();
    for rel in releases.as_array().into_iter().flatten() {
        let Some(tag) = rel["tag_name"].as_str() else {
            continue;
        };
        if !is_game_release(tag) || rel["draft"].as_bool() == Some(true) {
            continue;
        }
        let has_asset = rel["assets"].as_array().is_some_and(|a| {
            a.iter().any(|x| x["name"].as_str() == Some(asset))
        });
        if !has_asset {
            continue;
        }
        let name = rel["name"].as_str().unwrap_or(tag).to_string();
        out.push(ClientVersion {
            id: tag.to_string(),
            name,
            game_version: GAME_VERSION.into(),
            loader: "vanilla".into(),
            detail: Some(if tag == "nightly" {
                "Updates automatically".into()
            } else {
                "Stays on this build".into()
            }),
            recommended: tag == "nightly",
        });
    }
    out
}

/// Where the current build of this install lives, and its resources.
#[derive(Serialize, Deserialize)]
struct Prepared {
    build_dir: PathBuf,
    resources_dir: PathBuf,
}

fn prepared_path(root: &Path, tag: &str) -> PathBuf {
    root.join(format!("prepared-{tag}.json"))
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(found) = find_file(&path, name)
        {
            return Some(found);
        }
    }
    None
}

pub(super) async fn prepare(
    install: &ClientInstall,
    _instance_dir: &Path,
    progress: &Progress<'_>,
) -> crate::Result<()> {
    let state = State::get().await?;
    let root = client_root(super::ClientKind::Horizon).await?;
    let tag = install.version_id.clone();
    let asset_name = asset_name()
        .context("Horizon isn't available for this system here yet")?;

    // 1. The build.
    progress.step(5.0, "Checking for Horizon updates");
    let release = get_json(&format!(
        "https://api.github.com/repos/{REPO}/releases/tags/{tag}"
    ))
    .await?;
    let asset = release["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(asset_name)))
        .context("this Horizon build has no download for your system")?;
    let asset_id = asset["id"].as_u64().unwrap_or_default();
    let url = asset["browser_download_url"]
        .as_str()
        .context("no download link")?;
    let digest = asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(String::from);

    let build_dir = root.join("builds").join(format!("{tag}-{asset_id}"));
    if !build_dir.join(".done").exists() {
        progress.step(20.0, "Downloading Horizon");
        let bytes = get_bytes(url).await?;
        if let Some(expected) = &digest
            && !sha256_hex(&bytes).eq_ignore_ascii_case(expected)
        {
            bail!("Horizon's download didn't match its checksum");
        }
        let _ = tokio::fs::remove_dir_all(&build_dir).await;
        extract_zip(bytes.to_vec(), build_dir.clone()).await?;
        tokio::fs::write(build_dir.join(".done"), b"").await?;
    }
    let preparer = {
        let dir = build_dir.clone();
        tokio::task::spawn_blocking(move || {
            find_file(&dir, &exe("horizon-prepare"))
        })
        .await?
    }
    .context("this Horizon build has no horizon-prepare")?;

    // 2. Its resources, made from the instance's own 26.3 files.
    let resources = root.join("resources").join(format!("{tag}-{asset_id}"));
    tokio::fs::create_dir_all(&resources).await?;
    let check = Command::new(&preparer)
        .arg("--check")
        .arg("--output")
        .arg(&resources)
        .current_dir(preparer.parent().unwrap_or(&build_dir))
        .output()
        .await;
    let ready = matches!(&check, Ok(out) if out.status.success());

    if !ready {
        progress.step(10.0, "Collecting your Minecraft 26.3 files");
        let version_json = mojang_version_json(GAME_VERSION, &root).await?;
        let version_json_path =
            root.join("mojang").join(format!("{GAME_VERSION}.json"));

        let mut client_jar = state
            .directories
            .version_dir(GAME_VERSION)
            .join(format!("{GAME_VERSION}.jar"));
        if !client_jar.exists() {
            client_jar =
                root.join("mojang").join(format!("{GAME_VERSION}.jar"));
            download_checked(
                version_json["downloads"]["client"]["url"]
                    .as_str()
                    .context("no client download")?,
                &client_jar,
                version_json["downloads"]["client"]["sha1"].as_str(),
            )
            .await?;
        }
        let server_jar = root
            .join("mojang")
            .join(format!("{GAME_VERSION}-server.jar"));
        progress.step(10.0, "Downloading the 26.3 server jar (needed once)");
        download_checked(
            version_json["downloads"]["server"]["url"]
                .as_str()
                .context("no server download")?,
            &server_jar,
            version_json["downloads"]["server"]["sha1"].as_str(),
        )
        .await?;

        progress.step(5.0, "Getting Java 25");
        let java = java_for(25).await?;
        let java_home = Path::new(&java.path)
            .parent()
            .and_then(|bin| bin.parent())
            .context("couldn't find Java's folder")?
            .to_path_buf();

        progress.step(
			10.0,
			"Preparing Horizon's resources (takes a few minutes the first time)",
		);
        let run = |mode: &'static str| {
            let mut cmd = Command::new(&preparer);
            cmd.arg(mode)
                .arg("--output")
                .arg(&resources)
                .arg("--version-json")
                .arg(&version_json_path)
                .arg("--client-jar")
                .arg(&client_jar)
                .arg("--libraries-dir")
                .arg(state.directories.libraries_dir())
                .arg("--assets-dir")
                .arg(state.directories.assets_dir())
                .arg("--server-jar")
                .arg(&server_jar)
                .arg("--java-home")
                .arg(&java_home)
                .env("JAVA_HOME", &java_home)
                .current_dir(preparer.parent().unwrap_or(&build_dir));
            cmd
        };
        let out = run("--complete").output().await?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            bail!(
                "Horizon couldn't prepare its resources ({}):\n{}\n{}",
                out.status,
                stderr.trim(),
                stdout.trim(),
            );
        }
    }

    let prepared = Prepared {
        build_dir,
        resources_dir: resources,
    };
    tokio::fs::write(
        prepared_path(&root, &tag),
        serde_json::to_vec_pretty(&prepared)?,
    )
    .await?;
    Ok(())
}

pub(super) async fn command(
    install: &ClientInstall,
    instance_dir: &Path,
    credentials: &Credentials,
    settings: &LaunchSettings<'_>,
) -> crate::Result<Command> {
    let root = client_root(super::ClientKind::Horizon).await?;
    let prepared: Prepared = serde_json::from_slice(
        &tokio::fs::read(prepared_path(&root, &install.version_id))
            .await
            .context("Horizon isn't prepared yet")?,
    )?;
    let client = {
        let dir = prepared.build_dir.clone();
        tokio::task::spawn_blocking(move || {
            find_file(&dir, &exe("horizon-client"))
        })
        .await?
    }
    .context("this Horizon build has no horizon-client")?;

    // Mods installed from Modrinth Studios start on. One the player switched
    // off in Options > Mods already has a line and is left alone.
    if let Ok(mut dir) = tokio::fs::read_dir(instance_dir.join("horizon-mods")).await {
        let listed = tokio::fs::read_to_string(instance_dir.join("horizon-mods.txt"))
            .await
            .unwrap_or_default();
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("hmod") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !listed.lines().any(|l| l.starts_with(&format!("mod.{id}:"))) {
                set_mod_enabled(instance_dir, id, true).await?;
            }
        }
    }

    // The wrapper hook works the same way, just around Horizon's binary.
    let mut cmd = java_command(&client.to_string_lossy(), settings.wrapper)?;
    cmd.current_dir(client.parent().unwrap_or(&prepared.build_dir));
    cmd.env("HORIZON_HOME", instance_dir);
    cmd.arg("--game-dir").arg(instance_dir);
    cmd.arg("--player-dir").arg(instance_dir);
    cmd.env(
        "HORIZON_MEMORY_LIMIT_MB",
        settings.memory.maximum.to_string(),
    );
    cmd.arg("--resources").arg(&prepared.resources_dir);
    // The signed-in account, handed over the way Horizon's own launcher does
    // (docs/microsoft-account.md, "Handing the session to the game"): a
    // short-lived session file the game reads and deletes at start, so the
    // token never shows in the process list. With it Horizon shows your skin
    // and cape and can join online-mode servers. If the file can't be
    // written, the game still starts offline under your name.
    // Passed through HORIZON_SESSION_FILE (same as `--session-file`), so a
    // build from before session support just ignores it and plays offline.
    match write_session_file(instance_dir, credentials).await {
        Ok(path) => {
            cmd.env("HORIZON_SESSION_FILE", path);
        }
        Err(e) => {
            tracing::warn!("Horizon session file couldn't be written: {e:#}");
        }
    }
    cmd.arg("--name").arg(&credentials.offline_profile.name);
    Ok(cmd)
}

async fn write_session_file(
    instance_dir: &Path,
    credentials: &Credentials,
) -> crate::Result<PathBuf> {
    let dir = instance_dir.join("sessions");
    tokio::fs::create_dir_all(&dir).await?;
    // Old files the game never got to read (it refuses them after two minutes).
    if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("session-") && name.ends_with(".json") {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
    let path = dir.join(format!("session-{}.json", uuid::Uuid::new_v4().simple()));
    let body = serde_json::json!({
        "name": credentials.offline_profile.name,
        "uuid": credentials.offline_profile.id.simple().to_string(),
        "access_token": credentials.access_token,
        "user_type": "msa",
        "xuid": "0",
    });
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    tokio::io::AsyncWriteExt::write_all(&mut file, &serde_json::to_vec(&body)?)
        .await?;
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    Ok(path)
}

// ---------- Horizon's mod registry ----------

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HorizonProject {
    pub id: String,
    /// "mod", "resourcepack" or "shader".
    pub kind: String,
    pub name: String,
    pub summary: String,
    pub icon: Option<String>,
    pub authors: Vec<String>,
    pub downloads: u64,
    pub published: Option<String>,
    pub categories: Vec<String>,
    pub version: String,
    pub source: Option<String>,
    pub signed: bool,
    pub installed: bool,
}

fn install_target(
    instance_dir: &Path,
    kind: &str,
    id: &str,
    file_name: &str,
) -> Option<PathBuf> {
    if !super::safe_file_name(id) || !super::safe_file_name(file_name) {
        return None;
    }
    Some(match kind {
        "mod" => instance_dir.join("horizon-mods").join(format!("{id}.hmod")),
        "resourcepack" => instance_dir.join("resourcepacks").join(file_name),
        "shader" => instance_dir.join("shaderpacks").join(file_name),
        _ => return None,
    })
}

/// Projects in Horizon's registry that can be installed by dropping a file in
/// (content mods, resource packs, shader packs). Native mods need Horizon to
/// rebuild itself and modpacks need Horizon's launcher, so they're left out.
pub async fn horizon_store_list(
    instance_id: Option<&str>,
) -> crate::Result<Vec<HorizonProject>> {
    let index = get_json(INDEX_URL)
        .await
        .context("Horizon's mod registry isn't reachable")?;
    let instance_dir = match instance_id {
        Some(id) => Some(crate::api::instance::get_full_path(id).await?),
        None => None,
    };
    let mut out = Vec::new();
    for p in index["projects"].as_array().into_iter().flatten() {
        let kind = p["kind"].as_str().unwrap_or_default();
        if !matches!(kind, "mod" | "resourcepack" | "shader") {
            continue;
        }
        let Some(latest) = p["versions"].as_array().and_then(|v| v.first())
        else {
            continue;
        };
        if latest["game"].as_str().is_some_and(|g| g != GAME_VERSION) {
            continue;
        }
        if kind == "mod" {
            // Same rule as Horizon's launcher: content mods that only register things.
            let caps: Vec<&str> = latest["capabilities"]
                .as_array()
                .map(|c| c.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            if caps.iter().any(|c| *c != "registries") {
                continue;
            }
            // "(assets)" halves of native mods do nothing without their native
            // half, which only Horizon's own launcher can compile in.
            // Ones already installed stay listed so they can be removed.
            let pid = p["id"].as_str().unwrap_or_default();
            let installed_here = instance_dir.as_deref().is_some_and(|dir| {
                super::safe_file_name(pid)
                    && dir.join("horizon-mods").join(format!("{pid}.hmod")).exists()
            });
            if needs_native_half(p) && !installed_here {
                continue;
            }
        }
        let id = p["id"].as_str().unwrap_or_default().to_string();
        let installed_release = p["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|version| {
                instance_dir
                    .as_deref()
                    .and_then(|dir| {
                        install_target(
                            dir,
                            kind,
                            &id,
                            version["file"]["name"]
                                .as_str()
                                .unwrap_or_default(),
                        )
                    })
                    .is_some_and(|path| path.exists())
            });
        let installed = installed_release.is_some();
        let displayed_version = installed_release.unwrap_or(latest);
        out.push(HorizonProject {
            kind: kind.to_string(),
            name: p["name"].as_str().unwrap_or(&id).to_string(),
            summary: p["summary"].as_str().unwrap_or_default().to_string(),
            icon: p["icon"].as_str().map(String::from),
            authors: p["authors"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            downloads: p["downloads"].as_u64().unwrap_or_default(),
            published: latest["published"].as_str().map(String::from),
            categories: p["categories"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            version: displayed_version["version"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            source: p["source"].as_str().map(String::from),
            signed: p["key"].as_str().is_some_and(|k| !k.is_empty()),
            installed,
            id,
        });
    }
    out.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    Ok(out)
}

/// Downloads the newest version of a registry project into the instance.
/// The file's SHA-256 must match the registry's; it is checked before the
/// file is put in place.
pub async fn horizon_store_install(
    instance_id: &str,
    project_id: &str,
) -> crate::Result<()> {
    let index = get_json(INDEX_URL).await?;
    let project: &Value = index["projects"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["id"].as_str() == Some(project_id)))
        .context("that project isn't in Horizon's registry anymore")?;
    let latest = project["versions"]
        .as_array()
        .and_then(|v| v.first())
        .context("that project has no versions")?;
    let kind = project["kind"].as_str().unwrap_or_default();
    if latest["game"]
        .as_str()
        .is_some_and(|game| game != GAME_VERSION)
        || (kind == "mod"
            && latest["capabilities"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|cap| cap.as_str() != Some("registries")))
    {
        bail!("This project requires a different Horizon runtime");
    }
    let file = &latest["file"];
    let sha = file["sha256"]
        .as_str()
        .context("registry entry without checksum")?;
    let revoked = index["revoked"].as_array().into_iter().flatten().any(|r| {
        r["id"].as_str() == Some(project_id)
            && r["sha256"]
                .as_str()
                .is_none_or(|s| s.eq_ignore_ascii_case(sha))
    });
    if revoked {
        bail!("Horizon's registry has taken this file down");
    }
    let url = file["url"]
        .as_str()
        .context("registry entry without download")?;
    if !url.starts_with("https://github.com/") {
        bail!("unexpected download location for a registry file");
    }
    let instance_dir = horizon_instance_dir(instance_id).await?;
    let target = install_target(
        &instance_dir,
        kind,
        project_id,
        file["name"].as_str().unwrap_or_default(),
    )
    .context("this kind of project can't be installed here")?;

    let bytes = get_bytes(url).await?;
    if !sha256_hex(&bytes).eq_ignore_ascii_case(sha) {
        bail!("the downloaded file didn't match the registry's checksum");
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&target, bytes).await?;
    if kind == "mod" {
        // Horizon keeps mods off until they're switched on in Options > Mods;
        // installing one here means the player wants it on.
        set_mod_enabled(&instance_dir, project_id, true).await?;
    }
    Ok(())
}

fn needs_native_half(project: &Value) -> bool {
    let name = project["name"].as_str().unwrap_or_default().to_lowercase();
    let summary = project["summary"].as_str().unwrap_or_default().to_lowercase();
    name.ends_with("(assets)") || summary.contains("native mod")
}

/// `horizon-mods.txt` in the game folder holds `mod.<id>:true|false`.
async fn set_mod_enabled(
    instance_dir: &Path,
    id: &str,
    enabled: bool,
) -> crate::Result<()> {
    let path = instance_dir.join("horizon-mods.txt");
    let current = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let key = format!("mod.{id}:");
    let mut lines: Vec<String> = current
        .lines()
        .filter(|l| !l.starts_with(&key))
        .map(String::from)
        .collect();
    if enabled {
        lines.push(format!("{key}true"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    tokio::fs::write(&path, out).await?;
    Ok(())
}

/// Patch notes of recent Horizon builds, newest first.
pub(super) async fn news() -> crate::Result<Vec<super::NewsItem>> {
    let releases = get_json(&format!(
        "https://api.github.com/repos/{REPO}/releases?per_page=15"
    ))
    .await?;
    Ok(release_notes(&releases))
}

/// Up to 8 game releases as patch-note cards.
fn release_notes(releases: &Value) -> Vec<super::NewsItem> {
    let mut out = Vec::new();
    for rel in releases.as_array().into_iter().flatten() {
        let tag = rel["tag_name"].as_str().unwrap_or_default();
        if !is_game_release(tag) {
            continue;
        }
        let body = rel["body"].as_str().unwrap_or_default();
        // Skip the "download the launcher" banner the release pages start with.
        let body = body
            .split_once("### ")
            .map(|(_, rest)| format!("### {rest}"))
            .unwrap_or_else(|| body.to_string());
        out.push(super::NewsItem {
            title: rel["name"].as_str().unwrap_or(tag).to_string(),
            date: rel["published_at"].as_str().map(String::from),
            url: rel["html_url"].as_str().map(String::from),
            image: None,
            summary: None,
            body,
        });
        if out.len() >= 8 {
            break;
        }
    }
    out
}

async fn horizon_instance_dir(instance_id: &str) -> crate::Result<PathBuf> {
    let install = super::get(instance_id)
        .await?
        .context("This is not a client installation")?;
    if install.client != super::ClientKind::Horizon {
        bail!("Horizon content can only be managed in a Horizon instance");
    }
    let state = State::get().await?;
    if crate::state::instance_has_running_process(instance_id, &state).await? {
        bail!("Stop Horizon before changing its content");
    }
    crate::api::instance::get_full_path(instance_id).await
}

/// Removes an installed registry project from the instance.
pub async fn horizon_store_remove(
    instance_id: &str,
    project_id: &str,
) -> crate::Result<()> {
    let instance_dir = horizon_instance_dir(instance_id).await?;
    let index = get_json(INDEX_URL).await?;
    let project: &Value = index["projects"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["id"].as_str() == Some(project_id)))
        .context("that project isn't in Horizon's registry anymore")?;
    let kind = project["kind"].as_str().unwrap_or_default();
    for version in project["versions"].as_array().into_iter().flatten() {
        if let Some(path) = install_target(
            &instance_dir,
            kind,
            project_id,
            version["file"]["name"].as_str().unwrap_or_default(),
        ) && path.exists()
        {
            tokio::fs::remove_file(&path).await?;
        }
    }
    if kind == "mod" {
        set_mod_enabled(&instance_dir, project_id, false).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_paths_reject_traversal() {
        let root = Path::new("instance");
        assert!(
            install_target(root, "mod", "../escape", "safe.hmod").is_none()
        );
        assert!(
            install_target(root, "resourcepack", "safe", "../escape.zip")
                .is_none()
        );
        assert_eq!(
            install_target(root, "mod", "safe", "safe.hmod"),
            Some(root.join("horizon-mods/safe.hmod"))
        );
    }

    #[test]
    fn each_kind_of_content_has_its_folder() {
        let root = Path::new("instance");
        assert_eq!(
            install_target(root, "resourcepack", "p", "Pack.zip"),
            Some(root.join("resourcepacks").join("Pack.zip"))
        );
        assert_eq!(
            install_target(root, "shader", "s", "Shader.zip"),
            Some(root.join("shaderpacks").join("Shader.zip"))
        );
        // Native mods and modpacks aren't installed by dropping a file in.
        assert_eq!(install_target(root, "native", "n", "n.zip"), None);
        assert_eq!(install_target(root, "modpack", "m", "m.zip"), None);
    }

    fn release(tag: &str, assets: &[&str], draft: bool) -> Value {
        json!({
            "tag_name": tag,
            "name": format!("Build {tag}"),
            "draft": draft,
            "published_at": "2026-11-20T00:00:00Z",
            "html_url": format!("https://github.com/{REPO}/releases/tag/{tag}"),
            "body": "Get the launcher at horizon.example\n\n### New\n- Shaders",
            "assets": assets.iter().map(|a| json!({ "name": a })).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn only_game_builds_for_this_system_are_offered() {
        let asset = "horizon-windows-x64.zip";
        let releases = json!([
            release("nightly", &[asset, "horizon-macos-arm64.zip"], false),
            release("v0.4.0", &[asset], false),
            release("nightly-2026-11-01", &[asset], false),
            release("launcher-v1.0.0", &[asset], false),
            release("v0.5.0-draft", &[asset], true),
            release("v0.3.0", &["horizon-macos-arm64.zip"], false),
        ]);
        let versions = versions_from_releases(&releases, asset);
        let ids: Vec<&str> = versions.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["nightly", "v0.4.0", "nightly-2026-11-01"]);
        assert!(versions[0].recommended);
        assert!(!versions[1].recommended);
        assert!(versions.iter().all(|v| v.game_version == GAME_VERSION && v.loader == "vanilla"));
        assert!(versions_from_releases(&json!({ "message": "rate limited" }), asset).is_empty());
    }

    #[test]
    fn patch_notes_skip_the_launcher_banner_and_stop_at_eight() {
        let mut list: Vec<Value> = (0..12).map(|i| release(&format!("v0.{i}.0"), &[], false)).collect();
        list.insert(1, release("launcher-v2", &[], false));
        let notes = release_notes(&Value::Array(list));
        assert_eq!(notes.len(), 8);
        assert_eq!(notes[0].title, "Build v0.0.0");
        assert_eq!(notes[0].body, "### New\n- Shaders");
        assert!(notes[0].url.as_deref().unwrap().starts_with("https://github.com/"));
        assert!(notes.iter().all(|n| !n.title.contains("launcher")));
    }

    #[test]
    fn asset_only_projects_are_recognised() {
        assert!(needs_native_half(&json!({ "name": "Better Water (assets)" })));
        assert!(needs_native_half(&json!({ "name": "X", "summary": "Needs the native mod too" })));
        assert!(!needs_native_half(&json!({ "name": "Shiny pack", "summary": "Textures" })));
    }

    #[tokio::test]
    async fn mod_switches_are_written_one_per_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("horizon-mods.txt");
        tokio::fs::write(&file, "mod.other:true\nmod.minimap:false\n").await.unwrap();
        set_mod_enabled(dir.path(), "minimap", true).await.unwrap();
        set_mod_enabled(dir.path(), "minimap", true).await.unwrap();
        assert_eq!(
            tokio::fs::read_to_string(&file).await.unwrap(),
            "mod.other:true\nmod.minimap:true\n"
        );
        set_mod_enabled(dir.path(), "minimap", false).await.unwrap();
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), "mod.other:true\n");
        // A mod whose id starts the same way isn't touched.
        set_mod_enabled(dir.path(), "other2", true).await.unwrap();
        set_mod_enabled(dir.path(), "other", false).await.unwrap();
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), "mod.other2:true\n");
    }

    #[test]
    fn files_are_found_in_nested_folders() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b").join(exe("horizon")), b"").unwrap();
        assert_eq!(
            find_file(dir.path(), &exe("horizon")),
            Some(dir.path().join("a/b").join(exe("horizon")))
        );
        assert_eq!(find_file(dir.path(), "missing"), None);
        assert!(exe("x").starts_with('x'));
    }

    // ---- live checks against GitHub: `cargo test -- --ignored` ----

    #[tokio::test]
    #[ignore = "live: talks to GitHub"]
    async fn live_horizon_builds_and_notes_load() {
        if asset_name().is_some() {
            let versions = list_versions().await.expect("Horizon builds");
            assert!(versions.iter().any(|v| v.id == "nightly"), "no nightly build");
        }
        let notes = news().await.expect("Horizon release notes");
        assert!(!notes.is_empty());
    }

    #[tokio::test]
    #[ignore = "live: talks to GitHub Pages"]
    async fn live_horizon_registry_has_installable_projects() {
        let index = get_json(INDEX_URL).await.expect("Horizon's registry");
        let projects = index["projects"].as_array().expect("no project list");
        assert!(!projects.is_empty());
        for p in projects {
            let id = p["id"].as_str().expect("project without id");
            let kind = p["kind"].as_str().unwrap_or_default();
            for v in p["versions"].as_array().into_iter().flatten() {
                let file = v["file"]["name"].as_str().unwrap_or_default();
                if matches!(kind, "mod" | "resourcepack" | "shader") {
                    assert!(
                        install_target(Path::new("x"), kind, id, file).is_some(),
                        "{id}: unusable file name {file}"
                    );
                }
            }
        }
    }
}
