//! "Copy from your <client> install": finds the game folders a client's own
//! launcher uses on this computer and copies servers, settings, packs,
//! screenshots and worlds from one of them into a new client installation.

use super::ClientKind;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One game folder of the client's own launcher.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportSource {
    pub path: String,
    pub label: String,
    /// Whether it's the folder for the version being installed.
    pub matches_version: bool,
    /// Lunar: "fabric" when the profile has Fabric mods, else "lunar"; the
    /// installation should use that module or the copied data won't fit.
    pub module: Option<String>,
    pub items: Vec<ImportItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImportItem {
    /// "servers", "options", "resourcepacks", "shaderpacks", "screenshots", "saves"
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
}

/// Folders that are never a game folder and are big to walk.
const SKIP: &[&str] = &[
    "assets", "libraries", "versions", "natives", "mods", "logs", "cache",
    "jre", "runtime", "textures", "jit", "ui", "saves", "resourcepacks",
    "shaderpacks", "screenshots", "config", "crash-reports", "node_modules",
];

fn roots(kind: ClientKind) -> Vec<(PathBuf, &'static str)> {
    let home = dirs::home_dir().unwrap_or_default();
    let data = dirs::data_dir().unwrap_or_default();
    let local = dirs::data_local_dir().unwrap_or_default();
    match kind {
        ClientKind::Lunar => vec![(home.join(".lunarclient").join("profiles"), "Lunar profile")],
        ClientKind::NoRisk => vec![
            (data.join("norisk"), "NoRisk"),
            (data.join("NoRiskClient"), "NoRisk"),
            (data.join("gg.norisk.launcher"), "NoRisk"),
            (home.join(".norisk"), "NoRisk"),
        ],
        // LabyMod's launcher plays in the normal Minecraft folder by default.
        ClientKind::LabyMod => vec![
            (data.join(".minecraft"), "Minecraft folder"),
            (data.join("LabyMod"), "LabyMod"),
            (data.join("labymodlauncher"), "LabyMod"),
        ],
        ClientKind::Horizon => vec![
            (data.join("horizon"), "Horizon"),
            (data.join("Horizon"), "Horizon"),
            (local.join("horizon"), "Horizon"),
            (data.join("dev.playhorizon.launcher"), "Horizon"),
            (home.join(".horizon"), "Horizon"),
        ],
    }
}

fn is_game_dir(dir: &Path) -> bool {
    dir.join("options.txt").is_file() || dir.join("servers.dat").is_file()
}

fn find_game_dirs(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if out.len() >= 16 || !root.is_dir() {
        return;
    }
    if is_game_dir(root) {
        out.push(root.to_path_buf());
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP.contains(&name.as_str()) || name.starts_with('.') && name != ".minecraft" {
            continue;
        }
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            find_game_dirs(&entry.path(), depth - 1, out);
        }
    }
}

/// Lunar keeps settings and servers per profile, but resource packs, shader
/// packs, screenshots and worlds in its launcher's "game directory" (the
/// normal `.minecraft` unless changed in Lunar's settings).
fn lunar_game_dir() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let configured = std::fs::read(home.join(".lunarclient/settings/launcher.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v["settings"]["gameDirectory"].as_str().map(PathBuf::from))
        .filter(|p| p.is_dir());
    configured.or_else(|| dirs::data_dir().map(|d| d.join(".minecraft")).filter(|p| p.is_dir()))
}

/// Where one kind of content lives for a game folder: the folder itself and,
/// for Lunar, its shared game directory.
fn content_dirs(kind: ClientKind, dir: &Path, folder: &str) -> Vec<PathBuf> {
    let mut out = vec![dir.join(folder)];
    if kind == ClientKind::Lunar
        && let Some(shared) = lunar_game_dir()
    {
        out.push(shared.join(folder));
    }
    out
}

fn count_entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.flatten().count()).unwrap_or(0)
}

