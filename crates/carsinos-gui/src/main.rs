use eframe::egui;
use eframe::egui::{Color32, RichText};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone)]
struct HealthSnapshot {
    ok: bool,
    service: String,
    version: String,
    uptime_ms: u64,
    now_utc: String,
}

#[derive(Debug, Clone)]
struct StatusSnapshot {
    service: String,
    version: String,
    started_at_utc: String,
    uptime_ms: u64,
    db_path: String,
    attachments_path: String,
}

#[derive(Debug, Clone)]
struct SessionListItem {
    session_id: String,
    title: Option<String>,
    message_count: i64,
    run_count: i64,
    updated_at: i64,
}

#[derive(Debug, Clone)]
struct ApprovalListItem {
    approval_id: String,
    run_id: String,
    kind: String,
    status: String,
    request_summary: String,
    requested_at: i64,
}

#[derive(Debug, Clone)]
struct TimelineMessage {
    role: String,
    content_text: String,
    created_at: i64,
}

#[derive(Debug, Clone)]
struct AuthProfileListItem {
    auth_profile_id: String,
    provider: String,
    display_name: String,
    auth_mode: String,
    risk_level: String,
    enabled: bool,
    kill_switch_scope: String,
    api_base_url: Option<String>,
    updated_at: i64,
}

#[derive(Debug, Clone)]
struct ChannelConfigSnapshot {
    discord_require_mention_in_guild_channels: bool,
    discord_allowlisted_user_ids_csv: String,
    telegram_require_mention_in_groups: bool,
    telegram_allowlisted_user_ids_csv: String,
    updated_at: i64,
}

impl Default for ChannelConfigSnapshot {
    fn default() -> Self {
        Self {
            discord_require_mention_in_guild_channels: true,
            discord_allowlisted_user_ids_csv: String::new(),
            telegram_require_mention_in_groups: true,
            telegram_allowlisted_user_ids_csv: String::new(),
            updated_at: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Mission,
    Sessions,
    Approvals,
    Auth,
    Channels,
}

impl MainTab {
    fn label(self) -> &'static str {
        match self {
            MainTab::Mission => "Mission",
            MainTab::Sessions => "Sessions",
            MainTab::Approvals => "Approvals",
            MainTab::Auth => "Auth",
            MainTab::Channels => "Channels",
        }
    }
}

#[derive(Debug, Clone)]
struct AuthProfileDraft {
    provider: String,
    display_name: String,
    auth_mode: String,
    risk_level: String,
    enabled: bool,
    kill_switch_scope: String,
    api_base_url: String,
    credentials_json_text: String,
}

impl Default for AuthProfileDraft {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            display_name: String::new(),
            auth_mode: "api_key".to_string(),
            risk_level: "low".to_string(),
            enabled: true,
            kill_switch_scope: "none".to_string(),
            api_base_url: "https://api.openai.com".to_string(),
            credentials_json_text: "{}".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct OpenAiOauthDraft {
    display_name: String,
    client_id: String,
    scope: String,
    authorize_url: String,
    token_url: String,
    api_base_url: String,
    oauth_session_id: String,
    authorize_url_result: String,
    callback_url: String,
    manual_code: String,
    manual_state: String,
}

impl Default for OpenAiOauthDraft {
    fn default() -> Self {
        Self {
            display_name: "openai-codex".to_string(),
            client_id: String::new(),
            scope: "offline_access".to_string(),
            authorize_url: "https://auth.openai.com/oauth/authorize".to_string(),
            token_url: "https://auth.openai.com/oauth/token".to_string(),
            api_base_url: "https://api.openai.com".to_string(),
            oauth_session_id: String::new(),
            authorize_url_result: String::new(),
            callback_url: String::new(),
            manual_code: String::new(),
            manual_state: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct AnthropicSetupTokenDraft {
    display_name: String,
    setup_token: String,
    api_base_url: String,
    enabled: bool,
    kill_switch_scope: String,
}

impl Default for AnthropicSetupTokenDraft {
    fn default() -> Self {
        Self {
            display_name: "anthropic-setup-token".to_string(),
            setup_token: String::new(),
            api_base_url: "https://api.anthropic.com".to_string(),
            enabled: true,
            kill_switch_scope: "none".to_string(),
        }
    }
}

#[derive(Debug)]
struct GuiApp {
    theme_applied: bool,
    initial_load_done: bool,
    auto_launch_attempted: bool,
    active_tab: MainTab,

    gateway_base_url: String,
    gateway_token: String,

    health: Option<HealthSnapshot>,
    status: Option<StatusSnapshot>,
    sessions: Vec<SessionListItem>,
    approvals: Vec<ApprovalListItem>,
    auth_profiles: Vec<AuthProfileListItem>,
    channel_config: ChannelConfigSnapshot,

    selected_session_id: Option<String>,
    timeline: Vec<TimelineMessage>,

    new_session_title: String,
    composer_role: String,
    composer_content_text: String,
    run_model_provider: String,
    run_model_id: String,
    run_auth_profile_id: String,

    auth_profile_draft: AuthProfileDraft,
    openai_oauth_draft: OpenAiOauthDraft,
    anthropic_setup_draft: AnthropicSetupTokenDraft,
    auth_order_agent_id: String,
    auth_order_provider: String,
    auth_order_profile_ids_csv: String,

    last_error: Option<String>,
    last_info: Option<String>,
}

type GatewaySnapshots = (
    HealthSnapshot,
    StatusSnapshot,
    Vec<SessionListItem>,
    Vec<ApprovalListItem>,
    Vec<AuthProfileListItem>,
    ChannelConfigSnapshot,
);

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            theme_applied: false,
            initial_load_done: false,
            auto_launch_attempted: false,
            active_tab: MainTab::Mission,
            gateway_base_url: std::env::var("CARSINOS_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18789".to_string()),
            gateway_token: std::env::var("CARSINOS_GATEWAY_TOKEN").unwrap_or_default(),
            health: None,
            status: None,
            sessions: Vec::new(),
            approvals: Vec::new(),
            auth_profiles: Vec::new(),
            channel_config: ChannelConfigSnapshot::default(),
            selected_session_id: None,
            timeline: Vec::new(),
            new_session_title: String::new(),
            composer_role: "user".to_string(),
            composer_content_text: String::new(),
            run_model_provider: "mock".to_string(),
            run_model_id: "mock-echo-v1".to_string(),
            run_auth_profile_id: String::new(),
            auth_profile_draft: AuthProfileDraft::default(),
            openai_oauth_draft: OpenAiOauthDraft::default(),
            anthropic_setup_draft: AnthropicSetupTokenDraft::default(),
            auth_order_agent_id: "default".to_string(),
            auth_order_provider: "openai".to_string(),
            auth_order_profile_ids_csv: String::new(),
            last_error: None,
            last_info: None,
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_applied {
            apply_frontend_design_theme(ctx);
            self.theme_applied = true;
        }

        if !self.initial_load_done {
            self.refresh_gateway_state();
            self.initial_load_done = true;
        }

        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(
                    RichText::new("carsinOS Control Deck")
                        .size(28.0)
                        .color(Color32::from_rgb(255, 214, 109)),
                );
                ui.label(
                    RichText::new("Operator console for sessions, approvals, auth, and channels")
                        .color(Color32::from_rgb(164, 212, 255)),
                );
                ui.separator();
                if ui.button("Refresh All").clicked() {
                    self.refresh_gateway_state();
                }
                if ui.button("Clear Notices").clicked() {
                    self.last_error = None;
                    self.last_info = None;
                }
            });
        });

        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            if let Some(error) = &self.last_error {
                ui.colored_label(
                    Color32::from_rgb(255, 107, 107),
                    format!("error: {}", error),
                );
            } else if let Some(info) = &self.last_info {
                ui.colored_label(Color32::from_rgb(118, 255, 168), info);
            } else {
                ui.label("Ready");
            }
        });

        egui::SidePanel::left("navigation")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| {
                card(ui, "Gateway", |ui| {
                    ui.label("URL");
                    ui.text_edit_singleline(&mut self.gateway_base_url);
                    ui.label("Bearer Token");
                    ui.add(egui::TextEdit::singleline(&mut self.gateway_token).password(true));
                    if ui.button("Reconnect + Refresh").clicked() {
                        self.refresh_gateway_state();
                    }
                });

                card(ui, "Navigation", |ui| {
                    for tab in [
                        MainTab::Mission,
                        MainTab::Sessions,
                        MainTab::Approvals,
                        MainTab::Auth,
                        MainTab::Channels,
                    ] {
                        let selected = self.active_tab == tab;
                        if ui
                            .selectable_label(selected, tab.label())
                            .on_hover_text(format!("Open {} view", tab.label()))
                            .clicked()
                        {
                            self.active_tab = tab;
                        }
                    }
                });

                card(ui, "Live Snapshot", |ui| {
                    if let Some(health) = &self.health {
                        ui.horizontal(|ui| {
                            let color = if health.ok {
                                Color32::from_rgb(118, 255, 168)
                            } else {
                                Color32::from_rgb(255, 107, 107)
                            };
                            ui.colored_label(color, if health.ok { "ONLINE" } else { "OFFLINE" });
                            ui.label(format!("version {}", health.version));
                        });
                        ui.label(format!("uptime: {} ms", health.uptime_ms));
                    } else {
                        ui.label("No health snapshot loaded");
                    }
                    ui.separator();
                    ui.label(format!("sessions: {}", self.sessions.len()));
                    ui.label(format!("approvals: {}", self.approvals.len()));
                    ui.label(format!("auth profiles: {}", self.auth_profiles.len()));
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            MainTab::Mission => self.render_mission(ui),
            MainTab::Sessions => self.render_sessions(ui),
            MainTab::Approvals => self.render_approvals(ui),
            MainTab::Auth => self.render_auth(ui),
            MainTab::Channels => self.render_channels(ui),
        });
    }
}

