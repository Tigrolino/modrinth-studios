//! Modrinth Studios addition: bridge to self-hosted Pterodactyl panels.
//!
//! The hosting UI talks to Modrinth's own backends (Archon/Kyros). For
//! servers living on a user's Pterodactyl panel, the frontend translates
//! those calls into Pterodactyl Client API calls (see
//! `apps/app-frontend/src/platform/pterodactyl/`). Everything that touches
//! the network goes through here instead of the webview because:
//!
//! - the panel can live on any URL (often plain `http://` on the LAN), which
//!   the webview's CSP / HTTP-plugin scope would block,
//! - the API key never has to leave the Rust side once saved,
//! - Wings only accepts console WebSocket connections whose `Origin` header
//!   is the panel URL, which a webview can't fake but Rust can.
//!
//! Panels are stored in `<config>/studio/pterodactyl_panels.json`.

use crate::api::Result;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::http::HeaderValue;
use dashmap::DashMap;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{Emitter, Manager, Runtime};
use tokio::sync::mpsc;

pub fn init<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("pterodactyl")
        .invoke_handler(tauri::generate_handler![
            ptero_list_panels,
            ptero_add_panel,
            ptero_remove_panel,
            ptero_request,
            ptero_request_bytes,
            ptero_write_bytes,
            ptero_upload_local,
            ptero_install_mrpack,
            ptero_ws_connect,
            ptero_ws_send,
            ptero_ws_close,
        ])
        .build()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Panel {
    id: String,
    name: String,
    url: String,
    api_key: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct PanelInfo {
    id: String,
    name: String,
    url: String,
}

impl From<&Panel> for PanelInfo {
    fn from(panel: &Panel) -> Self {
        Self {
            id: panel.id.clone(),
            name: panel.name.clone(),
            url: panel.url.clone(),
        }
    }
}

static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("ModrinthStudio-Pterodactyl")
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build Pterodactyl HTTP client")
});

fn panels_path<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_config_dir()?
        .join("studio")
        .join("pterodactyl_panels.json"))
}

async fn load_panels<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<Vec<Panel>> {
    let path = panels_path(app)?;
    match tokio::fs::read(&path).await {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

async fn save_panels<R: Runtime>(
    app: &tauri::AppHandle<R>,
    panels: &[Panel],
) -> Result<()> {
    let path = panels_path(app)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, serde_json::to_vec_pretty(panels)?).await?;
    Ok(())
}

async fn get_panel<R: Runtime>(
    app: &tauri::AppHandle<R>,
    panel_id: &str,
) -> Result<Panel> {
    load_panels(app)
        .await?
        .into_iter()
        .find(|p| p.id == panel_id)
        .ok_or_else(|| {
            anyhow::anyhow!("Pterodactyl panel {panel_id} is not configured")
                .into()
        })
}

fn normalize_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn api_url(panel: &Panel, path: &str) -> String {
    let path = if path == "/" || path.starts_with("/?") {
        // The server list lives at `/api/client` itself; avoid a trailing
        // slash so Laravel doesn't answer with a redirect.
        path[1..].to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{}/api/client{}", panel.url, path)
}

fn authed(panel: &Panel, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder
        .bearer_auth(&panel.api_key)
        .header("Accept", "application/json")
}

#[tauri::command]
async fn ptero_list_panels<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<Vec<PanelInfo>> {
    Ok(load_panels(&app).await?.iter().map(PanelInfo::from).collect())
}

#[tauri::command]
async fn ptero_add_panel<R: Runtime>(
    app: tauri::AppHandle<R>,
    name: String,
    url: String,
    api_key: String,
) -> Result<PanelInfo> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err(anyhow::anyhow!("An API key is required").into());
    }
    let mut panel = Panel {
        id: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
        name: name.trim().to_string(),
        url: normalize_url(&url),
        api_key,
    };

    // Validate the URL + key before saving anything.
    let response = authed(&panel, HTTP.get(api_url(&panel, "/account")))
        .send()
        .await
        .map_err(|e| {
            anyhow::anyhow!("Could not reach the panel at {}: {e}", panel.url)
        })?;
    match response.status().as_u16() {
        200..=299 => {}
        401 | 403 => {
            return Err(anyhow::anyhow!(
                "The panel rejected this API key. Make sure it's a client key (starts with ptlc_) from Account → API Credentials, and that your IP is allowed."
            )
            .into());
        }
        status => {
            return Err(anyhow::anyhow!(
                "The panel answered with HTTP {status}. Is {} a Pterodactyl panel?",
                panel.url
            )
            .into());
        }
    }

    if panel.name.is_empty() {
        panel.name = url::Url::parse(&panel.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "Pterodactyl".to_string());
    }

    let mut panels = load_panels(&app).await?;
    panels.retain(|p| p.url != panel.url);
    panels.push(panel.clone());
    save_panels(&app, &panels).await?;
    Ok(PanelInfo::from(&panel))
}

