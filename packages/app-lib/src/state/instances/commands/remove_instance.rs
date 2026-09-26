use crate::state::State;
use crate::state::instances::adapters::sqlite::instance_rows;
use crate::util::io;

pub(crate) async fn remove_instance(
    instance_id: &str,
    state: &State,
) -> crate::Result<()> {
    let instance = instance_rows::get_instance_by_id(instance_id, &state.pool)
        .await?
        .ok_or_else(|| {
            crate::ErrorKind::InputError("Unknown instance".to_string())
        })?;
    let _synced_options_lock = state.lock_synced_options().await;
    let _content_lock = state.lock_instance_content(instance_id).await;
    let _store_lock = state.content_store.files_lock.lock().await;
    let _store_lease = state.content_store.lease().await;
    if crate::state::instance_has_running_process(instance_id, state).await? {
        return Err(crate::state::content_store::input(
            "Stop this instance before removing it",
        ));
    }
    state.content_store.recover(Some(instance_id)).await?;
    crate::api::instance::remove_generated_instance_files(instance_id, state)
        .await?;

    delete_instance_row_and_locks(&instance.id, state).await?;

    // Modrinth Studios fix: the instance is already gone from the database by
    // this point — as far as the rest of the app (and the caller's
    // `emit_instance(Removed)` right after this returns) is concerned, it no
    // longer exists, full stop. Previously, a failure here (`?` on
    // `remove_dir_all`) propagated straight up and skipped that emit
    // entirely, which left a "ghost" card sitting in the Library — its DB row
    // already deleted, so clicking it threw "Unknown instance", but with no
    // event ever telling the frontend to refetch and drop it. It would only
    // disappear once some unrelated event happened to trigger a refetch.
    //
    // The most common way to hit this on Windows is a file in the instance
    // folder (a log file, a world's session.lock, a `.jar`) still being held
    // open for a moment — by a just-exited game process, an antivirus scan,
    // or a search indexer — right as the person deletes the instance. A few
    // short retries absorb that transient case; if it's still stuck after
    // that, we give up on the folder (logging it so it's at least
    // discoverable) rather than resurrecting a "removed" instance by
    // reporting failure.
    let path = state.directories.instances_dir().join(&instance.path);
    if path.exists() {
        const MAX_ATTEMPTS: u32 = 5;
        let mut attempt = 0;
        loop {
            match io::remove_dir_all(&path).await {
                Ok(()) => break,
                Err(err) if attempt + 1 < MAX_ATTEMPTS => {
                    attempt += 1;
                    tracing::warn!(
                        "Failed to remove instance folder '{}' (attempt {attempt}/{MAX_ATTEMPTS}): {err}. Retrying shortly.",
                        path.display()
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        400 * attempt as u64,
                    ))
                    .await;
                }
                Err(err) => {
                    tracing::warn!(
                        "Giving up removing instance folder '{}' for already-deleted instance '{instance_id}' after {MAX_ATTEMPTS} attempts: {err}. It may need to be deleted manually.",
                        path.display()
                    );
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn delete_instance_row_and_locks(
    instance_id: &str,
    state: &State,
) -> crate::Result<()> {
    // Keep these together so deleted instances cannot leave stale entries in the per-instance lock maps.
    instance_rows::delete_instance_by_id(instance_id, &state.pool).await?;
    state.remove_instance_locks(instance_id);

    Ok(())
}
