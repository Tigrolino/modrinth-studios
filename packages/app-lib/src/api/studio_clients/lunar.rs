//! Lunar Client, launched without Lunar's own launcher.
//!
//! Uses the two endpoints Lunar's launcher uses (no Lunar account needed):
//! `GET /launcher/metadata/versions/lunar` for the version list and
//! `POST /launcher/launch` for the files of one version + module. The launch
//! command follows what Lunar's launcher (and open-source launchers built
//! around it, like lunar-client-qt and the HMCL lunar-offline plugin) run:
//! `com.moonsworth.lunar.genesis.Genesis` with Lunar's jars on the class path.
//!
//! The game reads the signed-in account from Lunar's own
//! `~/.lunarclient/settings/game/accounts.json`, so the Minecraft account
//! that's selected in Modrinth is written there (the same thing Lunar's
//! launcher does) before every launch.
//!
//! Lunar can change any of this at any time, which would break launching
//! until it's updated here.

use super::{
    ClientInstall, ClientVersion, HTTP, LaunchSettings, Progress, client_root,
    download_checked, extract_zip, get_bytes, installation_id, java_command,
    java_for, mojang_version_json, node_arch, node_os, sha1_hex,
};
use crate::State;
use crate::state::Credentials;
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const API: &str = "https://api.lunarclientprod.com";
/// The launcher version Lunar's API is told we are; it rejects old ones.
const LAUNCHER_VERSION: &str = "3.5.2";

fn os_release() -> String {
    // Lunar only uses this for stats; a plausible value is enough.
    if cfg!(windows) {
        "10.0.22631".into()
    } else {
        "6.0.0".into()
    }
}

pub(super) async fn list_versions() -> crate::Result<Vec<ClientVersion>> {
    let url = format!(
        "{API}/launcher/metadata/versions/lunar?installation_id={}&os={}&arch={}&launcher_version={LAUNCHER_VERSION}&branch=master&branch_changed=true&private=true&os_release={}",
        installation_id().await,
        node_os(),
        node_arch(),
        os_release(),
    );
    let root = super::get_json(&url)
        .await
        .context("Lunar's version list isn't reachable right now")?;

    let mut out = Vec::new();
    for version in root["versions"].as_array().into_iter().flatten() {
        for sub in version["subversions"].as_array().into_iter().flatten() {
            let Some(gv) = sub["id"].as_str() else {
                continue;
            };
            let modules: Vec<&Value> = sub["modules"]
                .as_array()
                .map(|m| m.iter().collect())
                .unwrap_or_default();
            if modules.is_empty() {
                out.push(entry(
                    gv,
                    "lunar",
                    "Lunar",
                    sub["default"].as_bool().unwrap_or(false),
                ));
            }
            for module in modules {
                let id = module["id"].as_str().unwrap_or("lunar");
                let name = module["name"].as_str().unwrap_or(id);
                let default = sub["default"].as_bool().unwrap_or(false)
                    && module["default"].as_bool().unwrap_or(false);
                let mut e = entry(gv, id, name, default);
                // Lunar's Fabric module runs Fabric mods next to Lunar's own,
                // so the instance is a Fabric one and gets Modrinth's
                // Content tab and browse filters.
                if id == "fabric" || id == "forge" {
                    e.loader = id.into();
                }
                out.push(e);
            }
        }
    }
    if out.is_empty() {
        bail!("Lunar didn't return any versions");
    }
    Ok(out)
}

fn entry(
    gv: &str,
    module: &str,
    module_name: &str,
    recommended: bool,
) -> ClientVersion {
    ClientVersion {
        id: format!("{gv}|{module}"),
        name: gv.to_string(),
        game_version: gv.to_string(),
        loader: "vanilla".into(),
        detail: Some(module_name.to_string()),
        recommended,
    }
}

fn split_id(id: &str) -> (String, String) {
    match id.split_once('|') {
        Some((gv, module)) => (gv.to_string(), module.to_string()),
        None => (id.to_string(), "lunar".to_string()),
    }
}

async fn version_dir(install: &ClientInstall) -> crate::Result<PathBuf> {
    let (gv, module) = split_id(&install.version_id);
    Ok(client_root(super::ClientKind::Lunar)
        .await?
        .join(format!("{gv}-{module}")))
}

