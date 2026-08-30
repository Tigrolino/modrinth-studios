//! Modrinth Studios addition: lets the person customize what Studio's
//! Discord Rich Presence status actually shows, instead of the fixed
//! "Playing <instance name>" upstream always sends. Kept in its own table
//! (see the `studio_discord_rpc_settings` migration) rather than new columns
//! on `settings`, for the same SQLX_OFFLINE reason `studio_playtime_corrections`
//! and `studio_shared_profiles` already avoid touching upstream-queried
//! tables — see either of those modules for the full explanation.
//!
//! Deliberately does **not** let someone set a custom large/small image.
//! Both would need a Discord "Rich Presence art asset" key already uploaded
//! to *Modrinth's own* Discord application (the real, shared Modrinth app ID
//! this client authenticates as, see `state::discord::DiscordGuard::init`) —
//! Studio has no access to upload assets there, and setting an image key
//! that doesn't exist on that application just silently shows nothing. So
//! that's left out entirely rather than offered as a setting that looks
//! broken. Text (state/details lines, the activity's verb, buttons, elapsed
//! time, idle text) has no such restriction — none of that needs an asset
//! upload, so all of it is fully customizable here.
use crate::Result;
use crate::state::State;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

/// Controls what text the "playing" activity actually shows. See each
/// variant for exactly what it changes; `show_elapsed_time`, `activity_type`,
/// and the buttons apply on top of all four modes, not just `Custom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscordActivityMode {
    /// "Playing <instance name>" — matches upstream's original behavior.
    Default,
    /// Adds a second line showing the instance's loader and Minecraft
    /// version (e.g. "fabric 1.21.11").
    Detailed,
    /// Just "Playing Minecraft" — hides which specific instance/modpack is
    /// running, for anyone who'd rather not broadcast that.
    Minimal,
    /// The person's own text for both lines, with `{instance}`, `{loader}`,
    /// and `{version}` placeholders. An empty template falls back to that
    /// line's `Default`-mode text.
    Custom,
}

impl DiscordActivityMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Detailed => "detailed",
            Self::Minimal => "minimal",
            Self::Custom => "custom",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "detailed" => Self::Detailed,
            "minimal" => Self::Minimal,
            "custom" => Self::Custom,
            _ => Self::Default,
        }
    }
}

/// Which verb Discord shows before the activity text (e.g. "Playing X" vs
/// "Watching X"). Purely cosmetic — Discord doesn't treat any of these
/// differently otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscordActivityKind {
    Playing,
    Listening,
    Watching,
    Competing,
}

impl DiscordActivityKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Playing => "playing",
            Self::Listening => "listening",
            Self::Watching => "watching",
            Self::Competing => "competing",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "listening" => Self::Listening,
            "watching" => Self::Watching,
            "competing" => Self::Competing,
            _ => Self::Playing,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordRpcSettings {
    pub mode: DiscordActivityMode,
    pub activity_type: DiscordActivityKind,
    pub show_elapsed_time: bool,
    pub custom_state_template: String,
    pub custom_details_template: String,
    pub idle_text: String,
    /// Both the label and URL must be non-empty for this button to actually
    /// show. Discord allows at most 2 buttons per activity.
    pub button_1_label: Option<String>,
    pub button_1_url: Option<String>,
    pub button_2_label: Option<String>,
    pub button_2_url: Option<String>,
}

impl Default for DiscordRpcSettings {
    fn default() -> Self {
        Self {
            mode: DiscordActivityMode::Default,
            activity_type: DiscordActivityKind::Playing,
            show_elapsed_time: true,
            custom_state_template: String::new(),
            custom_details_template: String::new(),
            idle_text: "Idling...".to_string(),
            button_1_label: None,
            button_1_url: None,
            button_2_label: None,
            button_2_url: None,
        }
    }
}