impl GuiApp {
    fn set_error(&mut self, message: impl Into<String>) {
        self.last_info = None;
        self.last_error = Some(message.into());
    }

    fn set_info(&mut self, message: impl Into<String>) {
        self.last_error = None;
        self.last_info = Some(message.into());
    }

    fn refresh_gateway_state(&mut self) {
        self.last_error = None;
        match fetch_gateway_snapshots(&self.gateway_base_url, &self.gateway_token) {
            Ok(snapshots) => {
                self.apply_snapshots(snapshots);
                self.set_info("Gateway state refreshed");
            }
            Err(err) => {
                if !self.auto_launch_attempted && auto_launch_gateway_enabled() {
                    self.auto_launch_attempted = true;
                    if let Err(launch_err) = self.launch_gateway_process() {
                        self.set_error(format!(
                            "{}; gateway auto-launch failed: {}",
                            err, launch_err
                        ));
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(350));
                    match fetch_gateway_snapshots(&self.gateway_base_url, &self.gateway_token) {
                        Ok(snapshots) => {
                            self.apply_snapshots(snapshots);
                            self.set_info("Gateway auto-launched and connected");
                        }
                        Err(retry_err) => self.set_error(format!(
                            "{}; gateway auto-launch retry failed: {}",
                            err, retry_err
                        )),
                    }
                } else {
                    self.set_error(err);
                }
            }
        }
    }

    fn apply_snapshots(
        &mut self,
        (health, status, sessions, approvals, auth_profiles, channel_config): GatewaySnapshots,
    ) {
        self.health = Some(health);
        self.status = Some(status);
        self.sessions = sessions;
        self.approvals = approvals;
        self.auth_profiles = auth_profiles;
        self.channel_config = channel_config;
        if self.selected_session_id.is_none() {
            self.selected_session_id = self.sessions.first().map(|s| s.session_id.clone());
        } else if let Some(selected) = &self.selected_session_id {
            if !self.sessions.iter().any(|s| &s.session_id == selected) {
                self.selected_session_id = self.sessions.first().map(|s| s.session_id.clone());
            }
        }
        let _ = self.load_timeline_for_selected();
    }

    fn launch_gateway_process(&mut self) -> Result<(), String> {
        if self.gateway_token.trim().is_empty() {
            self.gateway_token = "carsinos-local-token".to_string();
        }
        let mut command = resolve_gateway_binary_path()
            .map(Command::new)
            .unwrap_or_else(|| Command::new("carsinos-gateway"));
        command
            .env("CARSINOS_GATEWAY_TOKEN", self.gateway_token.clone())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
            .spawn()
            .map(|_| ())
            .map_err(|err| format!("failed to spawn gateway process: {}", err))
    }

    fn load_timeline_for_selected(&mut self) -> Result<(), String> {
        let session_id = self
            .selected_session_id
            .clone()
            .ok_or_else(|| "no session selected".to_string())?;
        self.timeline =
            fetch_session_timeline(&self.gateway_base_url, &self.gateway_token, &session_id)?;
        Ok(())
    }

