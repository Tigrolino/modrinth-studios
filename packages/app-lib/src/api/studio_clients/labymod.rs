//! LabyMod 4, launched without the Laby launcher.
//!
//! LabyMod publishes a release manifest per channel
//! (`/api/v1/manifest/<channel>/latest.json`) listing the Minecraft versions
//! it supports, the LabyMod jar, its asset jars, and for each Minecraft
//! version a launcher version file (`customManifestUrl`) in Mojang's format.
//! We download all of that and start it like any Mojang-style version file,
//! reusing the vanilla files Modrinth already downloaded for the instance.
//!
//! This is unofficial: LabyMod only supports its own launcher, and other
//! launchers have reported crashes with this kind of setup on some versions.

use super::{
    ClientInstall, ClientVersion, LaunchSettings, Progress, client_root,
    download_checked, extract_zip, get_json, java_command, java_for,
    mojang_version_json,
};
use crate::State;
use crate::state::Credentials;
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::process::Command;

const RELEASES: &str = "https://releases.r2.labymod.net/api/v1";

pub(super) async fn list_versions() -> crate::Result<Vec<ClientVersion>> {
    let mut out = Vec::new();
    for channel in ["production", "snapshot"] {
        let manifest = match get_json(&format!(
            "{RELEASES}/manifest/{channel}/latest.json"
        ))
        .await
        {
            Ok(m) => m,
            Err(e) if channel == "snapshot" => {
                tracing::debug!("LabyMod snapshot channel unavailable: {e:#}");
                continue;
            }
            Err(e) => {
                return Err(e.context(
                    "LabyMod's release list isn't reachable right now",
                ));
            }
        };
        let laby_version = manifest["labyModVersion"]
            .as_str()
            .unwrap_or("4")
            .to_string();
        for (i, v) in manifest["minecraftVersions"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(gv) = v["version"].as_str() else {
                continue;
            };
            if v["type"].as_str() == Some("snapshot") && channel == "production"
            {
                continue;
            }
            out.push(ClientVersion {
                id: format!("{channel}|{gv}"),
                name: gv.to_string(),
                game_version: gv.to_string(),
                loader: "vanilla".into(),
                detail: Some(if channel == "production" {
                    format!("LabyMod 4 · build {laby_version}")
                } else {
                    format!("LabyMod 4 snapshot · build {laby_version}")
                }),
                recommended: channel == "production" && i == 0,
            });
        }
    }
    if out.is_empty() {
        bail!("LabyMod didn't list any versions");
    }
    Ok(out)
}

fn split_id(id: &str) -> (String, String) {
    match id.split_once('|') {
        Some((c, gv)) => (c.to_string(), gv.to_string()),
        None => ("production".to_string(), id.to_string()),
    }
}