#[tauri::command]
async fn ptero_remove_panel<R: Runtime>(
    app: tauri::AppHandle<R>,
    panel_id: String,
) -> Result<()> {
    let mut panels = load_panels(&app).await?;
    panels.retain(|p| p.id != panel_id);
    save_panels(&app, &panels).await
}

#[derive(Serialize)]
pub struct PteroResponse {
    status: u16,
    body: String,
}

/// Generic Client API call. `path` is relative to `/api/client` (query
/// string included). Non-2xx answers are returned, not thrown, so the
/// frontend can map them to the right error.
#[tauri::command]
async fn ptero_request<R: Runtime>(
    app: tauri::AppHandle<R>,
    panel_id: String,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
    text_body: Option<String>,
) -> Result<PteroResponse> {
    let panel = get_panel(&app, &panel_id).await?;
    let method = reqwest::Method::from_bytes(method.to_uppercase().as_bytes())?;
    let mut builder = authed(&panel, HTTP.request(method, api_url(&panel, &path)));
    if let Some(body) = body {
        builder = builder.json(&body);
    } else if let Some(text) = text_body {
        builder = builder.header("Content-Type", "text/plain").body(text);
    }
    let response = builder.send().await?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    Ok(PteroResponse { status, body })
}

/// Raw bytes of a Client API GET (used for file contents).
#[tauri::command]
async fn ptero_request_bytes<R: Runtime>(
    app: tauri::AppHandle<R>,
    panel_id: String,
    path: String,
) -> Result<Response> {
    let panel = get_panel(&app, &panel_id).await?;
    let response = authed(&panel, HTTP.get(api_url(&panel, &path)))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Panel returned HTTP {} for {path}",
            status.as_u16()
        )
        .into());
    }
    Ok(Response::new(response.bytes().await?.to_vec()))
}

fn header(request: &Request<'_>, name: &str) -> Result<String> {
    let value = request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("missing {name} header"))?;
    Ok(urlencoding::decode(value)?.into_owned())
}

/// Writes raw bytes to a server file. Headers:
/// - `x-ptero-panel`, `x-ptero-server`: target server
/// - `x-ptero-path`: URL-encoded file path (directory + name)
/// - `x-ptero-mode`: `write` (Client API write endpoint, fine for text) or
///   `upload` (signed Wings upload, for any size/binary)
#[tauri::command]
async fn ptero_write_bytes<R: Runtime>(
    app: tauri::AppHandle<R>,
    request: Request<'_>,
) -> Result<()> {
    let panel_id = header(&request, "x-ptero-panel")?;
    let server = header(&request, "x-ptero-server")?;
    let path = header(&request, "x-ptero-path")?;
    let mode = header(&request, "x-ptero-mode")
        .unwrap_or_else(|_| "upload".to_string());
    let bytes = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(_) => {
            return Err(anyhow::anyhow!("expected a raw byte body").into());
        }
    };
    let panel = get_panel(&app, &panel_id).await?;

    if mode == "write" {
        let url = api_url(
            &panel,
            &format!(
                "/servers/{server}/files/write?file={}",
                urlencoding::encode(&path)
            ),
        );
        let response = authed(&panel, HTTP.post(url))
            .header("Content-Type", "application/octet-stream")
            .body(bytes)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Panel refused to write {path} (HTTP {})",
                response.status().as_u16()
            )
            .into());
        }
        return Ok(());
    }

    upload_bytes(&panel, &server, &path, bytes).await
}

/// Uploads a file from the local disk (used for drag-and-drop uploads, which
/// hand the uploader a path instead of in-memory bytes).
#[tauri::command]
async fn ptero_upload_local<R: Runtime>(
    app: tauri::AppHandle<R>,
    panel_id: String,
    server: String,
    path: String,
    local_path: String,
) -> Result<()> {
    let panel = get_panel(&app, &panel_id).await?;
    let bytes = tokio::fs::read(&local_path).await?;
    upload_bytes(&panel, &server, &path, bytes).await
}