    fn create_session(&mut self) {
        let title = self.new_session_title.trim().to_string();
        let payload = if title.is_empty() {
            json!({})
        } else {
            json!({"title": title})
        };
        match send_json(
            &self.gateway_base_url,
            "/api/v1/sessions",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.new_session_title.clear();
                self.refresh_gateway_state();
                self.set_info("Session created");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn send_message_to_selected(&mut self) {
        let session_id = match self.selected_session_id.clone() {
            Some(value) => value,
            None => {
                self.set_error("select a session first");
                return;
            }
        };
        let content_text = self.composer_content_text.trim().to_string();
        if content_text.is_empty() {
            self.set_error("message content cannot be empty");
            return;
        }
        let payload = json!({
            "role": self.composer_role,
            "content_text": content_text
        });
        match send_json(
            &self.gateway_base_url,
            &format!("/api/v1/sessions/{}/messages", session_id),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.composer_content_text.clear();
                let _ = self.load_timeline_for_selected();
                self.refresh_gateway_state();
                self.set_info("Message posted");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn create_run_for_selected(&mut self) {
        let session_id = match self.selected_session_id.clone() {
            Some(value) => value,
            None => {
                self.set_error("select a session first");
                return;
            }
        };
        let mut payload = json!({
            "model_provider": self.run_model_provider.trim(),
            "model_id": self.run_model_id.trim()
        });
        let auth_profile = self.run_auth_profile_id.trim();
        if !auth_profile.is_empty() {
            payload["auth_profile_id"] = Value::String(auth_profile.to_string());
        }

        match send_json(
            &self.gateway_base_url,
            &format!("/api/v1/sessions/{}/runs", session_id),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.refresh_gateway_state();
                let _ = self.load_timeline_for_selected();
                self.set_info("Run created");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn resolve_approval(&mut self, approval_id: &str, decision: &str) {
        let payload = json!({
            "decision": decision,
            "decided_via": "gui"
        });
        match send_json(
            &self.gateway_base_url,
            &format!("/api/v1/approvals/{}/resolve", approval_id),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.refresh_gateway_state();
                self.set_info(format!("Approval {} {}", approval_id, decision));
            }
            Err(err) => self.set_error(err),
        }
    }

    fn create_auth_profile(&mut self) {
        let display_name = self.auth_profile_draft.display_name.trim();
        if display_name.is_empty() {
            self.set_error("auth profile display_name cannot be empty");
            return;
        }
        let credentials_json: Value =
            match serde_json::from_str(self.auth_profile_draft.credentials_json_text.trim()) {
                Ok(value) => value,
                Err(err) => {
                    self.set_error(format!("credentials_json must be valid JSON: {}", err));
                    return;
                }
            };

        let payload = json!({
            "provider": self.auth_profile_draft.provider.trim().to_ascii_lowercase(),
            "display_name": display_name,
            "auth_mode": self.auth_profile_draft.auth_mode.trim().to_ascii_lowercase(),
            "risk_level": self.auth_profile_draft.risk_level.trim().to_ascii_lowercase(),
            "enabled": self.auth_profile_draft.enabled,
            "kill_switch_scope": self.auth_profile_draft.kill_switch_scope.trim().to_ascii_lowercase(),
            "api_base_url": if self.auth_profile_draft.api_base_url.trim().is_empty() { Value::Null } else { Value::String(self.auth_profile_draft.api_base_url.trim().to_string()) },
            "credentials_json": credentials_json
        });

        match send_json(
            &self.gateway_base_url,
            "/api/v1/auth/profiles",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.auth_profile_draft.display_name.clear();
                self.auth_profile_draft.credentials_json_text = "{}".to_string();
                self.refresh_gateway_state();
                self.set_info("Auth profile created");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn start_openai_oauth(&mut self) {
        let client_id = self.openai_oauth_draft.client_id.trim().to_string();
        if client_id.is_empty() {
            self.set_error("openai oauth client_id is required");
            return;
        }
        let payload = json!({
            "display_name": self.openai_oauth_draft.display_name.trim(),
            "client_id": client_id,
            "scope": self.openai_oauth_draft.scope.trim(),
            "authorize_url": self.openai_oauth_draft.authorize_url.trim(),
            "token_url": self.openai_oauth_draft.token_url.trim(),
            "api_base_url": self.openai_oauth_draft.api_base_url.trim(),
        });
        match send_json(
            &self.gateway_base_url,
            "/api/v1/auth/openai/oauth/start",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(value) => {
                self.openai_oauth_draft.oauth_session_id = value
                    .get("oauth_session_id")
                    .and_then(|item| item.as_str())
                    .unwrap_or_default()
                    .to_string();
                self.openai_oauth_draft.authorize_url_result = value
                    .get("authorize_url")
                    .and_then(|item| item.as_str())
                    .unwrap_or_default()
                    .to_string();
                self.openai_oauth_draft.callback_url = value
                    .get("callback_url")
                    .and_then(|item| item.as_str())
                    .unwrap_or_default()
                    .to_string();
                self.openai_oauth_draft.manual_code.clear();
                self.openai_oauth_draft.manual_state.clear();
                self.set_info("OAuth session created. Open authorize URL, then finish with callback URL or manual code/state.");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn finish_openai_oauth(&mut self) {
        let oauth_session_id = self.openai_oauth_draft.oauth_session_id.trim().to_string();
        if oauth_session_id.is_empty() {
            self.set_error("oauth_session_id is required. Start OAuth first.");
            return;
        }
        let callback_url = self.openai_oauth_draft.callback_url.trim().to_string();
        let mut payload = json!({
            "oauth_session_id": oauth_session_id,
            "api_base_url": self.openai_oauth_draft.api_base_url.trim()
        });
        if !callback_url.is_empty() {
            payload["callback_url"] = Value::String(callback_url);
        } else {
            let code = self.openai_oauth_draft.manual_code.trim().to_string();
            let state = self.openai_oauth_draft.manual_state.trim().to_string();
            if code.is_empty() || state.is_empty() {
                self.set_error("provide callback_url OR both manual code/state to finish OAuth");
                return;
            }
            payload["code"] = Value::String(code);
            payload["state"] = Value::String(state);
        }

        match send_json(
            &self.gateway_base_url,
            "/api/v1/auth/openai/oauth/finish",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(value) => {
                let profile_id = value
                    .get("profile")
                    .and_then(|profile| profile.get("auth_profile_id"))
                    .and_then(|item| item.as_str())
                    .unwrap_or_default()
                    .to_string();
                self.openai_oauth_draft.oauth_session_id.clear();
                self.openai_oauth_draft.authorize_url_result.clear();
                self.openai_oauth_draft.callback_url.clear();
                self.openai_oauth_draft.manual_code.clear();
                self.openai_oauth_draft.manual_state.clear();
                self.refresh_gateway_state();
                self.set_info(format!(
                    "OpenAI OAuth profile created{}",
                    if profile_id.is_empty() {
                        String::new()
                    } else {
                        format!(": {profile_id}")
                    }
                ));
            }
            Err(err) => self.set_error(err),
        }
    }

    fn ingest_anthropic_setup_token(&mut self) {
        let display_name = self.anthropic_setup_draft.display_name.trim().to_string();
        if display_name.is_empty() {
            self.set_error("anthropic setup display_name cannot be empty");
            return;
        }
        let setup_token = self.anthropic_setup_draft.setup_token.trim().to_string();
        if setup_token.is_empty() {
            self.set_error("anthropic setup_token cannot be empty");
            return;
        }
        let payload = json!({
            "display_name": display_name,
            "setup_token": setup_token,
            "api_base_url": self.anthropic_setup_draft.api_base_url.trim(),
            "enabled": self.anthropic_setup_draft.enabled,
            "kill_switch_scope": self.anthropic_setup_draft.kill_switch_scope.trim().to_ascii_lowercase()
        });
        match send_json(
            &self.gateway_base_url,
            "/api/v1/auth/anthropic/setup-token/ingest",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.anthropic_setup_draft.setup_token.clear();
                self.refresh_gateway_state();
                self.set_info("Anthropic setup-token profile created");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn set_auth_profile_enabled(
        &mut self,
        auth_profile_id: &str,
        enabled: bool,
        kill_switch_scope: &str,
    ) {
        let payload = json!({
            "enabled": enabled,
            "kill_switch_scope": kill_switch_scope
        });
        match send_json(
            &self.gateway_base_url,
            &format!("/api/v1/auth/profiles/{}/state", auth_profile_id),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.refresh_gateway_state();
                self.set_info("Auth profile state updated");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn load_auth_order(&mut self) {
        let agent_id = self.auth_order_agent_id.trim();
        let provider = self.auth_order_provider.trim().to_ascii_lowercase();
        if agent_id.is_empty() || provider.is_empty() {
            self.set_error("agent_id and provider are required");
            return;
        }
        match fetch_json(
            &self.gateway_base_url,
            &format!(
                "/api/v1/auth/agents/{}/providers/{}/profile-order",
                agent_id, provider
            ),
            &self.gateway_token,
        ) {
            Ok(value) => {
                let profile_ids = value
                    .get("profile_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                self.auth_order_profile_ids_csv = profile_ids;
                self.set_info("Loaded profile order");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn save_auth_order(&mut self) {
        let agent_id = self.auth_order_agent_id.trim();
        let provider = self.auth_order_provider.trim().to_ascii_lowercase();
        if agent_id.is_empty() || provider.is_empty() {
            self.set_error("agent_id and provider are required");
            return;
        }
        let profile_ids = parse_string_csv(&self.auth_order_profile_ids_csv);
        let payload = json!({ "profile_ids": profile_ids });
        match send_json(
            &self.gateway_base_url,
            &format!(
                "/api/v1/auth/agents/{}/providers/{}/profile-order",
                agent_id, provider
            ),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                self.refresh_gateway_state();
                self.set_info("Saved profile order");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn save_channel_config(&mut self) {
        let discord_allowlisted_user_ids =
            parse_string_csv(&self.channel_config.discord_allowlisted_user_ids_csv);
        let telegram_allowlisted_user_ids =
            match parse_i64_csv(&self.channel_config.telegram_allowlisted_user_ids_csv) {
                Ok(values) => values,
                Err(err) => {
                    self.set_error(err);
                    return;
                }
            };

        let payload = json!({
            "discord": {
                "require_mention_in_guild_channels": self.channel_config.discord_require_mention_in_guild_channels,
                "allowlisted_user_ids": discord_allowlisted_user_ids
            },
            "telegram": {
                "require_mention_in_groups": self.channel_config.telegram_require_mention_in_groups,
                "allowlisted_user_ids": telegram_allowlisted_user_ids
            }
        });
        match send_json(
            &self.gateway_base_url,
            "/api/v1/config/channels",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(value) => match parse_channel_config(&value) {
                Ok(config) => {
                    self.channel_config = config;
                    self.set_info("Channel configuration saved");
                }
                Err(err) => self.set_error(err),
            },
            Err(err) => self.set_error(err),
        }
    }

    fn render_mission(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            card(&mut columns[0], "Health", |ui| {
                if let Some(health) = &self.health {
                    ui.horizontal_wrapped(|ui| {
                        ui.colored_label(
                            if health.ok {
                                Color32::from_rgb(118, 255, 168)
                            } else {
                                Color32::from_rgb(255, 107, 107)
                            },
                            if health.ok {
                                "Gateway healthy"
                            } else {
                                "Gateway unhealthy"
                            },
                        );
                        ui.label(format!("service: {}", health.service));
                    });
                    ui.label(format!("version: {}", health.version));
                    ui.label(format!("uptime: {} ms", health.uptime_ms));
                    ui.label(format!("now: {}", health.now_utc));
                } else {
                    ui.label("No health snapshot loaded");
                }
            });

            card(&mut columns[1], "Status", |ui| {
                if let Some(status) = &self.status {
                    ui.label(format!("service: {}", status.service));
                    ui.label(format!("version: {}", status.version));
                    ui.label(format!("started_at_utc: {}", status.started_at_utc));
                    ui.label(format!("uptime: {} ms", status.uptime_ms));
                    ui.label(format!("db_path: {}", status.db_path));
                    ui.label(format!("attachments_path: {}", status.attachments_path));
                } else {
                    ui.label("No status snapshot loaded");
                }
            });
        });

        card(ui, "Control Pulse", |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("sessions: {}", self.sessions.len()));
                ui.separator();
                ui.label(format!("approvals: {}", self.approvals.len()));
                ui.separator();
                ui.label(format!("auth profiles: {}", self.auth_profiles.len()));
                ui.separator();
                ui.label(format!("timeline msgs: {}", self.timeline.len()));
            });
        });
    }

    fn render_sessions(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            card(&mut columns[0], "Sessions", |ui| {
                ui.horizontal(|ui| {
                    ui.label("New session title");
                    ui.text_edit_singleline(&mut self.new_session_title);
                    if ui.button("Create").clicked() {
                        self.create_session();
                    }
                });
                ui.separator();

                let mut select_session: Option<String> = None;
                egui::ScrollArea::vertical()
                    .max_height(440.0)
                    .show(ui, |ui| {
                        if self.sessions.is_empty() {
                            ui.label("No sessions available");
                        }
                        for session in &self.sessions {
                            let is_selected = self
                                .selected_session_id
                                .as_ref()
                                .map(|id| id == &session.session_id)
                                .unwrap_or(false);
                            let title = session
                                .title
                                .clone()
                                .unwrap_or_else(|| "untitled".to_string());
                            let label = format!(
                                "{}\n{} | msg {} | run {}",
                                title, session.session_id, session.message_count, session.run_count
                            );
                            if ui.selectable_label(is_selected, label).clicked() {
                                select_session = Some(session.session_id.clone());
                            }
                            ui.small(format!("updated_at_ms: {}", session.updated_at));
                            ui.separator();
                        }
                    });

                if let Some(session_id) = select_session {
                    self.selected_session_id = Some(session_id);
                    let _ = self.load_timeline_for_selected();
                }
            });

            card(&mut columns[1], "Conversation", |ui| {
                ui.horizontal(|ui| {
                    if let Some(selected_session_id) = &self.selected_session_id {
                        ui.label(format!("selected: {}", selected_session_id));
                    } else {
                        ui.colored_label(Color32::from_rgb(255, 188, 104), "No session selected");
                    }
                    if ui.button("Reload Timeline").clicked() {
                        let _ = self.load_timeline_for_selected();
                    }
                });

                egui::ScrollArea::vertical()
                    .max_height(310.0)
                    .show(ui, |ui| {
                        if self.timeline.is_empty() {
                            ui.label("No timeline messages");
                        }
                        for message in &self.timeline {
                            let role_color = role_color(&message.role);
                            egui::Frame::group(ui.style())
                                .fill(Color32::from_rgb(20, 26, 34))
                                .stroke(egui::Stroke::new(1.0, role_color))
                                .show(ui, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.colored_label(
                                            role_color,
                                            RichText::new(message.role.to_uppercase()).strong(),
                                        );
                                        ui.label(format_timestamp_ms(message.created_at));
                                    });
                                    ui.separator();
                                    if matches!(
                                        message.role.as_str(),
                                        "assistant" | "tool" | "system"
                                    ) {
                                        render_markdown_baseline(ui, &message.content_text);
                                    } else {
                                        ui.label(&message.content_text);
                                    }
                                });
                            ui.add_space(6.0);
                        }
                    });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Role");
                    egui::ComboBox::from_id_salt("composer-role")
                        .selected_text(self.composer_role.clone())
                        .show_ui(ui, |ui| {
                            for role in ["user", "system", "tool"] {
                                ui.selectable_value(
                                    &mut self.composer_role,
                                    role.to_string(),
                                    role,
                                );
                            }
                        });
                });
                ui.add(
                    egui::TextEdit::multiline(&mut self.composer_content_text)
                        .desired_rows(4)
                        .hint_text("Write a prompt or control message..."),
                );
                ui.horizontal(|ui| {
                    if ui.button("Send Message").clicked() {
                        self.send_message_to_selected();
                    }
                });

                ui.separator();
                ui.heading("Run Controls");
                ui.horizontal(|ui| {
                    ui.label("provider");
                    ui.text_edit_singleline(&mut self.run_model_provider);
                    ui.label("model");
                    ui.text_edit_singleline(&mut self.run_model_id);
                });
                ui.horizontal(|ui| {
                    ui.label("auth_profile_id (optional)");
                    ui.text_edit_singleline(&mut self.run_auth_profile_id);
                    if ui.button("Create Run").clicked() {
                        self.create_run_for_selected();
                    }
                });
            });
        });
    }

    fn render_approvals(&mut self, ui: &mut egui::Ui) {
        card(ui, "Pending Approvals", |ui| {
            if self.approvals.is_empty() {
                ui.label("No pending approvals");
                return;
            }

            let mut requested_action: Option<(String, &'static str)> = None;
            egui::ScrollArea::vertical()
                .max_height(560.0)
                .show(ui, |ui| {
                    for approval in &self.approvals {
                        egui::Frame::group(ui.style())
                            .fill(Color32::from_rgb(31, 24, 28))
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(255, 188, 104)))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({})",
                                        approval.kind, approval.status
                                    ))
                                    .strong(),
                                );
                                ui.label(format!("approval_id: {}", approval.approval_id));
                                ui.label(format!("run_id: {}", approval.run_id));
                                ui.label(format!("requested_at_ms: {}", approval.requested_at));
                                ui.label(format!("summary: {}", approval.request_summary));
                                ui.horizontal(|ui| {
                                    if ui.button("Approve").clicked() {
                                        requested_action =
                                            Some((approval.approval_id.clone(), "approve"));
                                    }
                                    if ui.button("Deny").clicked() {
                                        requested_action =
                                            Some((approval.approval_id.clone(), "deny"));
                                    }
                                });
                            });
                        ui.add_space(8.0);
                    }
                });

            if let Some((approval_id, decision)) = requested_action {
                self.resolve_approval(&approval_id, decision);
            }
        });
    }

    fn render_auth(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            card(&mut columns[0], "Auth Profiles", |ui| {
                let mut toggle_action: Option<(String, bool, String)> = None;
                egui::ScrollArea::vertical()
                    .max_height(390.0)
                    .show(ui, |ui| {
                        if self.auth_profiles.is_empty() {
                            ui.label("No auth profiles configured");
                        }
                        for profile in &self.auth_profiles {
                            egui::Frame::group(ui.style())
                                .fill(Color32::from_rgb(21, 24, 34))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if profile.enabled {
                                        Color32::from_rgb(118, 255, 168)
                                    } else {
                                        Color32::from_rgb(255, 107, 107)
                                    },
                                ))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} ({})",
                                            profile.display_name, profile.provider
                                        ))
                                        .strong(),
                                    );
                                    ui.label(format!(
                                        "mode={} risk={} scope={}",
                                        profile.auth_mode,
                                        profile.risk_level,
                                        profile.kill_switch_scope
                                    ));
                                    if let Some(api_base_url) = &profile.api_base_url {
                                        ui.label(format!("base_url={}", api_base_url));
                                    }
                                    ui.label(format!("updated_at_ms={}", profile.updated_at));
                                    if ui
                                        .button(if profile.enabled {
                                            "Disable"
                                        } else {
                                            "Enable"
                                        })
                                        .clicked()
                                    {
                                        toggle_action = Some((
                                            profile.auth_profile_id.clone(),
                                            !profile.enabled,
                                            profile.kill_switch_scope.clone(),
                                        ));
                                    }
                                });
                            ui.add_space(8.0);
                        }
                    });

                if let Some((auth_profile_id, enabled, kill_switch_scope)) = toggle_action {
                    self.set_auth_profile_enabled(&auth_profile_id, enabled, &kill_switch_scope);
                }

                ui.separator();
                ui.heading("Provider Order");
                ui.horizontal(|ui| {
                    ui.label("agent_id");
                    ui.text_edit_singleline(&mut self.auth_order_agent_id);
                });
                ui.horizontal(|ui| {
                    ui.label("provider");
                    ui.text_edit_singleline(&mut self.auth_order_provider);
                });
                ui.label("profile_ids csv");
                ui.text_edit_singleline(&mut self.auth_order_profile_ids_csv);
                ui.horizontal(|ui| {
                    if ui.button("Load").clicked() {
                        self.load_auth_order();
                    }
                    if ui.button("Save").clicked() {
                        self.save_auth_order();
                    }
                });
            });

            egui::ScrollArea::vertical().show(&mut columns[1], |ui| {
                card(ui, "OpenAI OAuth (PKCE)", |ui| {
                    ui.label("Use start -> open authorize URL -> paste callback URL (or manual code/state) -> finish.");
                    ui.horizontal(|ui| {
                        ui.label("display_name");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.display_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("client_id");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.client_id);
                    });
                    ui.horizontal(|ui| {
                        ui.label("scope");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.scope);
                    });
                    ui.horizontal(|ui| {
                        ui.label("authorize_url");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.authorize_url);
                    });
                    ui.horizontal(|ui| {
                        ui.label("token_url");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.token_url);
                    });
                    ui.horizontal(|ui| {
                        ui.label("api_base_url");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.api_base_url);
                    });
                    if ui.button("Start OpenAI OAuth").clicked() {
                        self.start_openai_oauth();
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("oauth_session_id");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.oauth_session_id);
                    });
                    if !self.openai_oauth_draft.authorize_url_result.trim().is_empty() {
                        ui.hyperlink_to(
                            "Open authorize URL in browser",
                            self.openai_oauth_draft.authorize_url_result.clone(),
                        );
                        ui.add(
                            egui::TextEdit::multiline(
                                &mut self.openai_oauth_draft.authorize_url_result,
                            )
                            .desired_rows(2),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("callback_url");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.callback_url);
                    });
                    ui.label("manual fallback (when callback capture is unavailable)");
                    ui.horizontal(|ui| {
                        ui.label("code");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.manual_code);
                    });
                    ui.horizontal(|ui| {
                        ui.label("state");
                        ui.text_edit_singleline(&mut self.openai_oauth_draft.manual_state);
                    });
                    if ui.button("Finish OpenAI OAuth").clicked() {
                        self.finish_openai_oauth();
                    }
                });

                card(ui, "Anthropic Setup Token", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("display_name");
                        ui.text_edit_singleline(&mut self.anthropic_setup_draft.display_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("setup_token");
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut self.anthropic_setup_draft.setup_token,
                            )
                            .password(true),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("api_base_url");
                        ui.text_edit_singleline(&mut self.anthropic_setup_draft.api_base_url);
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.anthropic_setup_draft.enabled, "enabled");
                        ui.label("kill_switch_scope");
                        ui.text_edit_singleline(&mut self.anthropic_setup_draft.kill_switch_scope);
                    });
                    if ui.button("Ingest Setup Token").clicked() {
                        self.ingest_anthropic_setup_token();
                    }
                });

                card(ui, "Create Auth Profile (Manual)", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("provider");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.provider);
                    });
                    ui.horizontal(|ui| {
                        ui.label("display_name");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.display_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("auth_mode");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.auth_mode);
                    });
                    ui.horizontal(|ui| {
                        ui.label("risk_level");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.risk_level);
                    });
                    ui.horizontal(|ui| {
                        ui.label("kill_switch_scope");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.kill_switch_scope);
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.auth_profile_draft.enabled, "enabled");
                    });
                    ui.horizontal(|ui| {
                        ui.label("api_base_url");
                        ui.text_edit_singleline(&mut self.auth_profile_draft.api_base_url);
                    });
                    ui.label("credentials_json");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.auth_profile_draft.credentials_json_text)
                            .desired_rows(8),
                    );
                    if ui.button("Create Profile").clicked() {
                        self.create_auth_profile();
                    }
                });
            });
        });
    }

    fn render_channels(&mut self, ui: &mut egui::Ui) {
        card(ui, "Channel Configuration", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut self
                        .channel_config
                        .discord_require_mention_in_guild_channels,
                    "Discord requires mention in guild channels",
                );
            });
            ui.label("Discord allowlisted user IDs (csv)");
            ui.text_edit_singleline(&mut self.channel_config.discord_allowlisted_user_ids_csv);

            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut self.channel_config.telegram_require_mention_in_groups,
                    "Telegram requires mention in groups",
                );
            });
            ui.label("Telegram allowlisted user IDs (csv i64)");
            ui.text_edit_singleline(&mut self.channel_config.telegram_allowlisted_user_ids_csv);

            ui.separator();
            ui.label(format!("updated_at_ms: {}", self.channel_config.updated_at));

            if ui.button("Save Channel Config").clicked() {
                self.save_channel_config();
            }
        });
    }
}

