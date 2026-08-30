use std::sync::{Arc, atomic::AtomicBool};

use chrono::{DateTime, Utc};
use discord_rich_presence::{
    DiscordIpc, DiscordIpcClient,
    activity::{Activity, ActivityType, Assets, Button, Timestamps},
};
use tokio::sync::RwLock;

use crate::State;
use crate::api::discord_rpc::{
    DiscordActivityKind, DiscordActivityMode, DiscordRpcSettings,
};

pub struct DiscordGuard {
    client: Arc<RwLock<DiscordIpcClient>>,
    connected: Arc<AtomicBool>,
}

/// Modrinth Studios addition: the instance-specific info a customizable
/// "playing" Discord activity may need to fill into its text (see
/// `crate::api::discord_rpc` for the settings this gets combined with).
/// `loader`/`game_version` may be empty if unknown — this is just plain
/// display text, not something worth failing a Discord status update over.
#[derive(Debug, Clone, Default)]
pub struct DiscordActivityContext {
    pub instance_name: String,
    pub loader: String,
    pub game_version: String,
}

fn discord_activity_type(kind: DiscordActivityKind) -> ActivityType {
    match kind {
        DiscordActivityKind::Playing => ActivityType::Playing,
        DiscordActivityKind::Listening => ActivityType::Listening,
        DiscordActivityKind::Watching => ActivityType::Watching,
        DiscordActivityKind::Competing => ActivityType::Competing,
    }
}

impl DiscordGuard {
    /// Initialize discord IPC client, and attempt to connect to it
    /// If it fails, it will still return a DiscordGuard, but the client will be unconnected
    pub fn init() -> crate::Result<DiscordGuard> {
        let dipc = DiscordIpcClient::new("1123683254248148992");

        Ok(DiscordGuard {
            client: Arc::new(RwLock::new(dipc)),
            connected: Arc::new(AtomicBool::new(false)),
        })
    }

    /// If the client failed connecting during init(), this will check for connection and attempt to reconnect
    /// This MUST be called first in any client method that requires a connection, because those can PANIC if the client is not connected
    /// (No connection is different than a failed connection, the latter will not panic and can be retried)
    pub async fn retry_if_not_ready(&self) -> bool {
        let mut client = self.client.write().await;
        if !self.connected.load(std::sync::atomic::Ordering::Relaxed) {
            if client.connect().is_ok() {
                self.connected
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                return true;
            }
            return false;
        }
        true
    }

    /// Modrinth Studios addition: builds and sets the "playing" activity for
    /// a launched instance, honoring the person's customization settings
    /// (`crate::api::discord_rpc`) — replaces the old fixed
    /// `"Playing {instance name}"` text upstream always sent. First checks
    /// if Discord RPC is disabled entirely, and if so, clears the activity
    /// instead.
    pub async fn set_instance_activity(
        &self,
        ctx: &DiscordActivityContext,
        launched_at: DateTime<Utc>,
        reconnect_if_fail: bool,
    ) -> crate::Result<()> {
        let state = State::get().await?;
        let settings = crate::state::Settings::get(&state.pool).await?;
        if !settings.discord_rpc {
            return self.clear_activity(true).await;
        }

        let rpc_settings = crate::api::discord_rpc::get_settings().await?;

        let (state_text, details_text) = match rpc_settings.mode {
            DiscordActivityMode::Default => {
                (format!("Playing {}", ctx.instance_name), None)
            }
            DiscordActivityMode::Detailed => (
                format!("Playing {}", ctx.instance_name),
                Some(format!("{} {}", ctx.loader, ctx.game_version)),
            ),
            DiscordActivityMode::Minimal => {
                ("Playing Minecraft".to_string(), None)
            }
            DiscordActivityMode::Custom => {
                let state_text =
                    if rpc_settings.custom_state_template.trim().is_empty() {
                        format!("Playing {}", ctx.instance_name)
                    } else {
                        DiscordRpcSettings::render_template(
                            &rpc_settings.custom_state_template,
                            &ctx.instance_name,
                            &ctx.loader,
                            &ctx.game_version,
                        )
                    };
                let details_text = if rpc_settings
                    .custom_details_template
                    .trim()
                    .is_empty()
                {
                    None
                } else {
                    Some(DiscordRpcSettings::render_template(
                        &rpc_settings.custom_details_template,
                        &ctx.instance_name,
                        &ctx.loader,
                        &ctx.game_version,
                    ))
                };
                (state_text, details_text)
            }
        };

        self.apply_activity(
            &state_text,
            details_text.as_deref(),
            Some(launched_at),
            &rpc_settings,
            reconnect_if_fail,
        )
        .await
    }