/// Lunar keeps Fabric/Forge mods in `mods/<fabric|forge>-<version>/`. The
/// folder for this version if it has mods, else the one with the most.
/// Returns the folder and its module ("fabric" or "forge").
fn lunar_mods_dir(dir: &Path, game_version: &str) -> Option<(PathBuf, &'static str)> {
    let jars = |d: &Path| {
        std::fs::read_dir(d)
            .map(|r| {
                r.flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "jar"))
                    .count()
            })
            .unwrap_or(0)
    };
    for module in ["fabric", "forge"] {
        let exact = dir.join("mods").join(format!("{module}-{game_version}"));
        if jars(&exact) > 0 {
            return Some((exact, module));
        }
    }
    std::fs::read_dir(dir.join("mods"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().to_string();
            let module = if name.starts_with("fabric-") {
                "fabric"
            } else if name.starts_with("forge-") {
                "forge"
            } else {
                return None;
            };
            Some((jars(&p), p, module))
        })
        .filter(|(n, _, _)| *n > 0)
        .max_by_key(|(n, _, _)| *n)
        .map(|(_, p, m)| (p, m))
}

fn items_of(kind: ClientKind, dir: &Path) -> Vec<ImportItem> {
    let mut items = Vec::new();
    if dir.join("servers.dat").is_file() {
        items.push(ImportItem {
            id: "servers".into(),
            label: "Servers".into(),
            detail: None,
        });
    }
    if dir.join("options.txt").is_file() {
        items.push(ImportItem {
            id: "options".into(),
            label: "Settings and keybinds".into(),
            detail: None,
        });
    }
    for (id, label, folders) in [
        ("resourcepacks", "Resource packs", &["resourcepacks"][..]),
        ("shaderpacks", "Shader packs", &["shaderpacks", "shaders"][..]),
        ("screenshots", "Screenshots", &["screenshots"][..]),
        ("saves", "Worlds", &["saves"][..]),
    ] {
        let n: usize = folders
            .iter()
            .flat_map(|f| content_dirs(kind, dir, f))
            .map(|d| count_entries(&d))
            .sum();
        if n > 0 {
            items.push(ImportItem {
                id: id.into(),
                label: label.into(),
                detail: Some(n.to_string()),
            });
        }
    }
    items
}

/// "26.3" -> "26", "1.21.10" -> "1.21", "1.8.9" -> "1.8" (Lunar's profile names).
fn major(gv: &str) -> String {
    let parts: Vec<&str> = gv.split('.').collect();
    if parts.first() == Some(&"1") && parts.len() >= 2 {
        format!("1.{}", parts[1])
    } else {
        parts.first().copied().unwrap_or(gv).to_string()
    }
}

/// Game folders of the client's own launcher found on this computer.
pub async fn import_sources(
    kind: ClientKind,
    game_version: String,
) -> crate::Result<Vec<ImportSource>> {
    tokio::task::spawn_blocking(move || {
        let wanted = major(&game_version);
        let mut out = Vec::new();
        for (root, label) in roots(kind) {
            let mut dirs = Vec::new();
            find_game_dirs(&root, 3, &mut dirs);
            for dir in dirs {
                let items = items_of(kind, &dir);
                if items.is_empty() {
                    continue;
                }
                let name = dir
                    .strip_prefix(&root)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .filter(|p| !p.is_empty())
                    .unwrap_or_else(|| {
                        dir.file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default()
                    });
                let matches_version = kind == ClientKind::Lunar && name == wanted
                    || name.contains(&game_version);
                let mut items = items;
                let mut module = None;
                if kind == ClientKind::Lunar {
                    let found = lunar_mods_dir(&dir, &game_version);
                    if let Some((mods_dir, kind_name)) = &found {
                        let n = std::fs::read_dir(mods_dir)
                            .map(|r| {
                                r.flatten()
                                    .filter(|e| e.path().extension().is_some_and(|x| x == "jar"))
                                    .count()
                            })
                            .unwrap_or(0);
                        items.push(ImportItem {
                            id: "mods".into(),
                            label: if *kind_name == "forge" { "Forge mods" } else { "Fabric mods" }
                                .into(),
                            detail: Some(n.to_string()),
                        });
                    }
                    module = Some(found.map_or("lunar", |(_, m)| m).to_string());
                }
                out.push(ImportSource {
                    path: dir.to_string_lossy().to_string(),
                    label: format!("{label} {name}").trim().to_string(),
                    matches_version,
                    module,
                    items,
                });
            }
        }
        out.sort_by_key(|s| !s.matches_version);
        out
    })
    .await
    .context("looking for the client's folders")
}