fn apply_frontend_design_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(Color32::from_rgb(230, 232, 240));
    visuals.panel_fill = Color32::from_rgb(14, 18, 26);
    visuals.window_fill = Color32::from_rgb(20, 24, 34);
    visuals.extreme_bg_color = Color32::from_rgb(8, 12, 18);
    visuals.faint_bg_color = Color32::from_rgb(30, 38, 54);
    visuals.hyperlink_color = Color32::from_rgb(108, 208, 255);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(28, 34, 48);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(38, 46, 64);
    visuals.widgets.active.bg_fill = Color32::from_rgb(52, 63, 86);
    visuals.widgets.open.bg_fill = Color32::from_rgb(46, 56, 74);
    visuals.selection.bg_fill = Color32::from_rgb(255, 214, 109);
    visuals.selection.stroke = egui::Stroke::new(1.0, Color32::from_rgb(18, 22, 32));
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.indent = 16.0;
    ctx.set_style(style);
}

fn card(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(18, 22, 32))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(255, 214, 109)))
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .size(18.0)
                    .strong()
                    .color(Color32::from_rgb(255, 214, 109)),
            );
            ui.separator();
            add_contents(ui);
        });
    ui.add_space(10.0);
}

fn role_color(role: &str) -> Color32 {
    match role {
        "user" => Color32::from_rgb(164, 212, 255),
        "assistant" => Color32::from_rgb(118, 255, 168),
        "tool" => Color32::from_rgb(255, 188, 104),
        "system" => Color32::from_rgb(255, 146, 146),
        _ => Color32::from_rgb(196, 196, 196),
    }
}

