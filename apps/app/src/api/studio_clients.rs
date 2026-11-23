//! Modrinth Studios addition: client installations (Horizon, NoRisk Client,
//! Lunar Client, LabyMod). See packages/app-lib/src/api/studio_clients.

use crate::api::Result;
use tauri::Runtime;
use theseus::studio_clients::{
    self, ClientInstall, ClientKind, ClientVersion, HorizonProject,
    ImportSource, NewsItem, NoRiskCape, NoRiskCapePage, NoRiskMod,
};

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("studio-clients")
        .invoke_handler(tauri::generate_handler![
            studio_clients_versions,
            studio_clients_setup,
            studio_clients_get,
            studio_clients_prepare,
            studio_clients_news,
            studio_clients_icon,
            norisk_pack_mods,
            norisk_sign_in,
            norisk_signed_in,
            norisk_capes_browse,
            norisk_capes_mine,
            norisk_cape_equip,
            norisk_cape_unequip,
            norisk_cape_upload,
            norisk_cape_texture,
            horizon_store_list,
            horizon_store_install,
            horizon_store_remove,
            studio_clients_import_sources,
            studio_clients_import,
            studio_news_text,
            studio_news_media,
        ])
        .build()
}

#[tauri::command]
pub async fn studio_clients_versions(
    client: ClientKind,
) -> Result<Vec<ClientVersion>> {
    Ok(studio_clients::list_versions(client).await?)
}

#[tauri::command]
pub async fn studio_clients_setup(
    instance_id: String,
    client: ClientKind,
    version: ClientVersion,
) -> Result<ClientInstall> {
    Ok(studio_clients::setup(&instance_id, client, version).await?)
}

#[tauri::command]
pub async fn studio_clients_get(
    instance_id: String,
) -> Result<Option<ClientInstall>> {
    Ok(studio_clients::get(&instance_id).await?)
}

#[tauri::command]
pub async fn studio_clients_prepare(instance_id: String) -> Result<()> {
    Ok(studio_clients::prepare(&instance_id).await?)
}

#[tauri::command]
pub async fn horizon_store_list(
    instance_id: Option<String>,
) -> Result<Vec<HorizonProject>> {
    Ok(studio_clients::horizon_store_list(instance_id.as_deref()).await?)
}

#[tauri::command]
pub async fn horizon_store_install(
    instance_id: String,
    project_id: String,
) -> Result<()> {
    Ok(
        studio_clients::horizon_store_install(&instance_id, &project_id)
            .await?,
    )
}

#[tauri::command]
pub async fn studio_clients_news(client: ClientKind) -> Result<Vec<NewsItem>> {
    Ok(studio_clients::news(client).await?)
}

#[tauri::command]
pub async fn studio_clients_icon(
    client: ClientKind,
) -> Result<std::path::PathBuf> {
    Ok(studio_clients::icon(client).await?)
}

#[tauri::command]
pub async fn norisk_pack_mods(instance_id: String) -> Result<Vec<NoRiskMod>> {
    Ok(studio_clients::norisk_pack_mods(&instance_id).await?)
}

#[tauri::command]
pub async fn horizon_store_remove(
    instance_id: String,
    project_id: String,
) -> Result<()> {
    Ok(studio_clients::horizon_store_remove(&instance_id, &project_id).await?)
}

#[tauri::command]
pub async fn norisk_sign_in() -> Result<String> {
    Ok(studio_clients::norisk_sign_in().await?)
}

#[tauri::command]
pub async fn norisk_signed_in() -> Result<Option<String>> {
    Ok(studio_clients::norisk_signed_in().await?)
}

#[tauri::command]
pub async fn norisk_capes_browse(
    sort: String,
    time_frame: Option<String>,
    cursor: Option<String>,
) -> Result<NoRiskCapePage> {
    Ok(studio_clients::norisk_capes_browse(
        &sort,
        time_frame.as_deref(),
        cursor.as_deref(),
    )
    .await?)
}

#[tauri::command]
pub async fn norisk_capes_mine() -> Result<Vec<NoRiskCape>> {
    Ok(studio_clients::norisk_capes_mine().await?)
}

#[tauri::command]
pub async fn norisk_cape_equip(hash: String) -> Result<()> {
    Ok(studio_clients::norisk_cape_equip(&hash).await?)
}

#[tauri::command]
pub async fn norisk_cape_unequip() -> Result<()> {
    Ok(studio_clients::norisk_cape_unequip().await?)
}

#[tauri::command]
pub async fn norisk_cape_upload(path: std::path::PathBuf) -> Result<String> {
    Ok(studio_clients::norisk_cape_upload(&path).await?)
}

#[tauri::command]
pub async fn norisk_cape_texture(
    hash: String,
    review: Option<bool>,
) -> Result<String> {
    Ok(
        studio_clients::norisk_cape_texture(&hash, review.unwrap_or(false))
            .await?,
    )
}

#[tauri::command]
pub async fn studio_clients_import_sources(
    client: ClientKind,
    game_version: String,
) -> Result<Vec<ImportSource>> {
    Ok(studio_clients::import_sources(client, game_version).await?)
}

#[tauri::command]
pub async fn studio_clients_import(
    client: ClientKind,
    instance_id: String,
    source: String,
    items: Vec<String>,
) -> Result<()> {
    Ok(studio_clients::import_from(client, &instance_id, &source, items).await?)
}

#[tauri::command]
pub async fn studio_news_text(url: String) -> Result<String> {
    Ok(studio_clients::news_text(&url).await?)
}

#[tauri::command]
pub async fn studio_news_media(url: String) -> Result<tauri::ipc::Response> {
    Ok(tauri::ipc::Response::new(
        studio_clients::news_media(&url).await?,
    ))
}
