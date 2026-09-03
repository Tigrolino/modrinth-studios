use crate::launcher::get_loader_version_from_profile;
use crate::state::instances::{
    ContentSet, ContentSetStatus, ContentSourceKind, Instance,
    InstanceLaunchOverrides, InstanceLink,
    adapters::sqlite::{content_rows, instance_rows},
};
use crate::state::{
    InstanceIconConfig, InstanceInstallStage, LauncherFeatureVersion,
    ModLoader, ReleaseChannel, State,
};
use crate::util::fetch;
use crate::util::io;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{info, trace};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateInstance {
    pub name: String,
    pub path: Option<String>,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub icon_path: Option<String>,
    pub icon_config: Option<InstanceIconConfig>,
    pub link: InstanceLink,
}

pub(crate) async fn create_instance(
    input: CreateInstance,
    state: &State,
) -> crate::Result<Instance> {
    trace!("Creating new instance. {}", input.name);

    let (path, full_path) =
        resolve_instance_path(&input.name, input.path.as_deref(), None, state)
            .await?;
    io::create_dir_all(&full_path).await?;

    let result = async {
        info!(
            "Creating instance at path {}",
            &io::canonicalize(&full_path)?.display()
        );

        let loader_version = if input.loader != ModLoader::Vanilla {
            get_loader_version_from_profile(
                &input.game_version,
                input.loader,
                input.loader_version.as_deref(),
            )
            .await?
            .map(|value| value.id)
        } else {
            None
        };

        let icon_path = resolve_icon_path(
            input.icon_path.as_deref(),
            matches!(&input.link, InstanceLink::SharedInstance { .. }),
            state,
        )
        .await?;
        let now = Utc::now();
        let instance_id = format!("local:{}", Uuid::new_v4());
        let content_set_id = format!("content-set:{}", Uuid::new_v4());
        let instance = Instance {
            id: instance_id.clone(),
            path: path.clone(),
            applied_content_set_id: Some(content_set_id.clone()),
            install_stage: InstanceInstallStage::NotInstalled,
            launcher_feature_version: LauncherFeatureVersion::MOST_RECENT,
            update_channel: ReleaseChannel::Release,
            name: input.name,
            icon_path,
            created: now,
            modified: now,
            last_played: None,
            submitted_time_played: 0,
            recent_time_played: 0,
        };
        let content_set = ContentSet {
            id: content_set_id,
            instance_id: instance_id.clone(),
            name: "Default".to_string(),
            source_kind: content_source_kind(&input.link),
            status: ContentSetStatus::Available,
            game_version: input.game_version,
            protocol_version: None,
            loader: input.loader,
            loader_version,
            created: now,
            modified: now,
        };
        let launch_overrides =
            InstanceLaunchOverrides::empty(instance_id.clone());

        let mut tx = state.pool.begin().await?;
        instance_rows::insert_instance(&instance, &mut tx).await?;
        instance_rows::insert_default_instance_sync_preferences(
            &instance_id,
            &mut tx,
        )
        .await?;
        if let Some(icon_config) = &input.icon_config {
            instance_rows::update_instance_icon_config(
                &instance_id,
                Some(icon_config),
                &mut tx,
            )
            .await?;
        }
        content_rows::insert_content_set(&content_set, &mut tx).await?;
        instance_rows::upsert_instance_link(&instance_id, &input.link, &mut tx)
            .await?;
        instance_rows::replace_instance_groups(&instance_id, &[], &mut tx)
            .await?;
        instance_rows::upsert_instance_launch_overrides(
            &launch_overrides,
            &mut tx,
        )
        .await?;
        tx.commit().await?;

        // Modrinth Studios addition: see reconcile_instance_paths.rs — this
        // is what lets a later external rename of this folder be traced
        // back to this instance instead of just breaking it.
        super::write_instance_marker(&full_path, &instance.id).await;

        crate::state::instances::watcher::watch_instance_folder(
            &instance.id,
            &instance.path,
            &state.file_watcher,
            &state.directories,
        )
        .await;
        if let Err(error) =
            crate::api::instance::reconcile_instance_synced_options(
                &instance.id,
            )
            .await
        {
            tracing::warn!(
                "Failed to reconcile synced options for newly created instance {}: {error}",
                instance.id
            );
        }

        Ok(instance)
    }
    .await;

    if result.is_err() {
        let _ = io::remove_dir_all(&full_path).await;
    }

    result
}