fn format_timestamp_ms(timestamp_ms: i64) -> String {
    format!("created_at_ms={}", timestamp_ms)
}

fn render_markdown_baseline(ui: &mut egui::Ui, text: &str) {
    let mut in_code_block = false;
    for raw_line in text.lines() {
        let line = raw_line.trim_end();
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            ui.label(
                RichText::new(line)
                    .monospace()
                    .color(Color32::from_rgb(255, 214, 109)),
            );
            continue;
        }

        if let Some(rest) = line.strip_prefix("### ") {
            ui.label(RichText::new(rest).size(19.0).strong());
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            ui.label(RichText::new(rest).size(21.0).strong());
            continue;
        }
        if let Some(rest) = line.strip_prefix("# ") {
            ui.label(RichText::new(rest).size(24.0).strong());
            continue;
        }
        if let Some(rest) = line.strip_prefix("- ") {
            ui.label(format!("• {}", rest));
            continue;
        }
        if let Some(rest) = line.strip_prefix("* ") {
            ui.label(format!("• {}", rest));
            continue;
        }
        if let Some(rest) = line.strip_prefix("> ") {
            ui.label(
                RichText::new(rest)
                    .italics()
                    .color(Color32::from_rgb(164, 212, 255)),
            );
            continue;
        }

        if line.trim().is_empty() {
            ui.add_space(4.0);
        } else {
            ui.label(line);
        }
    }
}