pub(super) async fn prepare(
    install: &ClientInstall,
    instance_dir: &Path,
    progress: &Progress<'_>,
) -> crate::Result<()> {
    let state = State::get().await?;
    let (channel, gv) = split_id(&install.version_id);
    let root = client_root(super::ClientKind::LabyMod).await?;

    progress.step(5.0, "Checking for LabyMod updates");
    let manifest =
        get_json(&format!("{RELEASES}/manifest/{channel}/latest.json")).await?;
    let commit = manifest["commitReference"]
        .as_str()
        .context("LabyMod's manifest has no build reference")?
        .to_string();
    let entry = manifest["minecraftVersions"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|v| v["version"].as_str() == Some(gv.as_str()))
        })
        .cloned()
        .with_context(|| {
            format!("LabyMod no longer supports Minecraft {gv}")
        })?;

    let build_dir = root.join(format!("{channel}-{commit}"));
    tokio::fs::create_dir_all(&build_dir).await?;

    // The LabyMod jar itself.
    progress.step(15.0, "Downloading LabyMod");
    download_checked(
        &format!("{RELEASES}/download/labymod4/{channel}/{commit}.jar"),
        &build_dir.join("LabyMod-4.jar"),
        manifest["sha1"].as_str(),
    )
    .await?;

    // Asset jars live in the game folder, where LabyMod looks for them.
    let assets_dir = instance_dir.join("labymod-neo").join("assets");
    if let Some(assets) = manifest["assets"].as_object() {
        let step = 20.0 / assets.len().max(1) as f64;
        for (name, sha) in assets {
            let Some(sha) = sha.as_str() else { continue };
            progress.step(step, &format!("Downloading LabyMod {name}"));
            download_checked(
				&format!(
					"{RELEASES}/download/assets/labymod4/{channel}/{commit}/{name}/{sha}.jar"
				),
				&assets_dir.join(format!("{name}.jar")),
				Some(sha),
			)
			.await?;
        }
    }

    // The version file for this Minecraft version.
    progress.step(5.0, "Downloading the LabyMod version file");
    let version_url = entry["customManifestUrl"]
        .as_str()
        .context("LabyMod has no version file for this Minecraft version")?;
    let version_json = get_json(version_url)
        .await
        .context("LabyMod's version file couldn't be downloaded (it may only be available to the Laby launcher)")?;
    let mut merged = merge_with_parent(version_json, &gv, &root).await?;
    add_client_libraries(&mut merged, &channel, &gv).await?;

    // Libraries (shared with Modrinth's library folder) and natives.
    let libs = libraries(&merged);
    let step = 45.0 / libs.len().max(1) as f64;
    let natives_dir = build_dir.join(format!("natives-{gv}"));
    for lib in &libs {
        progress.step(step, &format!("Downloading {}", lib.name));
        let path = state.directories.libraries_dir().join(&lib.path);
        if let Some(url) = &lib.url {
            download_checked(url, &path, lib.sha1.as_deref()).await?;
        }
        if lib.native && path.exists() && !natives_dir.join(".done").exists() {
            let bytes = tokio::fs::read(&path).await?;
            extract_zip(bytes, natives_dir.clone()).await?;
        }
    }
    if natives_dir.exists() {
        let _ = tokio::fs::write(natives_dir.join(".done"), b"").await;
    }

    let snapshot = json!({
        "commit": commit,
        "channel": channel,
        "javaMajor": entry["runtime"]["version"].as_u64().unwrap_or(21),
        "version": merged,
    });
    tokio::fs::write(
        root.join(format!("{channel}-{gv}.json")),
        serde_json::to_vec_pretty(&snapshot)?,
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
    let state = State::get().await?;
    let (channel, gv) = split_id(&install.version_id);
    let root = client_root(super::ClientKind::LabyMod).await?;
    let snapshot: Value = serde_json::from_slice(
        &tokio::fs::read(root.join(format!("{channel}-{gv}.json")))
            .await
            .context("LabyMod isn't downloaded yet")?,
    )?;
    let commit = snapshot["commit"].as_str().unwrap_or_default();
    let build_dir = root.join(format!("{channel}-{commit}"));
    let mut repaired_version = snapshot["version"].clone();
    add_client_libraries(&mut repaired_version, &channel, &gv).await?;
    let version = &repaired_version;
    let java =
        java_for(snapshot["javaMajor"].as_u64().unwrap_or(21) as u32).await?;

    let libraries_dir = state.directories.libraries_dir();
    let mut class_path: Vec<String> = Vec::new();
    let libs = libraries(version);
    let mut has_laby_lib = false;
    for lib in &libs {
        if let Some(url) = &lib.url {
            download_checked(
                url,
                &libraries_dir.join(&lib.path),
                lib.sha1.as_deref(),
            )
            .await?;
        }
        if lib.native {
            continue;
        }
        if lib
            .name
            .to_ascii_lowercase()
            .starts_with("net.labymod:labymod:")
        {
            has_laby_lib = true;
        }
        class_path
            .push(libraries_dir.join(&lib.path).to_string_lossy().to_string());
    }
    if !has_laby_lib {
        class_path.push(
            build_dir
                .join("LabyMod-4.jar")
                .to_string_lossy()
                .to_string(),
        );
    }
    // The vanilla game jar Modrinth downloaded for this instance.
    let client_jar =
        state.directories.version_dir(&gv).join(format!("{gv}.jar"));
    if !client_jar.exists() {
        bail!(
            "Minecraft {gv} isn't installed yet; let the instance finish installing first"
        );
    }
    class_path.push(client_jar.to_string_lossy().to_string());
    let class_path = class_path.join(super::classpath_separator());

    let natives_dir = build_dir.join(format!("natives-{gv}"));
    tokio::fs::create_dir_all(&natives_dir).await?;
    let assets_dir = state.directories.assets_dir();
    let asset_index = version["assetIndex"]["id"]
        .as_str()
        .unwrap_or(&gv)
        .to_string();

    let vars = |s: &str| -> String {
        s.replace("${auth_player_name}", &credentials.offline_profile.name)
            .replace("${version_name}", &gv)
            .replace("${game_directory}", &instance_dir.to_string_lossy())
            .replace("${assets_root}", &assets_dir.to_string_lossy())
            .replace("${game_assets}", &assets_dir.to_string_lossy())
            .replace("${assets_index_name}", &asset_index)
            .replace(
                "${auth_uuid}",
                &credentials.offline_profile.id.simple().to_string(),
            )
            .replace("${auth_access_token}", &credentials.access_token)
            .replace("${auth_session}", &credentials.access_token)
            .replace("${user_type}", "msa")
            .replace(
                "${version_type}",
                version["type"].as_str().unwrap_or("release"),
            )
            .replace("${user_properties}", "{}")
            .replace("${clientid}", "")
            .replace("${auth_xuid}", "")
            .replace("${natives_directory}", &natives_dir.to_string_lossy())
            .replace("${library_directory}", &libraries_dir.to_string_lossy())
            .replace("${classpath_separator}", super::classpath_separator())
            .replace("${launcher_name}", "modrinth-studios")
            .replace("${launcher_version}", env!("CARGO_PKG_VERSION"))
            .replace("${classpath}", &class_path)
            .replace("${resolution_width}", &settings.resolution.0.to_string())
            .replace("${resolution_height}", &settings.resolution.1.to_string())
    };

    let mut cmd = java_command(&java.path, settings.wrapper)?;
    cmd.current_dir(instance_dir);
    cmd.arg(format!("-Xmx{}M", settings.memory.maximum));
    cmd.args(settings.java_args);

    let jvm = argument_list(&version["arguments"]["jvm"]);
    if jvm.is_empty() {
        cmd.arg(format!("-Djava.library.path={}", natives_dir.display()));
        cmd.arg("-cp").arg(&class_path);
    } else {
        cmd.args(jvm.iter().map(|a| vars(a)));
    }
    cmd.arg(
        version["mainClass"]
            .as_str()
            .context("LabyMod's version file has no main class")?,
    );

    let game = argument_list(&version["arguments"]["game"]);
    if game.is_empty() {
        let legacy = version["minecraftArguments"].as_str().unwrap_or_default();
        cmd.args(legacy.split_whitespace().map(|a| vars(a)));
    } else {
        cmd.args(game.iter().map(|a| vars(a)));
    }
    cmd.arg("--width").arg(settings.resolution.0.to_string());
    cmd.arg("--height").arg(settings.resolution.1.to_string());
    Ok(cmd)
}