async fn upload_bytes(
    panel: &Panel,
    server: &str,
    path: &str,
    bytes: Vec<u8>,
) -> Result<()> {
    let (directory, filename) = match path.rsplit_once('/') {
        Some((dir, name)) => {
            (if dir.is_empty() { "/" } else { dir }.to_string(), name.to_string())
        }
        None => ("/".to_string(), path.to_string()),
    };

    #[derive(Deserialize)]
    struct SignedUrl {
        attributes: SignedUrlAttrs,
    }
    #[derive(Deserialize)]
    struct SignedUrlAttrs {
        url: String,
    }
    let signed: SignedUrl = authed(
        panel,
        HTTP.get(api_url(panel, &format!("/servers/{server}/files/upload"))),
    )
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;

    let separator = if signed.attributes.url.contains('?') { '&' } else { '?' };
    let upload_url = format!(
        "{}{separator}directory={}",
        signed.attributes.url,
        urlencoding::encode(&directory)
    );
    let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
    let form = reqwest::multipart::Form::new().part("files", part);
    let response = HTTP
        .post(upload_url)
        .header("Origin", &panel.url)
        .multipart(form)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Wings refused the upload of {path} (HTTP {})",
            response.status().as_u16()
        )
        .into());
    }
    Ok(())
}

// --- Modrinth modpack (.mrpack) installer -------------------------------------

#[derive(Deserialize)]
struct MrIndex {
    name: String,
    #[serde(rename = "versionId", default)]
    version_id: String,
    #[serde(default)]
    files: Vec<MrFile>,
    #[serde(default)]
    dependencies: std::collections::HashMap<String, String>,
}

#[derive(Deserialize)]
struct MrFile {
    path: String,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
    env: Option<MrEnv>,
    #[serde(default)]
    downloads: Vec<String>,
}

#[derive(Deserialize)]
struct MrEnv {
    #[serde(default)]
    server: String,
}

#[derive(Serialize)]
pub struct MrpackInstalledFile {
    path: String,
    sha1: Option<String>,
}

#[derive(Serialize)]
pub struct MrpackSummary {
    name: String,
    version_id: String,
    dependencies: std::collections::HashMap<String, String>,
    files: Vec<MrpackInstalledFile>,
    overrides: usize,
}

#[derive(Serialize, Clone)]
struct MrpackProgress {
    done: usize,
    total: usize,
    stage: String,
}

fn safe_relative_path(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    if normalized.starts_with('/')
        || normalized.split('/').any(|part| part == ".." || part.is_empty())
        || normalized.contains(':')
    {
        return None;
    }
    Some(normalized)
}

async fn pull_or_upload(
    panel: &Panel,
    server: &str,
    url: &str,
    path: &str,
) -> Result<()> {
    let (dir, name) = match path.rsplit_once('/') {
        Some((dir, name)) => (format!("/{dir}"), name.to_string()),
        None => ("/".to_string(), path.to_string()),
    };
    let pulled = authed(
        panel,
        HTTP.post(api_url(panel, &format!("/servers/{server}/files/pull"))),
    )
    .json(&serde_json::json!({
        "url": url,
        "directory": dir,
        "filename": name,
        "foreground": true,
    }))
    .send()
    .await
    .map(|r| r.status().is_success())
    .unwrap_or(false);
    if pulled {
        return Ok(());
    }
    // Remote downloads disabled in Wings: fetch here and upload instead.
    let bytes = HTTP
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?
        .to_vec();
    upload_bytes(panel, server, &format!("/{path}"), bytes).await
}