fn auto_launch_gateway_enabled() -> bool {
    std::env::var("CARSINOS_GUI_AUTO_LAUNCH_GATEWAY")
        .ok()
        .and_then(|value| parse_boolish(&value))
        .unwrap_or(true)
}

fn parse_boolish(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn resolve_gateway_binary_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CARSINOS_GATEWAY_BIN") {
        let candidate = PathBuf::from(explicit.trim());
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let current = std::env::current_exe().ok()?;
    let parent = current.parent()?;
    let candidates = [
        parent.join("carsinos-gateway"),
        parent.join("../carsinos-gateway"),
        parent.join("../../carsinos-gateway"),
    ];
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn fetch_gateway_snapshots(base_url: &str, token: &str) -> Result<GatewaySnapshots, String> {
    let health_json = fetch_json(base_url, "/api/v1/health", token)?;
    let status_json = fetch_json(base_url, "/api/v1/status", token)?;
    let sessions_json = fetch_json(base_url, "/api/v1/sessions?limit=100", token)?;
    let approvals_json = fetch_json(
        base_url,
        "/api/v1/approvals?status=requested&limit=100",
        token,
    )?;
    let auth_profiles_json = fetch_json(
        base_url,
        "/api/v1/auth/profiles?include_disabled=true&limit=200",
        token,
    )?;
    let channel_config_json = fetch_json(base_url, "/api/v1/config/channels", token)?;

    Ok((
        parse_health_snapshot(&health_json)?,
        parse_status_snapshot(&status_json)?,
        parse_sessions(&sessions_json)?,
        parse_approvals(&approvals_json)?,
        parse_auth_profiles(&auth_profiles_json)?,
        parse_channel_config(&channel_config_json)?,
    ))
}

fn fetch_json(base_url: &str, path: &str, token: &str) -> Result<Value, String> {
    send_json(base_url, path, "GET", token, None)
}

fn send_json(
    base_url: &str,
    path: &str,
    method: &str,
    token: &str,
    payload: Option<&Value>,
) -> Result<Value, String> {
    let url = format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    let mut request = agent.request(method, &url);
    request = request.set("Accept", "application/json");
    if !token.trim().is_empty() {
        request = request.set("Authorization", &format!("Bearer {}", token.trim()));
    }

    let response = match payload {
        Some(payload) => request.send_json(payload.clone()),
        None => request.call(),
    };

    match response {
        Ok(response) => response
            .into_json::<Value>()
            .map_err(|err| format!("invalid json for {} {}: {}", method, path, err)),
        Err(ureq::Error::Status(status, response)) => {
            let body = response
                .into_string()
                .unwrap_or_else(|_| "<unreadable error body>".to_string());
            Err(format!(
                "request {} {} failed with status {}: {}",
                method, path, status, body
            ))
        }
        Err(ureq::Error::Transport(err)) => Err(format!(
            "request {} {} transport error: {}",
            method, path, err
        )),
    }
}

fn parse_health_snapshot(value: &Value) -> Result<HealthSnapshot, String> {
    Ok(HealthSnapshot {
        ok: value
            .get("ok")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| "health.ok missing".to_string())?,
        service: value
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "health.service missing".to_string())?
            .to_string(),
        version: value
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "health.version missing".to_string())?
            .to_string(),
        uptime_ms: value
            .get("uptime_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "health.uptime_ms missing".to_string())?,
        now_utc: value
            .get("now_utc")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "health.now_utc missing".to_string())?
            .to_string(),
    })
}