/// Fills in a version file that says `inheritsFrom` with its parent (Mojang's
/// version), the way Mojang's launcher does.
async fn merge_with_parent(
    child: Value,
    gv: &str,
    root: &Path,
) -> crate::Result<Value> {
    let Some(parent_id) = child["inheritsFrom"].as_str().map(String::from)
    else {
        return Ok(child);
    };
    let parent = mojang_version_json(
        if parent_id.is_empty() {
            gv
        } else {
            parent_id.as_str()
        },
        root,
    )
    .await?;
    let mut merged = parent.clone();
    for (key, value) in child.as_object().into_iter().flatten() {
        match key.as_str() {
            "libraries" => {
                let mut libs: Vec<Value> =
                    value.as_array().cloned().unwrap_or_default();
                let seen: HashSet<String> =
                    libs.iter().filter_map(library_key).collect();
                for lib in parent["libraries"].as_array().into_iter().flatten()
                {
                    if library_key(lib).is_none_or(|k| !seen.contains(&k)) {
                        libs.push(lib.clone());
                    }
                }
                merged["libraries"] = Value::Array(libs);
            }
            "arguments" => {
                for side in ["jvm", "game"] {
                    let mut list: Vec<Value> = parent["arguments"][side]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    list.extend(
                        value[side].as_array().cloned().unwrap_or_default(),
                    );
                    merged["arguments"][side] = Value::Array(list);
                }
            }
            "inheritsFrom" => {}
            _ => merged[key.as_str()] = value.clone(),
        }
    }
    Ok(merged)
}