    /// Modrinth Studios addition: sets the idle activity (no instance
    /// running), honoring the person's customized idle text. First checks
    /// if Discord RPC is disabled entirely, and if so, clears the activity
    /// instead.
    pub async fn set_idle(
        &self,
        reconnect_if_fail: bool,
    ) -> crate::Result<()> {
        let state = State::get().await?;
        let settings = crate::state::Settings::get(&state.pool).await?;
        if !settings.discord_rpc {
            return self.clear_activity(true).await;
        }

        let rpc_settings = crate::api::discord_rpc::get_settings().await?;
        let idle_text = if rpc_settings.idle_text.trim().is_empty() {
            "Idling...".to_string()
        } else {
            rpc_settings.idle_text.clone()
        };

        self.apply_activity(
            &idle_text,
            None,
            None,
            &rpc_settings,
            reconnect_if_fail,
        )
        .await
    }

    /// Modrinth Studios addition: the actual low-level activity builder,
    /// shared by `set_instance_activity` and `set_idle` — assembles the
    /// Discord `Activity` from already-decided text plus whatever's
    /// currently configured for elapsed time / activity verb / buttons, and
    /// sends it. Does not check the `discord_rpc` setting itself; callers
    /// are expected to have already decided this activity should be shown.
    async fn apply_activity(
        &self,
        state_text: &str,
        details_text: Option<&str>,
        start_time: Option<DateTime<Utc>>,
        rpc_settings: &DiscordRpcSettings,
        reconnect_if_fail: bool,
    ) -> crate::Result<()> {
        if !self.retry_if_not_ready().await {
            return Ok(());
        }

        let mut activity = Activity::new()
            .state(state_text)
            .assets(
                Assets::new()
                    .large_image("modrinth_simple")
                    .large_text("Modrinth Logo"),
            )
            .activity_type(discord_activity_type(rpc_settings.activity_type));

        if let Some(details) = details_text {
            activity = activity.details(details);
        }

        if rpc_settings.show_elapsed_time {
            if let Some(start) = start_time {
                activity = activity
                    .timestamps(Timestamps::new().start(start.timestamp()));
            }
        }

        let mut buttons = Vec::new();
        if let (Some(label), Some(url)) = (
            rpc_settings.button_1_label.as_deref(),
            rpc_settings.button_1_url.as_deref(),
        ) {
            if !label.is_empty() && !url.is_empty() {
                buttons.push(Button::new(label, url));
            }
        }
        if buttons.len() < 2 {
            if let (Some(label), Some(url)) = (
                rpc_settings.button_2_label.as_deref(),
                rpc_settings.button_2_url.as_deref(),
            ) {
                if !label.is_empty() && !url.is_empty() {
                    buttons.push(Button::new(label, url));
                }
            }
        }
        if !buttons.is_empty() {
            activity = activity.buttons(buttons);
        }

        // Attempt to set the activity
        // If the existing connection fails, attempt to reconnect and try again
        let mut client: tokio::sync::RwLockWriteGuard<'_, DiscordIpcClient> =
            self.client.write().await;
        let res = client.set_activity(activity.clone());

        if reconnect_if_fail {
            if let Err(_e) = res {
                client.reconnect()?;
                return Ok(client.set_activity(activity)?); // try again, but don't reconnect if it fails again
            }
        } else {
            res?;
        }

        Ok(())
    }

    /// Clear the activity entirely ('disabling' the RPC until the next set_activity)
    pub async fn clear_activity(
        &self,
        reconnect_if_fail: bool,
    ) -> crate::Result<()> {
        // Attempt to connect if not connected. Do not continue if it fails, as the client.clear_activity can panic if it never was connected
        if !self.retry_if_not_ready().await {
            return Ok(());
        }

        // Attempt to clear the activity
        // If the existing connection fails, attempt to reconnect and try again
        let mut client = self.client.write().await;
        let res = client.clear_activity();

        if reconnect_if_fail {
            if res.is_err() {
                client.reconnect()?;
                return Ok(client.clear_activity()?); // try again, but don't reconnect if it fails again
            }
        } else {
            res?;
        }
        Ok(())
    }

    /// Clear the activity, but if there is a running profile, set the activity to that instead
    pub async fn clear_to_default(
        &self,
        reconnect_if_fail: bool,
    ) -> crate::Result<()> {
        let state = State::get().await?;

        let settings = crate::state::Settings::get(&state.pool).await?;
        if !settings.discord_rpc {
            self.clear_activity(true).await?;
            return Ok(());
        }

        let running_instances = state.process_manager.get_all();
        if let Some(existing_child) = running_instances.first() {
            // Modrinth Studios note: loader/version are looked up fresh here
            // rather than threaded through `ProcessMetadata`, since this can
            // be called well after launch (app startup, or right after the
            // person edits these settings) — not just at the moment of
            // launch, where `launch_minecraft` already has this info on
            // hand and passes it directly via `set_instance_activity`.
            let (instance_name, loader, game_version) =
                crate::api::discord_rpc::instance_display_info(
                    &existing_child.instance_id,
                )
                .await
                .unwrap_or_else(|_| {
                    (existing_child.instance_name.clone(), String::new(), String::new())
                });

            self.set_instance_activity(
                &DiscordActivityContext {
                    instance_name,
                    loader,
                    game_version,
                },
                existing_child.start_time,
                reconnect_if_fail,
            )
            .await?;
        } else {
            self.set_idle(reconnect_if_fail).await?;
        }
        Ok(())
    }
}