fn parse_status_snapshot(value: &Value) -> Result<StatusSnapshot, String> {
    Ok(StatusSnapshot {
        service: value
            .get("service")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "status.service missing".to_string())?
            .to_string(),
        version: value
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "status.version missing".to_string())?
            .to_string(),
        started_at_utc: value
            .get("started_at_utc")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "status.started_at_utc missing".to_string())?
            .to_string(),
        uptime_ms: value
            .get("uptime_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "status.uptime_ms missing".to_string())?,
        db_path: value
            .get("db_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "status.db_path missing".to_string())?
            .to_string(),
        attachments_path: value
            .get("attachments_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "status.attachments_path missing".to_string())?
            .to_string(),
    })
}

fn parse_sessions(value: &Value) -> Result<Vec<SessionListItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "sessions.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(SessionListItem {
            session_id: item
                .get("session_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "sessions.session_id missing".to_string())?
                .to_string(),
            title: item
                .get("title")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            message_count: item
                .get("message_count")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "sessions.message_count missing".to_string())?,
            run_count: item
                .get("run_count")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "sessions.run_count missing".to_string())?,
            updated_at: item
                .get("updated_at")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "sessions.updated_at missing".to_string())?,
        });
    }
    Ok(out)
}

fn parse_approvals(value: &Value) -> Result<Vec<ApprovalListItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "approvals.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(ApprovalListItem {
            approval_id: item
                .get("approval_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "approvals.approval_id missing".to_string())?
                .to_string(),
            run_id: item
                .get("run_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "approvals.run_id missing".to_string())?
                .to_string(),
            kind: item
                .get("kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "approvals.kind missing".to_string())?
                .to_string(),
            status: item
                .get("status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "approvals.status missing".to_string())?
                .to_string(),
            request_summary: item
                .get("request_summary")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "approvals.request_summary missing".to_string())?
                .to_string(),
            requested_at: item
                .get("requested_at")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "approvals.requested_at missing".to_string())?,
        });
    }
    Ok(out)
}

fn parse_auth_profiles(value: &Value) -> Result<Vec<AuthProfileListItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "auth_profiles.items missing".to_string())?;
    let mut out = Vec::new();
    for item in items {
        out.push(AuthProfileListItem {
            auth_profile_id: item
                .get("auth_profile_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.auth_profile_id missing".to_string())?
                .to_string(),
            provider: item
                .get("provider")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.provider missing".to_string())?
                .to_string(),
            display_name: item
                .get("display_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.display_name missing".to_string())?
                .to_string(),
            auth_mode: item
                .get("auth_mode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.auth_mode missing".to_string())?
                .to_string(),
            risk_level: item
                .get("risk_level")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.risk_level missing".to_string())?
                .to_string(),
            enabled: item
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "auth_profile.enabled missing".to_string())?,
            kill_switch_scope: item
                .get("kill_switch_scope")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "auth_profile.kill_switch_scope missing".to_string())?
                .to_string(),
            api_base_url: item
                .get("api_base_url")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            updated_at: item
                .get("updated_at")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "auth_profile.updated_at missing".to_string())?,
        });
    }
    Ok(out)
}