/// Copies the chosen parts of `source` into the instance. Files that already
/// exist in the instance are kept, except the server list and settings.
pub async fn import_from(
    kind: ClientKind,
    instance_id: &str,
    source: &str,
    items: Vec<String>,
) -> crate::Result<()> {
    let game_version = super::get(instance_id)
        .await?
        .map(|i| i.game_version)
        .unwrap_or_default();
    let source = PathBuf::from(source);
    // Only folders this module would offer can be copied from.
    if !is_allowed_source(&roots(kind), &source) || !is_game_dir(&source) {
        bail!("That folder isn't one of the client's game folders");
    }
    let dest = crate::api::instance::get_full_path(instance_id).await?;
    tokio::task::spawn_blocking(move || -> crate::Result<()> {
        for item in items {
            match item.as_str() {
                "servers" => copy_file(&source, &dest, "servers.dat")?,
                "options" => {
                    for f in ["options.txt", "optionsof.txt", "optionsshaders.txt", "optionsLC.txt"] {
                        copy_file(&source, &dest, f)?;
                    }
                }
                "resourcepacks" => {
                    for from in content_dirs(kind, &source, "resourcepacks") {
                        copy_dir(&from, &dest.join("resourcepacks"))?;
                    }
                }
                "shaderpacks" => {
                    // Clients read shader packs from their own folder name.
                    for f in ["shaderpacks", "shaders"] {
                        for from in content_dirs(kind, &source, f) {
                            copy_dir(&from, &dest.join(f))?;
                        }
                    }
                }
                "screenshots" | "saves" => {
                    for from in content_dirs(kind, &source, item.as_str()) {
                        copy_dir(&from, &dest.join(item.as_str()))?;
                    }
                }
                // Lunar's Fabric mods go where Modrinth keeps mods; they're
                // handed back to Lunar at launch.
                "mods" if kind == ClientKind::Lunar => {
                    if let Some((mods_dir, _)) = lunar_mods_dir(&source, &game_version) {
                        std::fs::create_dir_all(dest.join("mods"))?;
                        for entry in std::fs::read_dir(&mods_dir)?.flatten() {
                            let path = entry.path();
                            let target = dest.join("mods").join(entry.file_name());
                            if path.is_file()
                                && path.extension().is_some_and(|x| x == "jar")
                                && !target.exists()
                            {
                                std::fs::copy(&path, &target)?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })
    .await??;
    Ok(())
}

/// Only folders under one of the client's own roots can be copied from, and
/// "<root>/../somewhere" doesn't count as under it.
fn is_allowed_source(roots: &[(PathBuf, &'static str)], source: &Path) -> bool {
    !source
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir))
        && roots.iter().any(|(root, _)| source.starts_with(root))
}

fn copy_file(from_dir: &Path, to_dir: &Path, name: &str) -> crate::Result<()> {
    let from = from_dir.join(name);
    if from.is_file() {
        std::fs::create_dir_all(to_dir)?;
        std::fs::copy(&from, to_dir.join(name))?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> crate::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let ty = entry.file_type()?;
        let target = to.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if ty.is_file() && !target.exists() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn profile_names_use_the_major_version() {
        assert_eq!(major("1.8.9"), "1.8");
        assert_eq!(major("1.21.10"), "1.21");
        assert_eq!(major("26.3"), "26");
        assert_eq!(major("26"), "26");
    }

    #[test]
    fn game_folders_are_found_but_big_folders_are_skipped() {
        let root = tempfile::tempdir().unwrap();
        let r = root.path();
        touch(&r.join("1.8/options.txt"));
        touch(&r.join("1.21/servers.dat"));
        touch(&r.join("deep/a/b/c/options.txt"));
        touch(&r.join("1.8/saves/World/options.txt"));
        touch(&r.join(".hidden/options.txt"));
        touch(&r.join("empty/readme.txt"));
        let mut found = Vec::new();
        find_game_dirs(r, 2, &mut found);
        found.sort();
        assert_eq!(found, [r.join("1.21"), r.join("1.8")]);
        assert!(is_game_dir(&r.join("1.8")));
        assert!(!is_game_dir(&r.join("empty")));
    }

    #[test]
    fn the_search_stops_after_sixteen_folders() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..20 {
            touch(&root.path().join(format!("p{i}/options.txt")));
        }
        let mut found = Vec::new();
        find_game_dirs(root.path(), 1, &mut found);
        assert_eq!(found.len(), 16);
    }

    #[test]
    fn lunar_mods_prefer_this_version_then_the_fullest_folder() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("mods/fabric-1.21.10/a.jar"));
        touch(&d.join("mods/fabric-1.21.10/b.jar"));
        touch(&d.join("mods/forge-1.8.9/c.jar"));
        touch(&d.join("mods/fabric-1.20/readme.txt"));
        assert_eq!(
            lunar_mods_dir(d, "1.8.9"),
            Some((d.join("mods/forge-1.8.9"), "forge"))
        );
        assert_eq!(
            lunar_mods_dir(d, "1.20"),
            Some((d.join("mods/fabric-1.21.10"), "fabric"))
        );
        assert_eq!(lunar_mods_dir(&d.join("nothing"), "1.8.9"), None);
    }

    #[test]
    fn importable_items_only_list_what_exists() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        touch(&d.join("options.txt"));
        touch(&d.join("resourcepacks/a.zip"));
        touch(&d.join("resourcepacks/b.zip"));
        touch(&d.join("shaders/s.zip"));
        std::fs::create_dir_all(d.join("screenshots")).unwrap();
        let items = items_of(ClientKind::NoRisk, d);
        let summary: Vec<(&str, Option<&str>)> =
            items.iter().map(|i| (i.id.as_str(), i.detail.as_deref())).collect();
        assert_eq!(
            summary,
            [("options", None), ("resourcepacks", Some("2")), ("shaderpacks", Some("1"))]
        );
    }

    #[test]
    fn only_the_clients_own_folders_can_be_copied_from() {
        let roots = vec![(PathBuf::from("/home/me/.lunarclient/profiles"), "Lunar profile")];
        assert!(is_allowed_source(&roots, Path::new("/home/me/.lunarclient/profiles/1.8")));
        assert!(!is_allowed_source(&roots, Path::new("/home/me/Documents")));
        assert!(!is_allowed_source(
            &roots,
            Path::new("/home/me/.lunarclient/profiles/../../.ssh")
        ));
        assert!(!is_allowed_source(&roots, Path::new("/home/me/.lunarclient/profiles-other")));
    }

    #[test]
    fn copying_keeps_files_the_instance_already_has() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        touch(&from.path().join("packs/a.zip"));
        touch(&from.path().join("packs/sub/b.zip"));
        std::fs::create_dir_all(to.path().join("packs")).unwrap();
        std::fs::write(to.path().join("packs/a.zip"), b"mine").unwrap();
        copy_dir(&from.path().join("packs"), &to.path().join("packs")).unwrap();
        assert_eq!(std::fs::read(to.path().join("packs/a.zip")).unwrap(), b"mine");
        assert!(to.path().join("packs/sub/b.zip").exists());
        // Missing sources are fine.
        copy_dir(&from.path().join("nope"), &to.path().join("nope")).unwrap();
        assert!(!to.path().join("nope").exists());

        touch(&from.path().join("servers.dat"));
        copy_file(from.path(), to.path(), "servers.dat").unwrap();
        copy_file(from.path(), to.path(), "missing.txt").unwrap();
        assert!(to.path().join("servers.dat").exists());
        assert!(!to.path().join("missing.txt").exists());
    }
}