pub(super) async fn prepare(
    install: &ClientInstall,
    progress: &Progress<'_>,
) -> crate::Result<()> {
    let (gv, module) = split_id(&install.version_id);
    let dir = version_dir(install).await?;
    tokio::fs::create_dir_all(&dir).await?;

    progress.step(5.0, "Asking Lunar for the client files");
    let manifest = launch_manifest(&gv, &module).await?;
    let artifacts: Vec<Value> = manifest["launchTypeData"]["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if artifacts.is_empty() {
        bail!("Lunar didn't send any files for {gv}");
    }
    prepare_files(&manifest, &artifacts, &dir, progress).await?;
    // Asset index id for this Minecraft version (vanilla's assets are reused).
    let _ =
        mojang_version_json(&gv, &client_root(super::ClientKind::Lunar).await?)
            .await?;
    Ok(())
}

/// What Lunar's launcher gets from `POST /launcher/launch`: the files of one
/// version + module, its Java, menus and cosmetics indexes.
async fn launch_manifest(gv: &str, module: &str) -> crate::Result<Value> {
    let body = json!({
        "installation_id": installation_id().await,
        "os": node_os(),
        "os_release": os_release(),
        "arch": node_arch(),
        "branch": "master",
        "version": gv,
        "module": module,
        "launcher_version": LAUNCHER_VERSION,
        "hwid": "PUBLIC-HWID",
    });
    let res = HTTP
        .post(format!("{API}/launcher/launch"))
        .timeout(Duration::from_secs(60))
        .json(&body)
        .send()
        .await
        .context("Lunar's servers aren't reachable")?;
    let status = res.status();
    let manifest: Value = res.json().await.context("reading Lunar's reply")?;
    if !status.is_success() || manifest["success"].as_bool() == Some(false) {
        let msg = manifest["error"]["message"]
            .as_str()
            .or(manifest["error"]["short"].as_str())
            .unwrap_or("unknown error");
        bail!("Lunar refused the request ({status}): {msg}");
    }
    Ok(manifest)
}

async fn prepare_files(
    manifest: &Value,
    artifacts: &[Value],
    dir: &Path,
    progress: &Progress<'_>,
) -> crate::Result<()> {
    let total = artifacts.len().max(1) as f64;
    let natives_dir = dir.join("natives");
    for artifact in artifacts {
        let name = artifact["name"].as_str().unwrap_or_default();
        let url = artifact["url"].as_str().unwrap_or_default();
        let sha1 = artifact["sha1"].as_str();
        if !safe_artifact_path(name) || url.is_empty() {
            bail!("Lunar returned an invalid artifact");
        }
        progress.step(70.0 / total, &format!("Downloading {name}"));
        let path = dir.join(name);
        let fresh = download_checked(url, &path, sha1).await?;
        if artifact["type"].as_str() == Some("NATIVES")
            && (fresh || !natives_dir.exists())
        {
            let bytes = tokio::fs::read(&path).await?;
            extract_zip(bytes, natives_dir.clone()).await?;
        }
    }

    // The menus are a web UI shipped separately.
    if let Some(ui_url) = manifest["ui"]["sourceUrl"].as_str() {
        progress.step(10.0, "Downloading Lunar's menus");
        let ui_sha = manifest["ui"]["sourceSha1"].as_str().unwrap_or_default();
        let marker = dir.join("ui").join(".sha1");
        let current =
            tokio::fs::read_to_string(&marker).await.unwrap_or_default();
        if current.trim() != ui_sha || ui_sha.is_empty() {
            let bytes = get_bytes(ui_url).await?;
            if !ui_sha.is_empty()
                && !sha1_hex(&bytes).eq_ignore_ascii_case(ui_sha)
            {
                bail!("Lunar's menu files didn't match their checksum");
            }
            let _ = tokio::fs::remove_dir_all(dir.join("ui")).await;
            extract_zip(bytes.to_vec(), dir.join("ui")).await?;
            tokio::fs::write(&marker, ui_sha).await?;
        }
    }

    progress.step(5.0, "Downloading Lunar's menu assets");
    if let Err(e) = ensure_ui_assets(manifest, dir).await {
        // The game still starts without them; the menus just look plain.
        tracing::warn!("Lunar menu assets couldn't be updated: {e:#}");
    }
    progress.step(5.0, "Downloading Lunar cosmetics and emotes");
    if let Err(e) = ensure_textures(manifest).await {
        tracing::warn!("Lunar cosmetics couldn't be updated: {e:#}");
    }
    tokio::fs::write(
        dir.join("launch.json"),
        serde_json::to_vec_pretty(manifest)?,
    )
    .await?;
    Ok(())
}