/// Installs a `.mrpack` onto a Pterodactyl server: downloads every
/// server-side file straight onto the node, and unpacks `overrides/` +
/// `server-overrides/`. Headers: `x-ptero-panel`, `x-ptero-server`,
/// `x-ptero-event` (progress event name), and optionally `x-ptero-url`
/// (download the pack from there) — otherwise the raw body is the pack.
/// Loader/version changes are left to the frontend (egg variables).
#[tauri::command]
async fn ptero_install_mrpack<R: Runtime>(
    app: tauri::AppHandle<R>,
    request: Request<'_>,
) -> Result<MrpackSummary> {
    use std::io::{Cursor, Read, Write};

    let panel_id = header(&request, "x-ptero-panel")?;
    let server = header(&request, "x-ptero-server")?;
    let event = header(&request, "x-ptero-event")
        .unwrap_or_else(|_| "ptero-mrpack-progress".to_string());
    let source_url = header(&request, "x-ptero-url").ok();
    let local_path = header(&request, "x-ptero-local").ok();
    let body = match request.body() {
        InvokeBody::Raw(bytes) => bytes.clone(),
        InvokeBody::Json(_) => Vec::new(),
    };
    let panel = get_panel(&app, &panel_id).await?;

    let pack = match source_url {
        Some(url) if !url.is_empty() => HTTP
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec(),
        _ if !body.is_empty() => body,
        _ if local_path.as_deref().is_some_and(|p| !p.is_empty()) => {
            tokio::fs::read(local_path.unwrap_or_default()).await?
        }
        _ => return Err(anyhow::anyhow!("No modpack file was provided").into()),
    };

    // Parse the archive up front (sync), collecting everything we need.
    let (index, overrides_zip, overrides_count) = {
        let mut archive = zip::ZipArchive::new(Cursor::new(pack))
            .map_err(|e| anyhow::anyhow!("Not a valid .mrpack: {e}"))?;
        let index: MrIndex = {
            let mut entry = archive
                .by_name("modrinth.index.json")
                .map_err(|_| anyhow::anyhow!("modrinth.index.json missing from pack"))?;
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            serde_json::from_str(&text)?
        };

        // Merge overrides/ then server-overrides/ (later wins) into one zip.
        let mut merged: std::collections::BTreeMap<String, Vec<u8>> =
            std::collections::BTreeMap::new();
        for prefix in ["overrides/", "server-overrides/"] {
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name()?.replace('\\', "/");
                let Some(rest) = name.strip_prefix(prefix) else {
                    continue;
                };
                let Some(rest) = safe_relative_path(rest) else {
                    continue;
                };
                let mut data = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut data)?;
                merged.insert(rest, data);
            }
        }
        let count = merged.len();
        let overrides_zip = if merged.is_empty() {
            None
        } else {
            let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (path, data) in merged {
                writer.start_file(path, options)?;
                writer.write_all(&data)?;
            }
            Some(writer.finish()?.into_inner())
        };
        (index, overrides_zip, count)
    };

    let files: Vec<&MrFile> = index
        .files
        .iter()
        .filter(|f| {
            f.env.as_ref().map(|e| e.server.as_str()) != Some("unsupported")
        })
        .collect();
    let total = files.len() + usize::from(overrides_zip.is_some());
    let emit = |done: usize, stage: &str| {
        let _ = app.emit(
            &event,
            MrpackProgress {
                done,
                total,
                stage: stage.to_string(),
            },
        );
    };
    emit(0, "Downloading mods");

    let mut installed = Vec::new();
    for (i, file) in files.iter().enumerate() {
        let Some(path) = safe_relative_path(&file.path) else {
            continue;
        };
        let Some(url) = file.downloads.first() else {
            continue;
        };
        pull_or_upload(&panel, &server, url, &path).await.map_err(|e| {
            anyhow::anyhow!("Failed to install {path}: {}", e.0)
        })?;
        installed.push(MrpackInstalledFile {
            path,
            sha1: file.hashes.get("sha1").cloned(),
        });
        emit(i + 1, "Downloading mods");
    }

    if let Some(zip_bytes) = overrides_zip {
        emit(files.len(), "Copying config files");
        let archive_name = ".studio-overrides.zip";
        upload_bytes(&panel, &server, &format!("/{archive_name}"), zip_bytes)
            .await?;
        let decompress = authed(
            &panel,
            HTTP.post(api_url(
                &panel,
                &format!("/servers/{server}/files/decompress"),
            )),
        )
        .json(&serde_json::json!({ "root": "/", "file": archive_name }))
        .send()
        .await?;
        let decompress_ok = decompress.status().is_success();
        let _ = authed(
            &panel,
            HTTP.post(api_url(&panel, &format!("/servers/{server}/files/delete"))),
        )
        .json(&serde_json::json!({ "root": "/", "files": [archive_name] }))
        .send()
        .await;
        if !decompress_ok {
            return Err(anyhow::anyhow!(
                "The panel couldn't unpack the modpack's config files (HTTP {})",
                decompress.status().as_u16()
            )
            .into());
        }
        emit(total, "Done");
    }

    Ok(MrpackSummary {
        name: index.name,
        version_id: index.version_id,
        dependencies: index.dependencies,
        files: installed,
        overrides: overrides_count,
    })
}

// --- Console WebSocket proxy -------------------------------------------------

enum WsCommand {
    Send(String),
    Close,
}

static SOCKETS: LazyLock<DashMap<String, mpsc::UnboundedSender<WsCommand>>> =
    LazyLock::new(DashMap::new);