fn parse_channel_config(value: &Value) -> Result<ChannelConfigSnapshot, String> {
    let config = value
        .get("config")
        .ok_or_else(|| "channels.config missing".to_string())?;
    let discord = config
        .get("discord")
        .ok_or_else(|| "channels.config.discord missing".to_string())?;
    let telegram = config
        .get("telegram")
        .ok_or_else(|| "channels.config.telegram missing".to_string())?;

    let discord_allowlisted_user_ids = discord
        .get("allowlisted_user_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "channels.discord.allowlisted_user_ids missing".to_string())?
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .ok_or_else(|| {
                    "channels.discord.allowlisted_user_ids item must be string".to_string()
                })
                .map(|value| value.to_string())
        })
        .collect::<Result<Vec<String>, String>>()?;

    let telegram_allowlisted_user_ids = telegram
        .get("allowlisted_user_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "channels.telegram.allowlisted_user_ids missing".to_string())?
        .iter()
        .map(|entry| {
            entry.as_i64().ok_or_else(|| {
                "channels.telegram.allowlisted_user_ids item must be i64".to_string()
            })
        })
        .collect::<Result<Vec<i64>, String>>()?;

    Ok(ChannelConfigSnapshot {
        discord_require_mention_in_guild_channels: discord
            .get("require_mention_in_guild_channels")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| {
                "channels.discord.require_mention_in_guild_channels missing".to_string()
            })?,
        discord_allowlisted_user_ids_csv: join_string_csv(&discord_allowlisted_user_ids),
        telegram_require_mention_in_groups: telegram
            .get("require_mention_in_groups")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| "channels.telegram.require_mention_in_groups missing".to_string())?,
        telegram_allowlisted_user_ids_csv: join_i64_csv(&telegram_allowlisted_user_ids),
        updated_at: config
            .get("updated_at")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| "channels.config.updated_at missing".to_string())?,
    })
}

fn fetch_session_timeline(
    base_url: &str,
    token: &str,
    session_id: &str,
) -> Result<Vec<TimelineMessage>, String> {
    let endpoint = format!("/api/v1/sessions/{}/messages?limit=200", session_id);
    let json = fetch_json(base_url, &endpoint, token)?;
    parse_timeline(&json)
}

fn parse_timeline(value: &Value) -> Result<Vec<TimelineMessage>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "timeline.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(TimelineMessage {
            role: item
                .get("role")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "timeline.role missing".to_string())?
                .to_string(),
            content_text: item
                .get("content_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "timeline.content_text missing".to_string())?
                .to_string(),
            created_at: item
                .get("created_at")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "timeline.created_at missing".to_string())?,
        });
    }
    Ok(out)
}

fn parse_string_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

fn parse_i64_csv(raw: &str) -> Result<Vec<i64>, String> {
    let mut out = Vec::new();
    for entry in raw
        .split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let value = entry
            .parse::<i64>()
            .map_err(|_| format!("invalid i64 value in csv: {}", entry))?;
        out.push(value);
    }
    Ok(out)
}

fn join_string_csv(values: &[String]) -> String {
    values.join(",")
}

fn join_i64_csv(values: &[i64]) -> String {
    values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "carsinOS GUI",
        options,
        Box::new(|_cc| Ok(Box::new(GuiApp::default()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_health_snapshot_success() {
        let value = serde_json::json!({
            "ok": true,
            "service": "carsinos-gateway",
            "version": "0.1.0",
            "uptime_ms": 1234,
            "now_utc": "2026-01-01T00:00:00Z"
        });
        let parsed = parse_health_snapshot(&value).expect("health parse");
        assert!(parsed.ok);
        assert_eq!(parsed.service, "carsinos-gateway");
        assert_eq!(parsed.uptime_ms, 1234);
    }

    #[test]
    fn parse_status_snapshot_requires_fields() {
        let value = serde_json::json!({
            "service": "carsinos-gateway"
        });
        let error = parse_status_snapshot(&value).expect_err("status parse should fail");
        assert!(error.contains("status.version missing"));
    }

    #[test]
    fn parse_sessions_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "session_id": "s1",
                    "title": "demo",
                    "message_count": 2,
                    "run_count": 1,
                    "updated_at": 100
                }
            ]
        });
        let sessions = parse_sessions(&value).expect("sessions parse");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }

    #[test]
    fn parse_approvals_missing_items_fails() {
        let value = serde_json::json!({});
        let error = parse_approvals(&value).expect_err("approvals parse should fail");
        assert!(error.contains("approvals.items missing"));
    }

    #[test]
    fn parse_auth_profiles_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "auth_profile_id": "p1",
                    "provider": "openai",
                    "display_name": "primary",
                    "auth_mode": "api_key",
                    "risk_level": "low",
                    "enabled": true,
                    "kill_switch_scope": "none",
                    "api_base_url": "https://api.openai.com",
                    "updated_at": 20
                }
            ]
        });
        let profiles = parse_auth_profiles(&value).expect("auth profiles parse");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].auth_profile_id, "p1");
        assert!(profiles[0].enabled);
    }

    #[test]
    fn parse_channel_config_success() {
        let value = serde_json::json!({
            "config": {
                "discord": {
                    "require_mention_in_guild_channels": false,
                    "allowlisted_user_ids": ["u1", "u2"]
                },
                "telegram": {
                    "require_mention_in_groups": true,
                    "allowlisted_user_ids": [1001, 1002]
                },
                "updated_at": 9
            }
        });
        let parsed = parse_channel_config(&value).expect("channel config parse");
        assert!(!parsed.discord_require_mention_in_guild_channels);
        assert_eq!(parsed.discord_allowlisted_user_ids_csv, "u1,u2");
        assert_eq!(parsed.telegram_allowlisted_user_ids_csv, "1001,1002");
        assert_eq!(parsed.updated_at, 9);
    }

    #[test]
    fn parse_i64_csv_rejects_invalid_values() {
        let error = parse_i64_csv("1,abc,3").expect_err("expected parse failure");
        assert!(error.contains("invalid i64 value"));
    }

    #[test]
    fn parse_boolish_supports_common_values() {
        assert_eq!(parse_boolish("true"), Some(true));
        assert_eq!(parse_boolish("ON"), Some(true));
        assert_eq!(parse_boolish("0"), Some(false));
        assert_eq!(parse_boolish("No"), Some(false));
        assert_eq!(parse_boolish("maybe"), None);
    }

    #[test]
    fn parse_timeline_success() {
        let value = serde_json::json!({
            "items": [
                {"role":"user","content_text":"hello","created_at":1},
                {"role":"assistant","content_text":"hi","created_at":2}
            ]
        });
        let timeline = parse_timeline(&value).expect("timeline parse");
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0].role, "user");
        assert_eq!(timeline[1].role, "assistant");
    }
}