/// The Java major version Lunar ships for this version, read from the JRE it
/// names in the launch manifest (e.g. `zulu17.70.201-ca-jre17.0.21.1-win_x64`).
fn lunar_java_major(manifest: &Value) -> Option<u32> {
    let jre = &manifest["jre"];
    let names = jre["executablePathInArchive"]
        .as_array()
        .and_then(|p| p.first())
        .and_then(Value::as_str)
        .into_iter()
        .chain(jre["download"]["url"].as_str());
    for name in names {
        for marker in ["jre", "jdk", "zulu"] {
            if let Some(i) = name.rfind(marker) {
                let digits: String = name[i + marker.len()..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if let Ok(major) = digits.parse::<u32>()
                    && (8..=40).contains(&major)
                {
                    return Some(major);
                }
            }
        }
    }
    None
}

fn safe_artifact_path(name: &str) -> bool {
    name.split('/').all(super::safe_file_name)
}

#[cfg(test)]
mod tests {
    use super::{lunar_java_major, safe_artifact_path};

    #[test]
    fn lunar_java_major_comes_from_the_jre_name() {
        let m = serde_json::json!({"jre": {
            "download": {"url": "https://cdn.azul.com/zulu/bin/zulu17.70.201-ca-jre17.0.21.1-win_x64.zip"},
            "executablePathInArchive": ["zulu17.70.201-ca-jre17.0.21.1-win_x64", "bin", "javaw.exe"]
        }});
        assert_eq!(lunar_java_major(&m), Some(17));
        let m = serde_json::json!({"jre": {"download": {
            "url": "https://cdn.azul.com/zulu/bin/zulu25.38.201-ca-jre25.0.5.1-win_x64.zip"}}});
        assert_eq!(lunar_java_major(&m), Some(25));
        assert_eq!(lunar_java_major(&serde_json::json!({})), None);
    }

    #[test]
    fn lunar_artifacts_allow_nested_files_but_not_path_escapes() {
        assert!(safe_artifact_path("lunar.jar"));
        assert!(safe_artifact_path("hypixel/skyblock/dungeon/rooms.json"));
        for path in [
            "",
            "../outside",
            "/absolute",
            "a/../../outside",
            "C:/outside",
            "a\\outside",
            "a//b",
            "a/./b",
            "a/.. /b",
        ] {
            assert!(!safe_artifact_path(path), "{path}");
        }
    }

    use super::{
        extract_nested_jars, launch_manifest, list_versions, merge_account,
        split_id, sync_mods,
    };
    use super::super::test_util::zip_bytes;
    use serde_json::json;

    #[test]
    fn version_ids_split_into_game_version_and_module() {
        assert_eq!(split_id("1.8.9|forge"), ("1.8.9".into(), "forge".into()));
        assert_eq!(split_id("1.21.11"), ("1.21.11".into(), "lunar".into()));
    }

    #[test]
    fn accounts_file_gets_the_player_without_losing_others() {
        let existing = json!({
            "activeAccountLocalId": "other",
            "accounts": {
                "other": { "minecraftProfile": { "id": "o", "name": "Other" }, "accessToken": "keep" },
                "abc-local": {
                    "minecraftProfile": { "id": "p1", "name": "Old" },
                    "remoteId": "remote-1",
                    "eligibleForMigration": true
                }
            },
            "somethingLunarAdded": 1
        });
        let out = merge_account(existing, "p1", "Steve", "tok", "2030-01-01T00:00:00.000Z").unwrap();
        assert_eq!(out["activeAccountLocalId"], "abc-local");
        assert_eq!(out["somethingLunarAdded"], 1);
        assert_eq!(out["accounts"]["other"]["accessToken"], "keep");
        let me = &out["accounts"]["abc-local"];
        assert_eq!(me["accessToken"], "tok");
        assert_eq!(me["accessTokenExpiresAt"], "2030-01-01T00:00:00.000Z");
        assert_eq!(me["minecraftProfile"], json!({ "id": "p1", "name": "Steve" }));
        assert_eq!(me["username"], "Steve");
        // Fields Lunar set itself are kept.
        assert_eq!(me["remoteId"], "remote-1");
        assert_eq!(me["eligibleForMigration"], true);
        assert_eq!(out["accounts"].as_object().unwrap().len(), 2);
    }

    #[test]
    fn a_new_accounts_file_gets_a_complete_entry() {
        let out = merge_account(json!({}), "p2", "Alex", "tok", "x").unwrap();
        assert_eq!(out["activeAccountLocalId"], "p2");
        let me = &out["accounts"]["p2"];
        for key in [
            "accessToken", "accessTokenExpiresAt", "localId", "minecraftProfile", "remoteId",
            "type", "username", "eligibleForMigration", "hasMultipleProfiles", "legacy",
            "persistent", "userProperites",
        ] {
            assert!(!me[key].is_null(), "missing {key}");
        }
    }

    #[test]
    fn unexpected_accounts_files_are_left_alone() {
        assert!(merge_account(json!([]), "p", "n", "t", "e").is_err());
        assert!(merge_account(json!({ "accounts": [] }), "p", "n", "t", "e").is_err());
    }

    #[tokio::test]
    async fn instance_mods_are_handed_to_lunar() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        let target = mods.join("forge-1.8.9");
        tokio::fs::create_dir_all(&target).await.unwrap();
        let inner = zip_bytes(&[("inner.txt", &b"hi"[..])]);
        let outer = zip_bytes(&[("META-INF/jars/library.jar", &inner[..]), ("a.class", &b"x"[..])]);
        tokio::fs::write(mods.join("minimap.jar"), &outer).await.unwrap();
        tokio::fs::write(mods.join("off.jar.disabled"), b"x").await.unwrap();
        tokio::fs::write(mods.join("notes.txt"), b"x").await.unwrap();
        tokio::fs::write(target.join("removed.jar"), b"old").await.unwrap();

        sync_mods(dir.path(), "forge", "1.8.9").await.unwrap();

        let mut names: Vec<String> = std::fs::read_dir(&target)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["dependencies", "minimap.jar"]);
        assert_eq!(std::fs::read(target.join("minimap.jar")).unwrap(), outer);
        assert!(target.join("dependencies").join("library.jar").exists());

        // Removing the mod from the instance removes Lunar's copy too.
        tokio::fs::remove_file(mods.join("minimap.jar")).await.unwrap();
        sync_mods(dir.path(), "forge", "1.8.9").await.unwrap();
        assert!(!target.join("minimap.jar").exists());
        assert!(!target.join("dependencies").join("library.jar").exists());
    }

    #[tokio::test]
    async fn unsafe_game_versions_never_touch_the_mods_folder() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::create_dir_all(dir.path().join("mods")).await.unwrap();
        sync_mods(dir.path(), "forge", "../escape").await.unwrap();
        assert!(!dir.path().join("escape").exists());
        assert!(!dir.path().join("mods").join("forge-../escape").exists());
    }

    #[test]
    fn nested_jars_are_unpacked_recursively_but_not_forever() {
        let dir = tempfile::tempdir().unwrap();
        let level3 = zip_bytes(&[("x", &b"x"[..])]);
        let level2 = zip_bytes(&[("META-INF/jars/c.jar", &level3[..])]);
        let level1 = zip_bytes(&[("META-INF/jars/b.jar", &level2[..]), ("other/not-nested.jar", &level3[..])]);
        extract_nested_jars(level1, dir.path(), 0);
        assert!(dir.path().join("b.jar").exists());
        assert!(dir.path().join("c.jar").exists());
        // Only jars under META-INF/jars/ are nested jars.
        assert!(!dir.path().join("not-nested.jar").exists());
        // Not a zip: ignored.
        extract_nested_jars(b"not a zip".to_vec(), dir.path(), 0);
    }

    // ---- live checks against Lunar's servers: `cargo test -- --ignored` ----

    #[tokio::test]
    #[ignore = "live: talks to Lunar's servers"]
    async fn live_lunar_lists_versions_and_modules() {
        let versions = list_versions().await.expect("Lunar's version list");
        assert!(versions.iter().any(|v| v.recommended), "no default version");
        assert!(versions.iter().any(|v| v.id == "1.8.9|forge" && v.loader == "forge"));
        assert!(versions.iter().any(|v| v.id.ends_with("|fabric") && v.loader == "fabric"));
        for v in &versions {
            let (gv, module) = split_id(&v.id);
            assert_eq!(gv, v.game_version);
            assert!(super::super::safe_file_name(&gv) && super::super::safe_file_name(&module), "{}", v.id);
        }
    }

    #[tokio::test]
    #[ignore = "live: talks to Lunar's servers"]
    async fn live_lunar_sends_launchable_files() {
        let versions = list_versions().await.unwrap();
        let newest = versions.iter().find(|v| v.recommended).unwrap();
        let mut picks = vec![("1.8.9".to_string(), "forge".to_string())];
        picks.push(split_id(&newest.id));
        for (gv, module) in picks {
            let manifest = launch_manifest(&gv, &module)
                .await
                .unwrap_or_else(|e| panic!("{gv} {module}: {e:#}"));
            let artifacts = manifest["launchTypeData"]["artifacts"].as_array().unwrap();
            assert!(!artifacts.is_empty(), "{gv} {module}: no files");
            for a in artifacts {
                let name = a["name"].as_str().unwrap_or_default();
                assert!(safe_artifact_path(name), "{gv}: unsafe file name {name}");
                assert!(a["url"].as_str().is_some_and(|u| u.starts_with("https://")), "{name}");
            }
            assert!(artifacts.iter().any(|a| a["type"] == "CLASS_PATH"), "{gv}: no class path");
            assert!(manifest["launchTypeData"]["mainClass"].is_string() || gv == "1.8.9");
            assert!(lunar_java_major(&manifest).is_some(), "{gv}: can't tell which Java");
            assert!(manifest["ui"]["sourceUrl"].is_string(), "{gv}: no menus");
        }
        // 1.8.9 keeps running on the Java Lunar's own launcher uses.
        let old = launch_manifest("1.8.9", "forge").await.unwrap();
        assert_eq!(lunar_java_major(&old), Some(17));
    }
}