#[derive(Deserialize)]
struct WsCredentials {
    data: WsCredentialsData,
}
#[derive(Deserialize)]
struct WsCredentialsData {
    token: String,
    socket: String,
}

async fn fetch_ws_credentials(
    panel: &Panel,
    server: &str,
) -> Result<WsCredentialsData> {
    let creds: WsCredentials = authed(
        panel,
        HTTP.get(api_url(panel, &format!("/servers/{server}/websocket"))),
    )
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
    Ok(creds.data)
}

fn auth_message(token: &str) -> Message {
    Message::text(
        serde_json::json!({ "event": "auth", "args": [token] }).to_string(),
    )
}

/// Opens a console socket to Wings. Wings events are forwarded verbatim
/// (JSON text) as `ptero-ws:<conn_id>` events; when the socket ends a
/// `ptero-ws-close:<conn_id>` event is emitted. Authentication and token
/// refreshes are handled here, so the frontend only ever sees an
/// authenticated stream.
#[tauri::command]
async fn ptero_ws_connect<R: Runtime>(
    app: tauri::AppHandle<R>,
    conn_id: String,
    panel_id: String,
    server: String,
) -> Result<()> {
    if let Some((_, old)) = SOCKETS.remove(&conn_id) {
        let _ = old.send(WsCommand::Close);
    }

    let panel = get_panel(&app, &panel_id).await?;
    let creds = fetch_ws_credentials(&panel, &server).await?;

    let mut request = creds.socket.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert("Origin", HeaderValue::from_str(&panel.url)?);
    let (stream, _) = async_tungstenite::tokio::connect_async(request)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Could not open the console connection to Wings ({}): {e}",
                creds.socket
            )
        })?;
    let (mut sink, mut stream) = stream.split();
    sink.send(auth_message(&creds.token)).await?;

    let (tx, mut rx) = mpsc::unbounded_channel::<WsCommand>();
    SOCKETS.insert(conn_id.clone(), tx.clone());

    let message_event = format!("ptero-ws:{conn_id}");
    let close_event = format!("ptero-ws-close:{conn_id}");

    tauri::async_runtime::spawn(async move {
        let mut close_code: u16 = 1006;
        loop {
            tokio::select! {
                command = rx.recv() => match command {
                    Some(WsCommand::Send(text)) => {
                        if sink.send(Message::text(text)).await.is_err() {
                            break;
                        }
                    }
                    Some(WsCommand::Close) | None => {
                        let _ = sink.close(None).await;
                        close_code = 1000;
                        break;
                    }
                },
                incoming = stream.next() => match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let text = text.to_string();
                        let event = serde_json::from_str::<serde_json::Value>(&text)
                            .ok()
                            .and_then(|v| v.get("event").and_then(|e| e.as_str()).map(str::to_string))
                            .unwrap_or_default();
                        match event.as_str() {
                            "auth success" => {
                                for request in ["send logs", "send stats"] {
                                    let _ = sink
                                        .send(Message::text(
                                            serde_json::json!({ "event": request, "args": [null] })
                                                .to_string(),
                                        ))
                                        .await;
                                }
                            }
                            "token expiring" | "token expired" => {
                                match fetch_ws_credentials(&panel, &server).await {
                                    Ok(new_creds) => {
                                        let _ = sink.send(auth_message(&new_creds.token)).await;
                                    }
                                    Err(e) => {
                                        tracing::warn!("Pterodactyl token refresh failed: {e:?}");
                                    }
                                }
                                continue;
                            }
                            _ => {}
                        }
                        let _ = app.emit(&message_event, text);
                    }
                    Some(Ok(Message::Close(frame))) => {
                        close_code = frame.map(|f| u16::from(f.code)).unwrap_or(1000);
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        tracing::warn!("Pterodactyl console socket error: {e}");
                        break;
                    }
                    None => break,
                },
            }
        }
        if SOCKETS
            .get(&conn_id)
            .is_some_and(|current| current.same_channel(&tx))
        {
            SOCKETS.remove(&conn_id);
        }
        let _ = app.emit(&close_event, close_code);
    });

    Ok(())
}

#[tauri::command]
async fn ptero_ws_send(conn_id: String, text: String) -> Result<()> {
    if let Some(socket) = SOCKETS.get(&conn_id) {
        let _ = socket.send(WsCommand::Send(text));
    }
    Ok(())
}

#[tauri::command]
async fn ptero_ws_close(conn_id: String) -> Result<()> {
    if let Some((_, socket)) = SOCKETS.remove(&conn_id) {
        let _ = socket.send(WsCommand::Close);
    }
    Ok(())
}