/// "group:artifact" plus classifier, used to drop duplicate libraries.
fn library_key(lib: &Value) -> Option<String> {
    let name = lib["name"].as_str()?;
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return Some(name.to_string());
    }
    let mut key = format!("{}:{}", parts[0], parts[1]);
    if let Some(classifier) = parts.get(3) {
        key.push(':');
        key.push_str(classifier);
    }
    Some(key)
}

struct Lib {
    name: String,
    path: PathBuf,
    url: Option<String>,
    sha1: Option<String>,
    native: bool,
}

fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        _ => "linux",
    }
}

fn rules_allow(rules: &Value) -> bool {
    let Some(rules) = rules.as_array() else {
        return true;
    };
    let mut allowed = false;
    for rule in rules {
        let mut matches = true;
        if let Some(name) = rule["os"]["name"].as_str() {
            matches &= name == os_name();
        }
        if let Some(arch) = rule["os"]["arch"].as_str() {
            matches &= (arch == "x86") == (std::env::consts::ARCH == "x86");
        }
        if rule["features"].is_object() {
            // Demo mode, custom resolution, quick play... never on here.
            matches = false;
        }
        if matches {
            allowed = rule["action"].as_str() == Some("allow");
        }
    }
    allowed
}

fn maven_path(name: &str) -> Option<PathBuf> {
    let (coords, ext) = match name.split_once('@') {
        Some((c, e)) => (c, e),
        None => (name, "jar"),
    };
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    let file = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    let mut path = PathBuf::new();
    for seg in group.split('.') {
        path.push(seg);
    }
    path.push(artifact);
    path.push(version);
    path.push(file);
    Some(path)
}

/// A library path from a version file must stay inside the libraries folder.
fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn libraries(version: &Value) -> Vec<Lib> {
    let mut out = unchecked_libraries(version);
    out.retain(|lib| {
        let ok = safe_relative(&lib.path);
        if !ok {
            tracing::warn!("Skipping LabyMod library with an unsafe path: {}", lib.name);
        }
        ok
    });
    out
}

fn unchecked_libraries(version: &Value) -> Vec<Lib> {
    let mut out = Vec::new();
    for lib in version["libraries"].as_array().into_iter().flatten() {
        if !rules_allow(&lib["rules"]) && lib["rules"].is_array() {
            continue;
        }
        let Some(name) = lib["name"].as_str() else {
            continue;
        };

        // Old-style natives: a classifier per OS.
        if let Some(key) = lib["natives"][os_name()].as_str() {
            let key = key.replace(
                "${arch}",
                if cfg!(target_pointer_width = "64") {
                    "64"
                } else {
                    "32"
                },
            );
            if let Some(native) =
                lib["downloads"]["classifiers"][key.as_str()].as_object()
                && let Some(path) = native.get("path").and_then(|p| p.as_str())
            {
                out.push(Lib {
                    name: format!("{name}:{key}"),
                    path: PathBuf::from(path),
                    url: native
                        .get("url")
                        .and_then(|u| u.as_str())
                        .map(String::from),
                    sha1: native
                        .get("sha1")
                        .and_then(|u| u.as_str())
                        .map(String::from),
                    native: true,
                });
            }
        }

        if let Some(artifact) = lib["downloads"]["artifact"].as_object() {
            let path = artifact
                .get("path")
                .and_then(|p| p.as_str())
                .map(PathBuf::from)
                .or_else(|| maven_path(name));
            if let Some(path) = path {
                out.push(Lib {
                    name: name.to_string(),
                    path,
                    url: artifact
                        .get("url")
                        .and_then(|u| u.as_str())
                        .filter(|u| !u.is_empty())
                        .map(String::from),
                    sha1: artifact
                        .get("sha1")
                        .and_then(|u| u.as_str())
                        .map(String::from),
                    native: false,
                });
            }
        } else if lib["natives"].is_null() || lib["downloads"].is_null() {
            // Maven-style entry: a repository base URL plus the coordinates.
            if let Some(path) = maven_path(name) {
                let base = lib["url"]
                    .as_str()
                    .unwrap_or("https://libraries.minecraft.net/")
                    .trim_end_matches('/')
                    .to_string();
                let rel = path.to_string_lossy().replace('\\', "/");
                out.push(Lib {
                    name: name.to_string(),
                    url: Some(format!("{base}/{rel}")),
                    path,
                    sha1: lib["sha1"].as_str().map(String::from),
                    native: false,
                });
            }
        }
    }
    out
}