pub(super) async fn command(
    install: &ClientInstall,
    instance_dir: &Path,
    credentials: &Credentials,
    settings: &LaunchSettings<'_>,
) -> crate::Result<Command> {
    let state = State::get().await?;
    let (gv, module) = split_id(&install.version_id);
    let dir = version_dir(install).await?;
    let manifest: Value = serde_json::from_slice(
        &tokio::fs::read(dir.join("launch.json"))
            .await
            .context("Lunar isn't downloaded yet")?,
    )?;
    let vanilla =
        mojang_version_json(&gv, &client_root(super::ClientKind::Lunar).await?)
            .await?;
    let asset_index = vanilla["assetIndex"]["id"]
        .as_str()
        .unwrap_or(&gv)
        .to_string();

    write_account(credentials).await?;
    let runs_mods = module == "fabric" || module == "forge";
    if runs_mods
        && let Err(e) = sync_mods(instance_dir, &module, &gv).await
    {
        tracing::warn!("Couldn't hand the instance's mods to Lunar: {e:#}");
    }

    // Use the Java that Lunar's own launcher would (17 for 1.8.9, 25 for
    // 26.x, ...). Lunar and the Forge mods it loads are only tested on that
    // one; 1.8.9 Forge on Java 21 breaks joining worlds.
    let major = lunar_java_major(&manifest).unwrap_or(
        if gv.starts_with("26.") || gv.starts_with("27.") {
            25
        } else {
            21
        },
    );
    let java = java_for(major).await?;

    let artifacts: Vec<Value> = manifest["launchTypeData"]["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let of_type = |t: &str| -> Vec<String> {
        artifacts
            .iter()
            .filter(|a| a["type"].as_str() == Some(t))
            .filter_map(|a| a["name"].as_str().map(String::from))
            .collect()
    };
    let class_path = of_type("CLASS_PATH");
    let external = of_type("EXTERNAL_FILE");
    let main_class = manifest["launchTypeData"]["mainClass"]
        .as_str()
        .unwrap_or("com.moonsworth.lunar.genesis.Genesis")
        .to_string();

    let mut cmd = java_command(&java.path, settings.wrapper)?;
    // Forge and Fabric mods resolve relative paths ("./config", waypoint
    // folders and so on) against the process' working directory, like a
    // normal launch where it's the game folder. Lunar's own files are all
    // passed as absolute paths below, so the instance folder is safe here.
    // Plain Lunar keeps its own folder, the way its launcher starts it.
    if runs_mods {
        cmd.current_dir(instance_dir);
    } else {
        cmd.current_dir(&dir);
    }
    for arg in manifest["jre"]["extraArguments"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(arg) = arg.as_str() {
            if arg.starts_with("-Djava.library.path=") {
                continue;
            }
            cmd.arg(arg);
        }
    }
    cmd.arg(format!("-Xmx{}M", settings.memory.maximum));
    cmd.arg(format!(
        "-Djava.library.path={}",
        dir.join("natives").display()
    ));
    cmd.args(settings.java_args);
    if runs_mods {
        // The folder Lunar's Fabric and Forge modules ("Ichor+Fabric",
        // "Ichor+Forge") load extra mods from; both read this same property.
        // Without it they look next to Lunar's own files.
        cmd.arg(format!(
            "-Dichor.fabric.profileModPath={}",
            instance_dir.join("mods").join(format!("{module}-{gv}")).display()
        ));
    }
    cmd.arg("-cp").arg(
        class_path
            .iter()
            .map(|n| dir.join(n).to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(super::classpath_separator()),
    );
    cmd.arg(main_class);

    let assets = state.directories.assets_dir();
    let textures = client_root(super::ClientKind::Lunar)
        .await?
        .join("textures");
    tokio::fs::create_dir_all(&textures).await?;
    cmd.args([
        "--version".to_string(),
        gv.clone(),
        "--accessToken".into(),
        credentials.access_token.clone(),
        "--assetIndex".into(),
        asset_index,
        "--userProperties".into(),
        "{}".into(),
        "--gameDir".into(),
        instance_dir.to_string_lossy().to_string(),
        "--assetsDir".into(),
        assets.to_string_lossy().to_string(),
        "--username".into(),
        credentials.offline_profile.name.clone(),
        "--uuid".into(),
        credentials.offline_profile.id.simple().to_string(),
        "--userType".into(),
        "msa".into(),
        "--launcherVersion".into(),
        LAUNCHER_VERSION.into(),
        "--hwid".into(),
        "PUBLIC-HWID".into(),
        "--installationId".into(),
        installation_id().await,
        "--width".into(),
        settings.resolution.0.to_string(),
        "--height".into(),
        settings.resolution.1.to_string(),
        "--workingDirectory".into(),
        dir.to_string_lossy().to_string(),
        "--classpathDir".into(),
        dir.to_string_lossy().to_string(),
        "--ichorClassPath".into(),
        class_path.join(","),
        "--ichorExternalFiles".into(),
        external.join(","),
        "--texturesDir".into(),
        textures.to_string_lossy().to_string(),
        "--webosrDir".into(),
        dir.join("natives").to_string_lossy().to_string(),
    ]);
    if let Err(e) = ensure_ui_assets(&manifest, &dir).await {
        tracing::warn!("Lunar menu assets couldn't be updated: {e:#}");
    }
    // Installs prepared before cosmetics were downloaded get them here; the
    // marker makes this a no-op once they're complete.
    if let Err(e) = ensure_textures(&manifest).await {
        tracing::warn!("Lunar cosmetics couldn't be updated: {e:#}");
    }
    if dir.join("ui").exists() {
        cmd.arg("--uiDir").arg(dir.join("ui"));
    }
    if let Some(token) = manifest["canaryToken"].as_str() {
        cmd.arg("--canaryToken").arg(token);
    }
    cmd.arg("--launchId").arg(uuid::Uuid::new_v4().to_string());
    Ok(cmd)
}

/// Lunar's Fabric and Forge modules read mods from
/// `<game dir>/mods/<fabric|forge>-<version>/`
/// (what its launcher's Mods page fills). Modrinth installs them into
/// `<game dir>/mods/`, so the enabled ones are copied there before each
/// launch; disabled (`.disabled`) and removed mods are taken out again.
async fn sync_mods(
    instance_dir: &Path,
    module: &str,
    gv: &str,
) -> crate::Result<()> {
    if !super::safe_file_name(gv) {
        return Ok(());
    }
    let mods = instance_dir.join("mods");
    let target = mods.join(format!("{module}-{gv}"));
    tokio::fs::create_dir_all(&target).await?;

    let mut wanted = std::collections::HashSet::new();
    let mut dir = tokio::fs::read_dir(&mods).await?;
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if !entry.file_type().await?.is_file()
            || path.extension().and_then(|e| e.to_str()) != Some("jar")
        {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_owned()) else {
            continue;
        };
        let dest = target.join(&name);
        let same = match (
            tokio::fs::metadata(&path).await,
            tokio::fs::metadata(&dest).await,
        ) {
            (Ok(a), Ok(b)) => a.len() == b.len(),
            _ => false,
        };
        if !same {
            tokio::fs::copy(&path, &dest).await?;
        }
        wanted.insert(name);
    }
    let mut dir = tokio::fs::read_dir(&target).await?;
    while let Some(entry) = dir.next_entry().await? {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jar")
            && !wanted.contains(&entry.file_name())
        {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    // Lunar's Fabric loader doesn't open jars inside jars (Fabric's "jar in
    // jar"), so, like Lunar's launcher, put them next to the mods in
    // `dependencies/`.
    let jars: Vec<PathBuf> =
        wanted.iter().map(|name| target.join(name)).collect();
    let deps = target.join("dependencies");
    tokio::task::spawn_blocking(move || -> crate::Result<()> {
        let _ = std::fs::remove_dir_all(&deps);
        std::fs::create_dir_all(&deps)?;
        for jar in jars {
            if let Ok(bytes) = std::fs::read(&jar) {
                extract_nested_jars(bytes, &deps, 0);
            }
        }
        Ok(())
    })
    .await??;
    Ok(())
}

fn extract_nested_jars(bytes: Vec<u8>, deps: &Path, depth: u8) {
    if depth > 3 {
        return;
    }
    let Ok(mut archive) = zip::ZipArchive::new(std::io::Cursor::new(bytes))
    else {
        return;
    };
    for i in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(i) else {
            continue;
        };
        let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else {
            continue;
        };
        if !name.starts_with("META-INF/jars/") || !name.ends_with(".jar") {
            continue;
        }
        let Some(file) = name.rsplit('/').next().map(str::to_owned) else {
            continue;
        };
        if !super::safe_file_name(&file) {
            continue;
        }
        let mut inner = Vec::new();
        if std::io::Read::read_to_end(&mut entry, &mut inner).is_err() {
            continue;
        }
        let out = deps.join(&file);
        if !out.exists() {
            let _ = std::fs::write(&out, &inner);
        }
        extract_nested_jars(inner, deps, depth + 1);
    }
}

/// Lunar's own settings folder (`~/.lunarclient`).
fn lunar_home() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".lunarclient"))
}