impl DiscordRpcSettings {
    fn from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            mode: DiscordActivityMode::parse(
                &row.try_get::<String, _>("mode")?,
            ),
            activity_type: DiscordActivityKind::parse(
                &row.try_get::<String, _>("activity_type")?,
            ),
            show_elapsed_time: row.try_get::<i64, _>("show_elapsed_time")?
                != 0,
            custom_state_template: row.try_get("custom_state_template")?,
            custom_details_template: row
                .try_get("custom_details_template")?,
            idle_text: row.try_get("idle_text")?,
            button_1_label: row.try_get("button_1_label")?,
            button_1_url: row.try_get("button_1_url")?,
            button_2_label: row.try_get("button_2_label")?,
            button_2_url: row.try_get("button_2_url")?,
        })
    }

    /// Renders `{instance}`/`{loader}`/`{version}` placeholders in a custom
    /// template against a specific instance's info.
    pub(crate) fn render_template(
        template: &str,
        instance_name: &str,
        loader: &str,
        game_version: &str,
    ) -> String {
        template
            .replace("{instance}", instance_name)
            .replace("{loader}", loader)
            .replace("{version}", game_version)
    }
}

/// Current settings, or the defaults (matching upstream's original behavior)
/// if never customized.
pub async fn get_settings() -> Result<DiscordRpcSettings> {
    let state = State::get().await?;

    let row = sqlx::query(
        "SELECT * FROM studio_discord_rpc_settings WHERE id = 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    Ok(match row {
        Some(row) => DiscordRpcSettings::from_row(&row)?,
        None => DiscordRpcSettings::default(),
    })
}

/// Saves the settings, then immediately re-applies the current Discord
/// activity (idle, or whatever's currently running) so the change is
/// visible right away without restarting the app.
pub async fn set_settings(settings: DiscordRpcSettings) -> Result<()> {
    let state = State::get().await?;

    sqlx::query(
        "
		INSERT INTO studio_discord_rpc_settings (
			id, mode, activity_type, show_elapsed_time,
			custom_state_template, custom_details_template, idle_text,
			button_1_label, button_1_url, button_2_label, button_2_url
		)
		VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
			mode = excluded.mode,
			activity_type = excluded.activity_type,
			show_elapsed_time = excluded.show_elapsed_time,
			custom_state_template = excluded.custom_state_template,
			custom_details_template = excluded.custom_details_template,
			idle_text = excluded.idle_text,
			button_1_label = excluded.button_1_label,
			button_1_url = excluded.button_1_url,
			button_2_label = excluded.button_2_label,
			button_2_url = excluded.button_2_url
		",
    )
    .bind(settings.mode.as_str())
    .bind(settings.activity_type.as_str())
    .bind(settings.show_elapsed_time)
    .bind(&settings.custom_state_template)
    .bind(&settings.custom_details_template)
    .bind(&settings.idle_text)
    .bind(&settings.button_1_label)
    .bind(&settings.button_1_url)
    .bind(&settings.button_2_label)
    .bind(&settings.button_2_url)
    .execute(&state.pool)
    .await?;

    // Best-effort: a failure here shouldn't stop the settings from having
    // saved successfully.
    let _ = state.discord_rpc.clear_to_default(true).await;

    Ok(())
}

/// Looks up an instance's name, loader, and Minecraft version for use in a
/// Discord activity — the loader/version live on the instance's *applied
/// content set*, not on the instance row itself (see
/// `state::instances::model::instance::Instance`), so this is a join rather
/// than a single-table lookup. Used when re-applying the current activity
/// (e.g. right after the person changes these settings, or on app startup)
/// where only an instance id is on hand, not the full launch context.
pub(crate) async fn instance_display_info(
    instance_id: &str,
) -> Result<(String, String, String)> {
    let state = State::get().await?;

    let row = sqlx::query(
        "
		SELECT i.name AS name, cs.loader AS loader, cs.game_version AS game_version
		FROM instances i
		LEFT JOIN instance_content_sets cs ON cs.id = i.applied_content_set_id
		WHERE i.id = ?
		",
    )
    .bind(instance_id)
    .fetch_optional(&state.pool)
    .await?;

    Ok(match row {
        Some(row) => (
            row.try_get::<String, _>("name")?,
            row.try_get::<Option<String>, _>("loader")?
                .unwrap_or_default(),
            row.try_get::<Option<String>, _>("game_version")?
                .unwrap_or_default(),
        ),
        None => (instance_id.to_string(), String::new(), String::new()),
    })
}