/// Flattens Mojang's `arguments.jvm`/`arguments.game` lists, keeping only
/// entries whose rules apply here.
fn argument_list(list: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for arg in list.as_array().into_iter().flatten() {
        if let Some(s) = arg.as_str() {
            out.push(s.to_string());
        } else if rules_allow(&arg["rules"]) {
            match &arg["value"] {
                Value::String(s) => out.push(s.clone()),
                Value::Array(values) => out.extend(
                    values.iter().filter_map(|v| v.as_str().map(String::from)),
                ),
                _ => {}
            }
        }
    }
    out
}

async fn add_client_libraries(
    version: &mut Value,
    channel: &str,
    gv: &str,
) -> crate::Result<()> {
    let manifest =
        get_json(&format!("{RELEASES}/libraries/{channel}.json")).await?;
    let mut extra = Vec::new();
    for lib in manifest["libraries"].as_array().into_iter().flatten() {
        if !matches!(lib["minecraftVersion"].as_str(), Some("all"))
            && lib["minecraftVersion"].as_str() != Some(gv)
        {
            continue;
        }
        let name = lib["name"]
            .as_str()
            .context("Missing LabyMod library name")?;
        let path =
            maven_path(name).context("Invalid LabyMod library coordinates")?;
        extra.push(json!({ "name": name, "downloads": { "artifact": { "path": path.to_string_lossy().replace('\\', "/"), "url": lib["url"], "sha1": lib["sha1"] } } }));
    }
    let keys: HashSet<String> = extra.iter().filter_map(library_key).collect();
    let libraries = version["libraries"]
        .as_array_mut()
        .context("Missing LabyMod libraries")?;
    libraries
        .retain(|lib| library_key(lib).is_none_or(|key| !keys.contains(&key)));
    libraries.extend(extra);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os_rule(action: &str, os: &str) -> Value {
        json!({ "action": action, "os": { "name": os } })
    }

    #[test]
    fn version_ids_split_into_channel_and_game_version() {
        assert_eq!(
            split_id("snapshot|1.21.11"),
            ("snapshot".to_string(), "1.21.11".to_string())
        );
        // Ids saved before channels existed are production builds.
        assert_eq!(
            split_id("1.8.9"),
            ("production".to_string(), "1.8.9".to_string())
        );
    }

    #[test]
    fn library_keys_ignore_the_version_but_keep_the_classifier() {
        let key = |name: &str| library_key(&json!({ "name": name }));
        assert_eq!(key("org.lwjgl:lwjgl:3.3.3"), key("org.lwjgl:lwjgl:3.3.1"));
        assert_ne!(
            key("org.lwjgl:lwjgl:3.3.3"),
            key("org.lwjgl:lwjgl:3.3.3:natives-windows")
        );
        assert_eq!(key("odd"), Some("odd".to_string()));
        assert_eq!(library_key(&json!({})), None);
    }

    #[test]
    fn rules_follow_mojangs_allow_and_disallow_logic() {
        let other = if os_name() == "windows" { "osx" } else { "windows" };
        assert!(rules_allow(&Value::Null));
        assert!(rules_allow(&json!([{ "action": "allow" }])));
        assert!(!rules_allow(&json!([])));
        assert!(rules_allow(&json!([os_rule("allow", os_name())])));
        assert!(!rules_allow(&json!([os_rule("allow", other)])));
        assert!(!rules_allow(&json!([
            { "action": "allow" },
            os_rule("disallow", os_name())
        ])));
        assert!(rules_allow(&json!([
            { "action": "allow" },
            os_rule("disallow", other)
        ])));
        // Feature-gated arguments (demo mode, custom resolution) never apply.
        assert!(!rules_allow(&json!([
            { "action": "allow", "features": { "is_demo_user": true } }
        ])));
    }

    #[test]
    fn maven_coordinates_become_repository_paths() {
        let p = |s: &str| maven_path(s).map(|p| p.to_string_lossy().replace('\\', "/"));
        assert_eq!(
            p("net.labymod:labymod:4.2.0").as_deref(),
            Some("net/labymod/labymod/4.2.0/labymod-4.2.0.jar")
        );
        assert_eq!(
            p("org.lwjgl:lwjgl:3.3.3:natives-windows").as_deref(),
            Some("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
        );
        assert_eq!(
            p("com.example:thing:1.0@zip").as_deref(),
            Some("com/example/thing/1.0/thing-1.0.zip")
        );
        assert_eq!(p("not-maven"), None);
        assert_eq!(p("a:b"), None);
    }

    #[test]
    fn libraries_are_read_from_every_version_file_style() {
        let version = json!({ "libraries": [
            { "name": "a.b:art:1", "downloads": { "artifact": {
                "path": "a/b/art/1/art-1.jar", "url": "https://x/art.jar", "sha1": "abc" } } },
            { "name": "c.d:maven:2", "url": "https://maven.example/" },
            { "name": "e.f:plain:3" },
            { "name": "g.h:nope:1", "rules": [{ "action": "allow", "os": { "name": "nonexistent" } }],
              "downloads": { "artifact": { "path": "g/h/nope/1/nope-1.jar" } } },
            { "name": "i.j:noname-url:1", "downloads": { "artifact": { "path": "i/j/n.jar", "url": "" } } },
        ]});
        let libs = libraries(&version);
        let names: Vec<&str> = libs.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["a.b:art:1", "c.d:maven:2", "e.f:plain:3", "i.j:noname-url:1"]);
        assert_eq!(libs[0].url.as_deref(), Some("https://x/art.jar"));
        assert_eq!(libs[0].sha1.as_deref(), Some("abc"));
        assert_eq!(
            libs[1].url.as_deref(),
            Some("https://maven.example/c/d/maven/2/maven-2.jar")
        );
        assert_eq!(
            libs[2].url.as_deref(),
            Some("https://libraries.minecraft.net/e/f/plain/3/plain-3.jar")
        );
        // An empty URL means "already there", not "download from ''".
        assert_eq!(libs[3].url, None);
        assert!(libs.iter().all(|l| !l.native));
    }

    #[test]
    fn old_style_natives_pick_this_platforms_classifier() {
        let key = format!("natives-{}", os_name());
        let mut natives = serde_json::Map::new();
        natives.insert(os_name().to_string(), json!(key));
        let mut classifiers = serde_json::Map::new();
        classifiers.insert(
            key.clone(),
            json!({ "path": "org/lwjgl/natives.jar", "url": "https://x/n.jar", "sha1": "def" }),
        );
        let version = json!({ "libraries": [{
            "name": "org.lwjgl.lwjgl:lwjgl-platform:2.9.4",
            "natives": natives,
            "downloads": { "classifiers": classifiers }
        }]});
        let libs = libraries(&version);
        assert_eq!(libs.len(), 1);
        assert!(libs[0].native);
        assert_eq!(libs[0].name, format!("org.lwjgl.lwjgl:lwjgl-platform:2.9.4:{key}"));
    }

    #[test]
    fn library_paths_cannot_escape_the_libraries_folder() {
        let version = json!({ "libraries": [
            { "name": "bad:one:1", "downloads": { "artifact": { "path": "../../evil.jar", "url": "https://x" } } },
            { "name": "bad:two:1", "downloads": { "artifact": { "path": "/etc/evil.jar", "url": "https://x" } } },
            { "name": "good:lib:1", "downloads": { "artifact": { "path": "good/lib.jar", "url": "https://x" } } },
        ]});
        let names: Vec<String> = libraries(&version).into_iter().map(|l| l.name).collect();
        assert_eq!(names, ["good:lib:1"]);
    }

    #[test]
    fn arguments_keep_plain_strings_and_allowed_rule_values() {
        let args = argument_list(&json!([
            "--username",
            "${auth_player_name}",
            { "rules": [{ "action": "allow" }], "value": ["-a", "-b"] },
            { "rules": [{ "action": "allow" }], "value": "-c" },
            { "rules": [{ "action": "allow", "features": { "has_custom_resolution": true } }],
              "value": ["--width", "${resolution_width}"] },
            { "rules": [os_rule("allow", "nonexistent")], "value": "-XstartOnFirstThread" },
        ]));
        assert_eq!(args, ["--username", "${auth_player_name}", "-a", "-b", "-c"]);
        assert!(argument_list(&Value::Null).is_empty());
    }

    #[tokio::test]
    async fn version_files_inherit_from_mojangs_version() {
        let root = tempfile::tempdir().unwrap();
        // A cached Mojang version file, so no download happens.
        tokio::fs::create_dir_all(root.path().join("mojang")).await.unwrap();
        tokio::fs::write(
            root.path().join("mojang").join("1.21.11.json"),
            serde_json::to_vec(&json!({
                "id": "1.21.11",
                "mainClass": "net.minecraft.client.main.Main",
                "assetIndex": { "id": "28" },
                "libraries": [
                    { "name": "org.lwjgl:lwjgl:3.3.1" },
                    { "name": "com.mojang:brigadier:1.0" }
                ],
                "arguments": { "jvm": ["-Dvanilla"], "game": ["--vanilla"] }
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        let child = json!({
            "inheritsFrom": "1.21.11",
            "mainClass": "net.labymod.Main",
            "libraries": [{ "name": "org.lwjgl:lwjgl:3.3.3" }, { "name": "net.labymod:labymod:4" }],
            "arguments": { "jvm": ["-Dlaby"], "game": ["--laby"] }
        });
        let merged = merge_with_parent(child, "1.21.11", root.path()).await.unwrap();
        assert_eq!(merged["mainClass"], "net.labymod.Main");
        assert_eq!(merged["assetIndex"]["id"], "28");
        assert!(merged.get("inheritsFrom").is_none());
        let names: Vec<&str> = merged["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        // LabyMod's newer LWJGL replaces Mojang's; other libraries are kept.
        assert_eq!(names, ["org.lwjgl:lwjgl:3.3.3", "net.labymod:labymod:4", "com.mojang:brigadier:1.0"]);
        assert_eq!(merged["arguments"]["jvm"], json!(["-Dvanilla", "-Dlaby"]));
        assert_eq!(merged["arguments"]["game"], json!(["--vanilla", "--laby"]));

        // Files without a parent are returned unchanged.
        let lone = json!({ "mainClass": "x" });
        assert_eq!(merge_with_parent(lone.clone(), "1.21.11", root.path()).await.unwrap(), lone);
    }

    // ---- live checks against LabyMod's servers: `cargo test -- --ignored` ----

    #[tokio::test]
    #[ignore = "live: talks to LabyMod's servers"]
    async fn live_labymod_lists_versions() {
        let versions = list_versions().await.expect("LabyMod's release list");
        assert!(!versions.is_empty());
        assert!(versions.iter().any(|v| v.recommended));
        for v in &versions {
            assert!(super::super::safe_file_name(&v.game_version), "{}", v.game_version);
            let (channel, gv) = split_id(&v.id);
            assert!(channel == "production" || channel == "snapshot", "{}", v.id);
            assert_eq!(gv, v.game_version);
        }
    }

    #[tokio::test]
    #[ignore = "live: talks to LabyMod's servers"]
    async fn live_labymod_manifest_has_everything_prepare_needs() {
        let manifest = get_json(&format!("{RELEASES}/manifest/production/latest.json"))
            .await
            .unwrap();
        assert!(manifest["commitReference"].is_string(), "no build reference");
        assert!(manifest["sha1"].is_string(), "no jar checksum");
        let entry = &manifest["minecraftVersions"][0];
        assert!(entry["version"].is_string());
        let url = entry["customManifestUrl"]
            .as_str()
            .expect("no version file for the newest Minecraft version");
        let version = get_json(url).await.expect("LabyMod version file");
        assert!(version["mainClass"].is_string() || version["inheritsFrom"].is_string());
        let libs = libraries(&version);
        assert_eq!(libs.len(), unchecked_libraries(&version).len(), "unsafe library paths");

        let extra = get_json(&format!("{RELEASES}/libraries/production.json"))
            .await
            .expect("LabyMod library list");
        for lib in extra["libraries"].as_array().into_iter().flatten() {
            let name = lib["name"].as_str().expect("library without a name");
            assert!(maven_path(name).is_some(), "bad coordinates: {name}");
        }
    }
}