/// Puts the Modrinth account into Lunar's accounts file and makes it the
/// active one. Other accounts in the file are left alone.
async fn write_account(credentials: &Credentials) -> crate::Result<()> {
    let Some(home) = lunar_home() else {
        return Ok(());
    };
    let path = home.join("settings").join("game").join("accounts.json");
    let root: Value = match tokio::fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).context(
            "Lunar's accounts file is damaged; it has not been changed",
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
    };
    let root = merge_account(
        root,
        &credentials.offline_profile.id.simple().to_string(),
        &credentials.offline_profile.name,
        &credentials.access_token,
        &credentials
            .expires
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_vec_pretty(&root)?).await?;
    Ok(())
}

/// Adds or refreshes one player in the contents of Lunar's `accounts.json`
/// and makes them the active account.
fn merge_account(
    mut root: Value,
    profile_id: &str,
    name: &str,
    access_token: &str,
    expires_at: &str,
) -> crate::Result<Value> {
    if !root.is_object() {
        bail!(
            "Lunar's accounts file has an unsupported format; it has not been changed"
        );
    }
    // Reuse the entry for this player when there is one.
    let accounts = root
        .as_object_mut()
        .expect("checked above")
        .entry("accounts")
        .or_insert_with(|| json!({}));
    if !accounts.is_object() {
        bail!(
            "Lunar's account list has an unsupported format; it has not been changed"
        );
    }
    let map = accounts.as_object_mut().expect("checked above");
    let local_id = map
        .iter()
        .find(|(_, a)| a["minecraftProfile"]["id"].as_str() == Some(profile_id))
        .map(|(k, _)| k.clone())
        .unwrap_or_else(|| profile_id.to_string());

    let mut account = map.get(&local_id).cloned().unwrap_or_else(|| json!({}));
    account["accessToken"] = json!(access_token);
    account["accessTokenExpiresAt"] = json!(expires_at);
    account["localId"] = json!(local_id);
    account["minecraftProfile"] = json!({ "id": profile_id, "name": name });
    if account["remoteId"].is_null() {
        account["remoteId"] = json!("");
    }
    account["type"] = json!("Xbox");
    account["username"] = json!(name);
    if account["eligibleForMigration"].is_null() {
        account["eligibleForMigration"] = json!(false);
        account["hasMultipleProfiles"] = json!(false);
        account["legacy"] = json!(true);
        account["persistent"] = json!(true);
        account["userProperites"] = json!([]);
    }
    map.insert(local_id.clone(), account);
    root["activeAccountLocalId"] = json!(local_id);
    Ok(root)
}