// Modrinth Studios addition: visibility widened from private to pub(crate)
// so rename_instance_folder.rs can reuse the exact same sanitize +
// collision-avoidance logic for the in-app rename-folder-to-match-name
// feature, rather than a second, potentially-diverging copy of it.
//
// `exclude_instance_id`, if given, is the instance being *renamed* (not one
// being newly created — `create_instance()` below always passes `None`,
// since a brand new instance can never collide with itself). Without this,
// a rename to a name that already resolves to the instance's own current
// folder would never actually recognize that folder as available to
// itself — `path_available()` sees the folder physically existing (it does,
// it's the instance's own) and treats that as "taken," so it kept walking
// to the next `(N)` and renaming the instance sideways into it for no
// reason, on every single edit that didn't actually change the effective
// name (a modpack install applying its resolved title right after creating
// the instance under that exact same title being the most common trigger).
// That spurious rename was purely wasteful in the best case, and could fail
// outright in the worst case (see `rename_instance_folder_for_name_change`'s
// doc comment on the file-watcher race) — for no reason, since nothing
// actually needed to move.
pub(crate) async fn resolve_instance_path(
    name: &str,
    path: Option<&str>,
    exclude_instance_id: Option<&str>,
    state: &State,
) -> crate::Result<(String, std::path::PathBuf)> {
    let base_path = path
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_instance_name(name));
    let mut path = base_path.clone();
    let mut full_path = state.directories.instances_dir().join(&path);

    if path_available(&path, &full_path, exclude_instance_id, state).await? {
        return Ok((path, full_path));
    }

    let mut which = 1;
    loop {
        path = format!("{base_path} ({which})");
        full_path = state.directories.instances_dir().join(&path);

        if path_available(&path, &full_path, exclude_instance_id, state).await?
        {
            return Ok((path, full_path));
        }

        which += 1;
    }
}

async fn path_available(
    path: &str,
    full_path: &std::path::Path,
    exclude_instance_id: Option<&str>,
    state: &State,
) -> crate::Result<bool> {
    // Check the database first, not the filesystem: a path already owned by
    // `exclude_instance_id` itself is always available *to that instance*,
    // regardless of what's physically sitting there (it's that instance's
    // own current folder) — this is the one case a pure `full_path.exists()`
    // check can never get right, since the folder existing is exactly what's
    // supposed to happen when a rename resolves back to a no-op.
    if let Some(existing) = instance_rows::get_instance_by_path(path, &state.pool).await? {
        return Ok(exclude_instance_id == Some(existing.id.as_str()));
    }

    Ok(!full_path.exists())
}

pub(crate) async fn resolve_icon_path(
    icon_path: Option<&str>,
    ignore_missing_remote_icon: bool,
    state: &State,
) -> crate::Result<Option<String>> {
    let Some(icon) = icon_path else {
        return Ok(None);
    };

    let file = if icon.starts_with("https://") || icon.starts_with("http://") {
        let bytes = match fetch::fetch(
            icon,
            None,
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(error) if ignore_missing_remote_icon => {
                tracing::warn!("Error while getting instance icon: {error}");
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        crate::api::instance::cache_icon(bytes, state).await?
    } else {
        crate::api::instance::cache_icon_from_path(
            &state.directories.caches_dir().join(icon),
            state,
        )
        .await?
    };

    Ok(Some(file.to_string_lossy().to_string()))
}

fn content_source_kind(link: &InstanceLink) -> ContentSourceKind {
    match link {
        InstanceLink::Unmanaged => ContentSourceKind::Local,
        InstanceLink::ModrinthModpack { .. } => {
            ContentSourceKind::ModrinthModpack
        }
        InstanceLink::ServerProject { .. }
        | InstanceLink::ServerProjectModpack { .. } => {
            ContentSourceKind::ServerProject
        }
        InstanceLink::ModrinthHosting { .. } => {
            ContentSourceKind::ModrinthHosting
        }
        InstanceLink::ImportedModpack { .. } => {
            ContentSourceKind::ImportedModpack
        }
        InstanceLink::SharedInstance { .. } => {
            ContentSourceKind::SharedInstance
        }
    }
}

fn sanitize_instance_name(input: &str) -> String {
    input.replace(
        ['/', '\\', '?', '*', ':', '\'', '\"', '|', '<', '>', '!'],
        "_",
    )
}