async fn ensure_ui_assets(
    manifest: &Value,
    dir: &std::path::Path,
) -> crate::Result<()> {
    sync_index(&manifest["ui"]["assets"], &dir.join("ui").join("assets"))
        .await
}

/// Lunar's in-game resources: cosmetics, emotes, badges, sprays and the JIT
/// index. The game reads them from `--texturesDir`; without them it plays
/// fine but every cosmetic is invisible. Shared by every Lunar version.
async fn ensure_textures(manifest: &Value) -> crate::Result<()> {
    let dir = client_root(super::ClientKind::Lunar).await?.join("textures");
    sync_index(&manifest["textures"], &dir).await
}

/// Downloads every file listed in one of Lunar's asset indexes
/// (`{ indexUrl, indexSha1, baseUrl }`, lines of `path sha1`) into `dest`.
/// Files that already match are skipped, and a marker skips the whole pass
/// when the same index was already completed.
async fn sync_index(spec: &Value, dest: &Path) -> crate::Result<()> {
    use futures::StreamExt;

    let Some(index_url) = spec["indexUrl"].as_str() else {
        return Ok(());
    };
    let base = spec["baseUrl"]
        .as_str()
        .context("Lunar asset base URL is missing")?
        .trim_end_matches('/')
        .to_owned();
    let marker = dest.join(".index-sha1");
    let expected = spec["indexSha1"].as_str().unwrap_or_default();
    if !expected.is_empty()
        && tokio::fs::read_to_string(&marker)
            .await
            .is_ok_and(|done| done.trim().eq_ignore_ascii_case(expected))
    {
        return Ok(());
    }
    let index = get_bytes(index_url).await?;
    if !expected.is_empty() && !sha1_hex(&index).eq_ignore_ascii_case(expected)
    {
        bail!("Lunar asset index checksum mismatch");
    }
    let index = String::from_utf8_lossy(&index).into_owned();

    let mut jobs = Vec::new();
    for line in index.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.is_empty() {
            continue;
        }
        if fields.len() < 2 || !safe_artifact_path(fields[0]) {
            tracing::warn!("Skipping odd Lunar asset entry: {line}");
            continue;
        }
        let hash = fields[1];
        if hash.len() != 40 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            tracing::warn!("Skipping Lunar asset with odd checksum: {line}");
            continue;
        }
        jobs.push((dest.join(fields[0]), hash.to_owned()));
    }

    let failures = futures::stream::iter(jobs)
        .map(|(path, hash)| {
            let url = format!("{base}/{hash}");
            async move {
                super::download_checked(&url, &path, Some(&hash)).await
            }
        })
        .buffer_unordered(16)
        .filter(|r| std::future::ready(r.is_err()))
        .count()
        .await;

    if failures > 0 {
        // Leave the marker unwritten so the next launch retries the rest.
        bail!("{failures} Lunar assets couldn't be downloaded");
    }
    tokio::fs::create_dir_all(dest).await?;
    if !expected.is_empty() {
        tokio::fs::write(&marker, expected).await?;
    }
    Ok(())
}
