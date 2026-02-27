#![allow(dead_code)]

use eframe::egui;
use eframe::egui::{Color32, RichText};
use rand::rngs::OsRng;
use rand::RngCore;
use serde_json::{json, Value};
use std::collections::HashSet;
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
struct TeamAgentItem {
    agent_id: String,
    name: String,
    model_provider: String,
    model_id: String,
    tool_profile: String,
}

#[derive(Debug, Clone)]
struct BoardSummaryItem {
    board_id: String,
    board_key: String,
    name: String,
}

#[derive(Debug, Clone)]
struct BoardColumnItem {
    column_id: String,
    column_key: String,
    name: String,
    position: i64,
}

#[derive(Debug, Clone)]
struct BoardAssetItem {
    card_asset_id: String,
    filename: String,
    mime: String,
    bytes: i64,
}

#[derive(Debug, Clone)]
struct BoardCardItem {
    card_id: String,
    column_id: String,
    title: String,
    description: Option<String>,
    owner_kind: String,
    owner_agent_id: Option<String>,
    script_markdown: Option<String>,
    linked_session_id: Option<String>,
    latest_run_id: Option<String>,
    position: i64,
    assets: Vec<BoardAssetItem>,
}

#[derive(Debug, Clone, Default)]
struct BoardDetailItem {
    board_id: String,
    board_key: String,
    board_name: String,
    columns: Vec<BoardColumnItem>,
    cards: Vec<BoardCardItem>,
}

#[derive(Debug, Clone)]
struct CalendarJobItem {
    job_id: String,
    name: String,
    agent_id: String,
    enabled: bool,
    next_run_at: Option<i64>,
    schedule_kind: String,
}

#[derive(Debug, Clone)]
struct MemoryNoteItem {
    note_id: String,
    title: Option<String>,
    updated_at: i64,
    body_preview: String,
}

#[derive(Debug, Clone)]
struct BoardAutomationRuleItem {
    rule_id: String,
    job_id: String,
    board_id: String,
    column_id: String,
    target_column_id: String,
    name: String,
    enabled: bool,
    next_run_at: Option<i64>,
    max_cards_per_run: i64,
    max_runs_per_day: i64,
    max_attempts_per_card_per_day: i64,
    last_error: Option<String>,
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

#[derive(Debug, Clone)]
struct RuntimeProviderPolicyDraft {
    provider: String,
    enabled: bool,
    allow_consumer_oauth: bool,
    kill_switch_scope: String,
}

#[derive(Debug, Clone)]
struct RuntimeConfigWizardSnapshot {
    schema_version: String,
    updated_at: i64,
    jwt_issuer_allowlist_csv: String,
    jwt_audience_allowlist_csv: String,
    trusted_proxy_allowlist_csv: String,
    tls_termination_mode: String,
    public_base_url: String,
    provider_policies: Vec<RuntimeProviderPolicyDraft>,
    discord_operation_mode: String,
    discord_bot_token_secret_ref: String,
    discord_application_id: String,
    discord_intents_csv: String,
    discord_staging_guild_ids_csv: String,
    discord_staging_channel_ids_csv: String,
    telegram_operation_mode: String,
    telegram_bot_token_secret_ref: String,
    telegram_webhook_mode: String,
    telegram_webhook_url: String,
    telegram_staging_chat_ids_csv: String,
    threat_model_approver: String,
    risk_acceptance_owner: String,
    incident_primary: String,
    incident_backup: String,
    audit_archive_target: String,
    audit_archive_encryption: String,
    audit_hot_retention_days: String,
    audit_archive_retention_days: String,
}

impl Default for RuntimeConfigWizardSnapshot {
    fn default() -> Self {
        Self {
            schema_version: "runtime.config.v1".to_string(),
            updated_at: 0,
            jwt_issuer_allowlist_csv: String::new(),
            jwt_audience_allowlist_csv: String::new(),
            trusted_proxy_allowlist_csv: String::new(),
            tls_termination_mode: "edge".to_string(),
            public_base_url: String::new(),
            provider_policies: vec![
                RuntimeProviderPolicyDraft {
                    provider: "openai".to_string(),
                    enabled: true,
                    allow_consumer_oauth: false,
                    kill_switch_scope: "none".to_string(),
                },
                RuntimeProviderPolicyDraft {
                    provider: "anthropic".to_string(),
                    enabled: true,
                    allow_consumer_oauth: false,
                    kill_switch_scope: "none".to_string(),
                },
            ],
            discord_operation_mode: "shim".to_string(),
            discord_bot_token_secret_ref: String::new(),
            discord_application_id: String::new(),
            discord_intents_csv: "guilds,guild_messages,direct_messages".to_string(),
            discord_staging_guild_ids_csv: String::new(),
            discord_staging_channel_ids_csv: String::new(),
            telegram_operation_mode: "shim".to_string(),
            telegram_bot_token_secret_ref: String::new(),
            telegram_webhook_mode: "long_poll".to_string(),
            telegram_webhook_url: String::new(),
            telegram_staging_chat_ids_csv: String::new(),
            threat_model_approver: String::new(),
            risk_acceptance_owner: String::new(),
            incident_primary: String::new(),
            incident_backup: String::new(),
            audit_archive_target: String::new(),
            audit_archive_encryption: String::new(),
            audit_hot_retention_days: "90".to_string(),
            audit_archive_retention_days: "365".to_string(),
        }
    }
}

const RUNTIME_SCHEMA_VERSION_V1: &str = "runtime.config.v1";
const TLS_TERMINATION_MODES: [&str; 3] = ["edge", "gateway", "passthrough"];
const TELEGRAM_WEBHOOK_MODES: [&str; 2] = ["long_poll", "webhook"];
const CHANNEL_OPERATION_MODES: [&str; 2] = ["shim", "transport"];
const KILL_SWITCH_SCOPES: [&str; 4] = ["none", "profile", "provider", "global"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeWizardStep {
    EdgeIdentity,
    ProviderRisk,
    Channels,
    SecurityOps,
    ReviewApply,
}

impl RuntimeWizardStep {
    fn all() -> [Self; 5] {
        [
            Self::EdgeIdentity,
            Self::ProviderRisk,
            Self::Channels,
            Self::SecurityOps,
            Self::ReviewApply,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::EdgeIdentity => "1. Edge Identity",
            Self::ProviderRisk => "2. Provider Risk",
            Self::Channels => "3. Channels",
            Self::SecurityOps => "4. Security Ops",
            Self::ReviewApply => "5. Review + Apply",
        }
    }

    fn guidance(self) -> &'static str {
        match self {
            Self::EdgeIdentity => {
                "Configure gateway trust boundaries and edge identity values (R1)."
            }
            Self::ProviderRisk => {
                "Define per-provider enablement, kill-switch scope, and high-risk OAuth posture (R7)."
            }
            Self::Channels => {
                "Set production channel runtime values for Discord + Telegram (R2, R3)."
            }
            Self::SecurityOps => {
                "Set operational ownership and security retention/archive targets (R4, R5, R6)."
            }
            Self::ReviewApply => {
                "Validate readiness, enforce high-risk locks, then apply or rollback safely."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Tasks,
    Content,
    Calendar,
    Memory,
    Team,
    Approvals,
    Settings,
}

impl MainTab {
    fn label(self) -> &'static str {
        match self {
            MainTab::Tasks => "Tasks",
            MainTab::Content => "Content",
            MainTab::Calendar => "Calendar",
            MainTab::Memory => "Memory",
            MainTab::Team => "Team",
            MainTab::Approvals => "Approvals",
            MainTab::Settings => "Settings",
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
    team_agents: Vec<TeamAgentItem>,
    boards: Vec<BoardSummaryItem>,
    tasks_board: BoardDetailItem,
    content_board: BoardDetailItem,
    content_automation_rules: Vec<BoardAutomationRuleItem>,
    calendar_jobs: Vec<CalendarJobItem>,
    memory_notes: Vec<MemoryNoteItem>,
    channel_config: ChannelConfigSnapshot,
    runtime_config: RuntimeConfigWizardSnapshot,
    runtime_wizard_step: RuntimeWizardStep,
    runtime_wizard_rollback_reason: String,
    discord_bot_token_plaintext: String,
    telegram_bot_token_plaintext: String,

    selected_session_id: Option<String>,
    timeline: Vec<TimelineMessage>,

    new_session_title: String,
    composer_role: String,
    composer_content_text: String,
    run_model_provider: String,
    run_model_id: String,
    run_auth_profile_id: String,
    new_task_title: String,
    new_task_owner_agent_id: String,
    new_content_title: String,
    new_content_owner_agent_id: String,
    new_content_script: String,
    content_auto_interval_seconds: String,
    content_auto_max_cards_per_run: String,
    content_auto_max_runs_per_day: String,
    content_auto_max_attempts_per_card_per_day: String,
    memory_search_query: String,

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
    RuntimeConfigWizardSnapshot,
);

impl Default for GuiApp {
    fn default() -> Self {
        Self {
            theme_applied: false,
            initial_load_done: false,
            auto_launch_attempted: false,
            active_tab: MainTab::Tasks,
            gateway_base_url: std::env::var("CARSINOS_GATEWAY_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:18789".to_string()),
            gateway_token: std::env::var("CARSINOS_GATEWAY_TOKEN").unwrap_or_default(),
            health: None,
            status: None,
            sessions: Vec::new(),
            approvals: Vec::new(),
            auth_profiles: Vec::new(),
            team_agents: Vec::new(),
            boards: Vec::new(),
            tasks_board: BoardDetailItem::default(),
            content_board: BoardDetailItem::default(),
            content_automation_rules: Vec::new(),
            calendar_jobs: Vec::new(),
            memory_notes: Vec::new(),
            channel_config: ChannelConfigSnapshot::default(),
            runtime_config: RuntimeConfigWizardSnapshot::default(),
            runtime_wizard_step: RuntimeWizardStep::EdgeIdentity,
            runtime_wizard_rollback_reason: String::new(),
            discord_bot_token_plaintext: String::new(),
            telegram_bot_token_plaintext: String::new(),
            selected_session_id: None,
            timeline: Vec::new(),
            new_session_title: String::new(),
            composer_role: "user".to_string(),
            composer_content_text: String::new(),
            run_model_provider: "mock".to_string(),
            run_model_id: "mock-echo-v1".to_string(),
            run_auth_profile_id: String::new(),
            new_task_title: String::new(),
            new_task_owner_agent_id: "lyra".to_string(),
            new_content_title: String::new(),
            new_content_owner_agent_id: "claude".to_string(),
            new_content_script: String::new(),
            content_auto_interval_seconds: "3600".to_string(),
            content_auto_max_cards_per_run: "2".to_string(),
            content_auto_max_runs_per_day: "24".to_string(),
            content_auto_max_attempts_per_card_per_day: "2".to_string(),
            memory_search_query: String::new(),
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
                    if ui.button("Generate Local Token").clicked() {
                        self.gateway_token = generate_local_gateway_token();
                        self.set_info("Generated a new local gateway token");
                    }
                    ui.small("For local runs, keep URL as http://127.0.0.1:18789.");
                    if ui.button("Reconnect + Refresh").clicked() {
                        self.refresh_gateway_state();
                    }
                });

                card(ui, "Navigation", |ui| {
                    for tab in [
                        MainTab::Tasks,
                        MainTab::Content,
                        MainTab::Calendar,
                        MainTab::Memory,
                        MainTab::Team,
                        MainTab::Approvals,
                        MainTab::Settings,
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
            MainTab::Tasks => self.render_tasks_board(ui),
            MainTab::Content => self.render_content_board(ui),
            MainTab::Calendar => self.render_calendar(ui),
            MainTab::Memory => self.render_memory(ui),
            MainTab::Team => self.render_team(ui),
            MainTab::Approvals => self.render_approvals(ui),
            MainTab::Settings => self.render_settings(ui),
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
                self.set_mc3_refresh_status("Gateway state refreshed");
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
                            self.set_mc3_refresh_status("Gateway auto-launched and connected");
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

    fn set_mc3_refresh_status(&mut self, success_message: &str) {
        if let Err(mc3_err) = self.refresh_mc3_state() {
            self.set_info(format!("{success_message}; MC3 sync failed: {mc3_err}"));
        } else {
            self.set_info(success_message);
        }
    }

    fn apply_snapshots(
        &mut self,
        (
            health,
            status,
            sessions,
            approvals,
            auth_profiles,
            channel_config,
            runtime_config,
        ): GatewaySnapshots,
    ) {
        self.health = Some(health);
        self.status = Some(status);
        self.sessions = sessions;
        self.approvals = approvals;
        self.auth_profiles = auth_profiles;
        self.channel_config = channel_config;
        self.runtime_config = runtime_config;
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
            self.gateway_token = generate_local_gateway_token();
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

    fn save_runtime_config(&mut self) {
        if let Err(err) = validate_runtime_config_draft(&self.runtime_config) {
            self.set_error(err);
            return;
        }
        let completeness_issues = runtime_wizard_completeness_issues(&self.runtime_config);
        let mut to_apply = self.runtime_config.clone();
        if !completeness_issues.is_empty() {
            for provider in &mut to_apply.provider_policies {
                provider.allow_consumer_oauth = false;
            }
        }

        let payload = match runtime_config_update_payload(&to_apply) {
            Ok(value) => value,
            Err(err) => {
                self.set_error(err);
                return;
            }
        };
        match send_json(
            &self.gateway_base_url,
            "/api/v1/config/runtime",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(value) => match parse_runtime_config(&value) {
                Ok(config) => {
                    self.runtime_config = config;
                    if completeness_issues.is_empty() {
                        self.set_info("Runtime wizard configuration applied");
                    } else {
                        self.set_info("Runtime config saved with high-risk OAuth forced OFF until wizard completeness is green");
                    }
                }
                Err(err) => self.set_error(err),
            },
            Err(err) => self.set_error(err),
        }
    }

    fn rollback_runtime_config(&mut self) {
        let reason = self.runtime_wizard_rollback_reason.trim().to_string();
        let payload = if reason.is_empty() {
            json!({})
        } else {
            json!({ "reason": reason })
        };
        match send_json(
            &self.gateway_base_url,
            "/api/v1/config/runtime/rollback",
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(value) => match parse_runtime_config(&value) {
                Ok(config) => {
                    self.runtime_config = config;
                    self.runtime_wizard_rollback_reason.clear();
                    self.set_info("Runtime configuration rolled back to last-known-good snapshot");
                }
                Err(err) => self.set_error(err),
            },
            Err(err) => self.set_error(err),
        }
    }

    fn upsert_runtime_secret_ref(
        &self,
        scope: &str,
        secret_value: &str,
        previous_secret_ref: Option<&str>,
    ) -> Result<String, String> {
        let mut payload = json!({
            "scope": scope,
            "secret_value": secret_value,
        });
        if let Some(previous) = previous_secret_ref
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            payload["previous_secret_ref"] = Value::String(previous.to_string());
        }
        let value = send_json(
            &self.gateway_base_url,
            "/api/v1/config/runtime/secrets/upsert",
            "POST",
            &self.gateway_token,
            Some(&payload),
        )?;
        value
            .get("secret_ref")
            .and_then(|entry| entry.as_str())
            .map(|entry| entry.to_string())
            .ok_or_else(|| "runtime secret upsert response missing secret_ref".to_string())
    }

    fn runtime_step_issues(&self, step: RuntimeWizardStep) -> Vec<String> {
        let mut issues = Vec::new();
        match step {
            RuntimeWizardStep::EdgeIdentity => {
                if parse_string_csv(&self.runtime_config.jwt_issuer_allowlist_csv).is_empty() {
                    issues.push(
                        "global.jwt_issuer_allowlist must contain at least one issuer".to_string(),
                    );
                }
                if parse_string_csv(&self.runtime_config.jwt_audience_allowlist_csv).is_empty() {
                    issues.push(
                        "global.jwt_audience_allowlist must contain at least one audience"
                            .to_string(),
                    );
                }
                if parse_string_csv(&self.runtime_config.trusted_proxy_allowlist_csv).is_empty() {
                    issues.push("global.trusted_proxy_allowlist must contain at least one trusted proxy/CIDR".to_string());
                }
                let tls_mode = self
                    .runtime_config
                    .tls_termination_mode
                    .trim()
                    .to_ascii_lowercase();
                if !TLS_TERMINATION_MODES.contains(&tls_mode.as_str()) {
                    issues.push(
                        "global.tls_termination_mode must be edge|gateway|passthrough".to_string(),
                    );
                }
                if self.runtime_config.public_base_url.trim().is_empty() {
                    issues.push(
                        "global.public_base_url should be set for internet-facing deployment"
                            .to_string(),
                    );
                }
            }
            RuntimeWizardStep::ProviderRisk => {
                if self.runtime_config.provider_policies.is_empty() {
                    issues.push("providers must include at least one provider policy".to_string());
                }
                let mut seen = HashSet::new();
                for provider in &self.runtime_config.provider_policies {
                    let provider_id = provider.provider.trim().to_ascii_lowercase();
                    if provider_id.is_empty() {
                        issues.push("provider policy contains empty provider id".to_string());
                        continue;
                    }
                    if !seen.insert(provider_id.clone()) {
                        issues.push(format!("provider policy duplicated for {}", provider_id));
                    }
                    let scope = provider.kill_switch_scope.trim().to_ascii_lowercase();
                    if !KILL_SWITCH_SCOPES.contains(&scope.as_str()) {
                        issues.push(format!(
                            "provider {} kill_switch_scope must be none|profile|provider|global",
                            provider_id
                        ));
                    }
                }
            }
            RuntimeWizardStep::Channels => {
                let discord_mode = self
                    .runtime_config
                    .discord_operation_mode
                    .trim()
                    .to_ascii_lowercase();
                if !CHANNEL_OPERATION_MODES.contains(&discord_mode.as_str()) {
                    issues
                        .push("channels.discord.operation_mode must be shim|transport".to_string());
                }
                if discord_mode == "transport"
                    && self
                        .runtime_config
                        .discord_bot_token_secret_ref
                        .trim()
                        .is_empty()
                {
                    issues.push("channels.discord.bot_token_secret_ref is required when discord operation_mode=transport".to_string());
                }

                let telegram_mode = self
                    .runtime_config
                    .telegram_operation_mode
                    .trim()
                    .to_ascii_lowercase();
                if !CHANNEL_OPERATION_MODES.contains(&telegram_mode.as_str()) {
                    issues.push(
                        "channels.telegram.operation_mode must be shim|transport".to_string(),
                    );
                }
                if telegram_mode == "transport"
                    && self
                        .runtime_config
                        .telegram_bot_token_secret_ref
                        .trim()
                        .is_empty()
                {
                    issues.push("channels.telegram.bot_token_secret_ref is required when telegram operation_mode=transport".to_string());
                }
                if let Err(err) = validate_secret_ref_format(
                    "channels.discord.bot_token_secret_ref",
                    &self.runtime_config.discord_bot_token_secret_ref,
                ) {
                    issues.push(err);
                }
                if let Err(err) = validate_secret_ref_format(
                    "channels.telegram.bot_token_secret_ref",
                    &self.runtime_config.telegram_bot_token_secret_ref,
                ) {
                    issues.push(err);
                }
                let mode = self
                    .runtime_config
                    .telegram_webhook_mode
                    .trim()
                    .to_ascii_lowercase();
                if !TELEGRAM_WEBHOOK_MODES.contains(&mode.as_str()) {
                    issues.push(
                        "channels.telegram.webhook_mode must be long_poll|webhook".to_string(),
                    );
                } else if mode == "webhook"
                    && self.runtime_config.telegram_webhook_url.trim().is_empty()
                {
                    issues.push(
                        "channels.telegram.webhook_url is required when webhook_mode=webhook"
                            .to_string(),
                    );
                }
                if let Err(err) = parse_i64_csv(&self.runtime_config.telegram_staging_chat_ids_csv)
                {
                    issues.push(err);
                }
            }
            RuntimeWizardStep::SecurityOps => {
                for (field_name, value) in [
                    (
                        "security.threat_model_approver",
                        &self.runtime_config.threat_model_approver,
                    ),
                    (
                        "security.risk_acceptance_owner",
                        &self.runtime_config.risk_acceptance_owner,
                    ),
                    (
                        "security.incident_primary",
                        &self.runtime_config.incident_primary,
                    ),
                    (
                        "security.incident_backup",
                        &self.runtime_config.incident_backup,
                    ),
                    (
                        "security.audit_archive_target",
                        &self.runtime_config.audit_archive_target,
                    ),
                    (
                        "security.audit_archive_encryption",
                        &self.runtime_config.audit_archive_encryption,
                    ),
                ] {
                    if value.trim().is_empty() {
                        issues.push(format!("{field_name} is required for production signoff"));
                    }
                }

                let hot_retention = match parse_i64_field(
                    "security.audit_hot_retention_days",
                    &self.runtime_config.audit_hot_retention_days,
                ) {
                    Ok(value) => value,
                    Err(err) => {
                        issues.push(err);
                        0
                    }
                };
                let archive_retention = match parse_i64_field(
                    "security.audit_archive_retention_days",
                    &self.runtime_config.audit_archive_retention_days,
                ) {
                    Ok(value) => value,
                    Err(err) => {
                        issues.push(err);
                        0
                    }
                };
                if hot_retention > 0 && hot_retention < 90 {
                    issues.push("security.audit_hot_retention_days must be >= 90".to_string());
                }
                if archive_retention > 0 && hot_retention > 0 && archive_retention < hot_retention {
                    issues.push("security.audit_archive_retention_days must be >= security.audit_hot_retention_days".to_string());
                }
            }
            RuntimeWizardStep::ReviewApply => {
                if let Err(err) = validate_runtime_config_draft(&self.runtime_config) {
                    issues.push(err);
                }
            }
        }
        issues
    }

    fn runtime_completeness_issues(&self) -> Vec<String> {
        runtime_wizard_completeness_issues(&self.runtime_config)
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

    fn board_id_by_key(&self, board_key: &str) -> Option<String> {
        self.boards
            .iter()
            .find(|board| board.board_key == board_key)
            .map(|board| board.board_id.clone())
    }

    fn refresh_mc3_state(&mut self) -> Result<(), String> {
        self.team_agents = parse_team_agents(&fetch_json(
            &self.gateway_base_url,
            "/api/v1/agents",
            &self.gateway_token,
        )?)?;
        self.boards = parse_board_summaries(&fetch_json(
            &self.gateway_base_url,
            "/api/v1/boards",
            &self.gateway_token,
        )?)?;
        self.calendar_jobs = parse_calendar_jobs(&fetch_json(
            &self.gateway_base_url,
            "/api/v1/jobs?limit=200",
            &self.gateway_token,
        )?)?;
        self.memory_notes = parse_memory_notes(&fetch_json(
            &self.gateway_base_url,
            "/api/v1/memory/notes?limit=200",
            &self.gateway_token,
        )?)?;

        if let Some(tasks_board_id) = self.board_id_by_key("tasks") {
            let path = format!("/api/v1/boards/{tasks_board_id}");
            self.tasks_board = parse_board_detail(&fetch_json(
                &self.gateway_base_url,
                &path,
                &self.gateway_token,
            )?)?;
        }
        self.content_automation_rules = Vec::new();
        if let Some(content_board_id) = self.board_id_by_key("content") {
            let path = format!("/api/v1/boards/{content_board_id}");
            self.content_board = parse_board_detail(&fetch_json(
                &self.gateway_base_url,
                &path,
                &self.gateway_token,
            )?)?;
            let automation_path = format!("/api/v1/boards/{content_board_id}/automation");
            self.content_automation_rules = parse_board_automation_rules(&fetch_json(
                &self.gateway_base_url,
                &automation_path,
                &self.gateway_token,
            )?)?;
        }
        Ok(())
    }

    fn create_card_for_board(
        &mut self,
        board_key: &str,
        title: String,
        owner_agent_id: String,
        script_markdown: Option<String>,
    ) {
        let board_id = match self.board_id_by_key(board_key) {
            Some(value) => value,
            None => {
                self.set_error(format!("board '{}' not found", board_key));
                return;
            }
        };
        let board = if board_key == "tasks" {
            self.tasks_board.clone()
        } else {
            self.content_board.clone()
        };
        let first_column = board
            .columns
            .iter()
            .min_by_key(|column| column.position)
            .map(|column| column.column_id.clone());
        let column_id = match first_column {
            Some(value) => value,
            None => {
                self.set_error("board has no columns configured");
                return;
            }
        };
        let mut payload = json!({
            "column_id": column_id,
            "title": title,
            "owner_kind": "agent",
            "owner_agent_id": owner_agent_id,
        });
        if let Some(script) = script_markdown.map(|value| value.trim().to_string()) {
            if !script.is_empty() {
                payload["script_markdown"] = Value::String(script);
            }
        }
        match send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/cards/create"),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                let _ = self.refresh_mc3_state();
                self.set_info("Card created");
            }
            Err(err) => self.set_error(err),
        }
    }

    fn move_card(
        &mut self,
        board_key: &str,
        card_id: &str,
        target_column_id: &str,
    ) -> Result<(), String> {
        let board_id = self
            .board_id_by_key(board_key)
            .ok_or_else(|| format!("board '{}' not found", board_key))?;
        let payload = json!({ "column_id": target_column_id });
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/cards/{card_id}/move"),
            "POST",
            &self.gateway_token,
            Some(&payload),
        )?;
        self.refresh_mc3_state()?;
        Ok(())
    }

    fn run_card(&mut self, board_key: &str, card_id: &str) -> Result<(), String> {
        let board_id = self
            .board_id_by_key(board_key)
            .ok_or_else(|| format!("board '{}' not found", board_key))?;
        let (model_provider, model_id) = self.resolve_card_run_model(board_key, card_id)?;
        let mut payload = json!({
            "model_provider": model_provider,
            "model_id": model_id
        });
        let auth_profile_id = self.run_auth_profile_id.trim();
        if !auth_profile_id.is_empty() {
            payload["auth_profile_id"] = Value::String(auth_profile_id.to_string());
        }
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/cards/{card_id}/run"),
            "POST",
            &self.gateway_token,
            Some(&payload),
        )?;
        self.refresh_mc3_state()?;
        Ok(())
    }

    fn resolve_card_run_model(
        &self,
        board_key: &str,
        card_id: &str,
    ) -> Result<(String, String), String> {
        let board = match board_key {
            "tasks" => &self.tasks_board,
            "content" => &self.content_board,
            _ => return Err(format!("board '{}' not supported for card runs", board_key)),
        };
        let owner_agent_id = board
            .cards
            .iter()
            .find(|card| card.card_id == card_id)
            .and_then(|card| card.owner_agent_id.as_deref());
        if let Some(owner_agent_id) = owner_agent_id {
            if let Some(agent) = self
                .team_agents
                .iter()
                .find(|candidate| candidate.agent_id == owner_agent_id)
            {
                let model_provider = agent.model_provider.trim();
                let model_id = agent.model_id.trim();
                if !model_provider.is_empty() && !model_id.is_empty() {
                    return Ok((model_provider.to_string(), model_id.to_string()));
                }
            }
        }

        let fallback_provider = self.run_model_provider.trim();
        let fallback_model_id = self.run_model_id.trim();
        if fallback_provider.is_empty() || fallback_model_id.is_empty() {
            return Err(
                "card run model is not configured (set session run provider/model or assign an owner agent with model settings)"
                    .to_string(),
            );
        }
        Ok((fallback_provider.to_string(), fallback_model_id.to_string()))
    }

    fn update_card_script(
        &mut self,
        board_key: &str,
        card_id: &str,
        script_markdown: &str,
    ) -> Result<(), String> {
        let board_id = self
            .board_id_by_key(board_key)
            .ok_or_else(|| format!("board '{}' not found", board_key))?;
        let payload = json!({
            "script_markdown": script_markdown
        });
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/cards/{card_id}/update"),
            "POST",
            &self.gateway_token,
            Some(&payload),
        )?;
        self.refresh_mc3_state()?;
        Ok(())
    }

    fn upsert_content_script_to_thumbnail_rule(&mut self, enabled: bool) {
        let board_id = match self.board_id_by_key("content") {
            Some(value) => value,
            None => {
                self.set_error("content board not found");
                return;
            }
        };
        let source_column_id = self
            .content_board
            .columns
            .iter()
            .find(|column| column.column_key == "scripting")
            .map(|column| column.column_id.clone());
        let target_column_id = self
            .content_board
            .columns
            .iter()
            .find(|column| column.column_key == "thumbnail")
            .map(|column| column.column_id.clone());
        let source_column_id = match source_column_id {
            Some(value) => value,
            None => {
                self.set_error("content board missing 'scripting' column");
                return;
            }
        };
        let target_column_id = match target_column_id {
            Some(value) => value,
            None => {
                self.set_error("content board missing 'thumbnail' column");
                return;
            }
        };
        let interval_seconds = match parse_i64_field(
            "content_auto_interval_seconds",
            &self.content_auto_interval_seconds,
        ) {
            Ok(value) => value.clamp(60, 86_400),
            Err(err) => {
                self.set_error(err);
                return;
            }
        };
        let max_cards_per_run = match parse_i64_field(
            "content_auto_max_cards_per_run",
            &self.content_auto_max_cards_per_run,
        ) {
            Ok(value) => value.clamp(1, 32),
            Err(err) => {
                self.set_error(err);
                return;
            }
        };
        let max_runs_per_day = match parse_i64_field(
            "content_auto_max_runs_per_day",
            &self.content_auto_max_runs_per_day,
        ) {
            Ok(value) => value.clamp(1, 5000),
            Err(err) => {
                self.set_error(err);
                return;
            }
        };
        let max_attempts_per_card_per_day = match parse_i64_field(
            "content_auto_max_attempts_per_card_per_day",
            &self.content_auto_max_attempts_per_card_per_day,
        ) {
            Ok(value) => value.clamp(1, 50),
            Err(err) => {
                self.set_error(err);
                return;
            }
        };

        let payload = json!({
            "name": "Script -> Thumbnail Automation",
            "enabled": enabled,
            "agent_id": self.new_content_owner_agent_id.trim(),
            "schedule_kind": "interval",
            "interval_seconds": interval_seconds,
            "target_column_id": target_column_id,
            "max_cards_per_run": max_cards_per_run,
            "max_runs_per_day": max_runs_per_day,
            "max_attempts_per_card_per_day": max_attempts_per_card_per_day,
            "breaker_failure_threshold": 3,
            "breaker_cooldown_ms": 900000,
            "generate_thumbnail_draft": true,
            "model_provider": "mock",
            "model_id": "mock-echo-v1",
            "max_retries": 0
        });

        match send_json(
            &self.gateway_base_url,
            &format!(
                "/api/v1/boards/{}/columns/{}/automation/upsert",
                board_id, source_column_id
            ),
            "POST",
            &self.gateway_token,
            Some(&payload),
        ) {
            Ok(_) => {
                let _ = self.refresh_mc3_state();
                if enabled {
                    self.set_info("Content automation enabled");
                } else {
                    self.set_info("Content automation rule updated");
                }
            }
            Err(err) => self.set_error(err),
        }
    }

    fn set_content_rule_enabled(&mut self, job_id: &str, enabled: bool) -> Result<(), String> {
        let board_id = self
            .board_id_by_key("content")
            .ok_or_else(|| "content board not found".to_string())?;
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/automation/{job_id}/state"),
            "POST",
            &self.gateway_token,
            Some(&json!({ "enabled": enabled })),
        )?;
        self.refresh_mc3_state()?;
        Ok(())
    }

    fn run_content_rule_now(&mut self, job_id: &str) -> Result<(), String> {
        let board_id = self
            .board_id_by_key("content")
            .ok_or_else(|| "content board not found".to_string())?;
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/boards/{board_id}/automation/{job_id}/run"),
            "POST",
            &self.gateway_token,
            Some(&json!({})),
        )?;
        self.refresh_mc3_state()?;
        Ok(())
    }

    fn run_job_now(&mut self, job_id: &str) -> Result<(), String> {
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/jobs/{job_id}/run"),
            "POST",
            &self.gateway_token,
            Some(&json!({})),
        )?;
        self.refresh_gateway_state();
        Ok(())
    }

    fn set_job_enabled(&mut self, job_id: &str, enabled: bool) -> Result<(), String> {
        send_json(
            &self.gateway_base_url,
            &format!("/api/v1/jobs/{job_id}/update"),
            "POST",
            &self.gateway_token,
            Some(&json!({ "enabled": enabled })),
        )?;
        self.refresh_gateway_state();
        Ok(())
    }

    fn render_tasks_board(&mut self, ui: &mut egui::Ui) {
        card(ui, "Tasks Pipeline", |ui| {
            ui.horizontal(|ui| {
                ui.label("New task");
                ui.text_edit_singleline(&mut self.new_task_title);
                ui.label("Agent");
                ui.text_edit_singleline(&mut self.new_task_owner_agent_id);
                if ui.button("Create").clicked() {
                    let title = self.new_task_title.trim().to_string();
                    if title.is_empty() {
                        self.set_error("task title cannot be empty");
                    } else {
                        self.create_card_for_board(
                            "tasks",
                            title,
                            self.new_task_owner_agent_id.trim().to_string(),
                            None,
                        );
                        self.new_task_title.clear();
                    }
                }
                if ui.button("Refresh").clicked() {
                    let _ = self.refresh_mc3_state();
                }
            });
        });

        let board = self.tasks_board.clone();
        if board.columns.is_empty() {
            ui.label("Tasks board unavailable yet. Run Refresh.");
            return;
        }

        let mut pending_move: Option<(String, String)> = None;
        let mut pending_run: Option<String> = None;

        ui.columns(board.columns.len(), |columns_ui| {
            for (index, column) in board.columns.iter().enumerate() {
                card(&mut columns_ui[index], &column.name, |ui| {
                    let mut cards = board
                        .cards
                        .iter()
                        .filter(|card| card.column_id == column.column_id)
                        .cloned()
                        .collect::<Vec<_>>();
                    cards.sort_by_key(|card| card.position);
                    for card_item in cards {
                        egui::Frame::group(ui.style())
                            .fill(Color32::from_rgb(26, 31, 46))
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(112, 188, 255)))
                            .show(ui, |ui| {
                                ui.label(RichText::new(&card_item.title).strong());
                                if let Some(description) = card_item.description.as_ref() {
                                    if !description.trim().is_empty() {
                                        ui.small(description.trim());
                                    }
                                }
                                ui.small(format!(
                                    "owner: {}",
                                    card_item
                                        .owner_agent_id
                                        .unwrap_or_else(|| "unassigned".to_string())
                                ));
                                ui.horizontal(|ui| {
                                    if ui.button("Run").clicked() {
                                        pending_run = Some(card_item.card_id.clone());
                                    }
                                    let next_column = board
                                        .columns
                                        .iter()
                                        .filter(|candidate| candidate.position > column.position)
                                        .min_by_key(|candidate| candidate.position)
                                        .cloned();
                                    if let Some(next_column) = next_column {
                                        if ui
                                            .button(format!("Move -> {}", next_column.name))
                                            .clicked()
                                        {
                                            pending_move = Some((
                                                card_item.card_id.clone(),
                                                next_column.column_id,
                                            ));
                                        }
                                    }
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
            }
        });

        if let Some((card_id, column_id)) = pending_move {
            if let Err(err) = self.move_card("tasks", &card_id, &column_id) {
                self.set_error(err);
            } else {
                self.set_info("Task card moved");
            }
        }
        if let Some(card_id) = pending_run {
            if let Err(err) = self.run_card("tasks", &card_id) {
                self.set_error(err);
            } else {
                self.set_info("Task card run executed");
            }
        }
    }

    fn render_content_board(&mut self, ui: &mut egui::Ui) {
        card(ui, "Content Pipeline", |ui| {
            ui.horizontal(|ui| {
                ui.label("Title");
                ui.text_edit_singleline(&mut self.new_content_title);
                ui.label("Agent");
                ui.text_edit_singleline(&mut self.new_content_owner_agent_id);
            });
            ui.label("Script draft");
            ui.add(egui::TextEdit::multiline(&mut self.new_content_script).desired_rows(4));
            ui.horizontal(|ui| {
                if ui.button("Create Content Card").clicked() {
                    let title = self.new_content_title.trim().to_string();
                    if title.is_empty() {
                        self.set_error("content title cannot be empty");
                    } else {
                        self.create_card_for_board(
                            "content",
                            title,
                            self.new_content_owner_agent_id.trim().to_string(),
                            Some(self.new_content_script.clone()),
                        );
                        self.new_content_title.clear();
                        self.new_content_script.clear();
                    }
                }
                if ui.button("Refresh").clicked() {
                    let _ = self.refresh_mc3_state();
                }
            });
        });

        let board = self.content_board.clone();
        if board.columns.is_empty() {
            ui.label("Content board unavailable yet. Run Refresh.");
            return;
        }

        let automation_rules = self.content_automation_rules.clone();
        let mut pending_rule_upsert: Option<bool> = None;
        let mut pending_rule_state: Option<(String, bool)> = None;
        let mut pending_rule_run: Option<String> = None;

        card(ui, "Content Automation", |ui| {
            ui.label("Script -> Thumbnail rule (config-first defaults, no hardcoded IDs)");
            ui.horizontal(|ui| {
                ui.label("interval_s");
                ui.text_edit_singleline(&mut self.content_auto_interval_seconds);
                ui.label("cards/run");
                ui.text_edit_singleline(&mut self.content_auto_max_cards_per_run);
                ui.label("runs/day");
                ui.text_edit_singleline(&mut self.content_auto_max_runs_per_day);
                ui.label("attempts/card/day");
                ui.text_edit_singleline(&mut self.content_auto_max_attempts_per_card_per_day);
            });
            ui.horizontal(|ui| {
                if ui.button("Save Rule (Disabled)").clicked() {
                    pending_rule_upsert = Some(false);
                }
                if ui.button("Save + Enable").clicked() {
                    pending_rule_upsert = Some(true);
                }
            });
            if automation_rules.is_empty() {
                ui.small("No automation rules configured for content board.");
            } else {
                for rule in &automation_rules {
                    egui::Frame::group(ui.style())
                        .fill(Color32::from_rgb(19, 25, 40))
                        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(112, 188, 255)))
                        .show(ui, |ui| {
                            ui.label(RichText::new(&rule.name).strong());
                            ui.small(format!("job_id: {}", rule.job_id));
                            ui.small(format!(
                                "enabled={} next_run_at={} cards/run={} runs/day={} attempts/card/day={}",
                                rule.enabled,
                                rule.next_run_at
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "none".to_string()),
                                rule.max_cards_per_run,
                                rule.max_runs_per_day,
                                rule.max_attempts_per_card_per_day
                            ));
                            if let Some(error) = rule.last_error.as_ref() {
                                ui.colored_label(
                                    Color32::from_rgb(255, 188, 104),
                                    format!("last_error: {error}"),
                                );
                            }
                            ui.horizontal(|ui| {
                                if ui.button("Run Now").clicked() {
                                    pending_rule_run = Some(rule.job_id.clone());
                                }
                                if ui
                                    .button(if rule.enabled { "Pause" } else { "Resume" })
                                    .clicked()
                                {
                                    pending_rule_state =
                                        Some((rule.job_id.clone(), !rule.enabled));
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            }
        });

        let mut pending_move: Option<(String, String)> = None;
        let mut pending_run: Option<String> = None;
        let mut pending_script_update: Option<(String, String)> = None;

        ui.columns(board.columns.len(), |columns_ui| {
            for (index, column) in board.columns.iter().enumerate() {
                card(&mut columns_ui[index], &column.name, |ui| {
                    let mut cards = board
                        .cards
                        .iter()
                        .filter(|card| card.column_id == column.column_id)
                        .cloned()
                        .collect::<Vec<_>>();
                    cards.sort_by_key(|card| card.position);
                    for card_item in cards {
                        egui::Frame::group(ui.style())
                            .fill(Color32::from_rgb(28, 24, 42))
                            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(255, 214, 109)))
                            .show(ui, |ui| {
                                ui.label(RichText::new(&card_item.title).strong());
                                ui.small(format!(
                                    "owner: {}",
                                    card_item
                                        .owner_agent_id
                                        .clone()
                                        .unwrap_or_else(|| "unassigned".to_string())
                                ));
                                let mut script =
                                    card_item.script_markdown.clone().unwrap_or_default();
                                ui.add(egui::TextEdit::multiline(&mut script).desired_rows(3));
                                ui.horizontal(|ui| {
                                    if ui.button("Save Script").clicked() {
                                        pending_script_update =
                                            Some((card_item.card_id.clone(), script.clone()));
                                    }
                                    if ui.button("Run").clicked() {
                                        pending_run = Some(card_item.card_id.clone());
                                    }
                                });
                                if !card_item.assets.is_empty() {
                                    ui.small("Assets");
                                    for asset in &card_item.assets {
                                        ui.small(format!(
                                            "- {} ({} bytes, {})",
                                            asset.filename, asset.bytes, asset.mime
                                        ));
                                    }
                                }
                                let next_column = board
                                    .columns
                                    .iter()
                                    .filter(|candidate| candidate.position > column.position)
                                    .min_by_key(|candidate| candidate.position)
                                    .cloned();
                                if let Some(next_column) = next_column {
                                    if ui
                                        .button(format!("Advance -> {}", next_column.name))
                                        .clicked()
                                    {
                                        pending_move = Some((
                                            card_item.card_id.clone(),
                                            next_column.column_id,
                                        ));
                                    }
                                }
                            });
                        ui.add_space(6.0);
                    }
                });
            }
        });

        if let Some((card_id, script)) = pending_script_update {
            if let Err(err) = self.update_card_script("content", &card_id, &script) {
                self.set_error(err);
            } else {
                self.set_info("Content card script updated");
            }
        }
        if let Some((card_id, column_id)) = pending_move {
            if let Err(err) = self.move_card("content", &card_id, &column_id) {
                self.set_error(err);
            } else {
                self.set_info("Content card advanced");
            }
        }
        if let Some(card_id) = pending_run {
            if let Err(err) = self.run_card("content", &card_id) {
                self.set_error(err);
            } else {
                self.set_info("Content card run executed");
            }
        }
        if let Some(enabled) = pending_rule_upsert {
            self.upsert_content_script_to_thumbnail_rule(enabled);
        }
        if let Some((job_id, enabled)) = pending_rule_state {
            if let Err(err) = self.set_content_rule_enabled(&job_id, enabled) {
                self.set_error(err);
            } else if enabled {
                self.set_info("Content automation resumed");
            } else {
                self.set_info("Content automation paused");
            }
        }
        if let Some(job_id) = pending_rule_run {
            if let Err(err) = self.run_content_rule_now(&job_id) {
                self.set_error(err);
            } else {
                self.set_info("Content automation run submitted");
            }
        }
    }

    fn render_calendar(&mut self, ui: &mut egui::Ui) {
        card(ui, "Calendar + Scheduler", |ui| {
            ui.horizontal(|ui| {
                if ui.button("Refresh Jobs").clicked() {
                    let _ = self.refresh_mc3_state();
                }
                ui.label(format!("jobs: {}", self.calendar_jobs.len()));
            });
        });

        let jobs = self.calendar_jobs.clone();
        let mut pending_run: Option<String> = None;
        let mut pending_toggle: Option<(String, bool)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for job in jobs {
                card(ui, &job.name, |ui| {
                    ui.label(format!("job_id: {}", job.job_id));
                    ui.label(format!("agent: {}", job.agent_id));
                    ui.label(format!("schedule: {}", job.schedule_kind));
                    ui.label(format!(
                        "next_run_at_ms: {}",
                        job.next_run_at
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "none".to_string())
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Run now").clicked() {
                            pending_run = Some(job.job_id.clone());
                        }
                        if ui
                            .button(if job.enabled { "Pause" } else { "Resume" })
                            .clicked()
                        {
                            pending_toggle = Some((job.job_id.clone(), !job.enabled));
                        }
                    });
                });
            }
        });

        if let Some(job_id) = pending_run {
            if let Err(err) = self.run_job_now(&job_id) {
                self.set_error(err);
            } else {
                self.set_info("Job triggered");
            }
        }
        if let Some((job_id, enabled)) = pending_toggle {
            if let Err(err) = self.set_job_enabled(&job_id, enabled) {
                self.set_error(err);
            } else {
                self.set_info("Job state updated");
            }
        }
    }

    fn render_memory(&mut self, ui: &mut egui::Ui) {
        card(ui, "Memory Search + Notes", |ui| {
            ui.horizontal(|ui| {
                ui.label("Filter");
                ui.text_edit_singleline(&mut self.memory_search_query);
                if ui.button("Refresh Notes").clicked() {
                    let _ = self.refresh_mc3_state();
                }
            });
        });

        let query = self.memory_search_query.trim().to_ascii_lowercase();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for note in self.memory_notes.iter().filter(|note| {
                if query.is_empty() {
                    return true;
                }
                note.title
                    .as_ref()
                    .map(|title| title.to_ascii_lowercase().contains(&query))
                    .unwrap_or(false)
                    || note.body_preview.to_ascii_lowercase().contains(&query)
            }) {
                card(ui, note.title.as_deref().unwrap_or("Untitled note"), |ui| {
                    ui.label(format!("note_id: {}", note.note_id));
                    ui.label(format!("updated_at_ms: {}", note.updated_at));
                    ui.small(&note.body_preview);
                });
            }
        });
    }

    fn render_team(&mut self, ui: &mut egui::Ui) {
        card(ui, "Dual Agent Team Control", |ui| {
            ui.horizontal(|ui| {
                if ui.button("Refresh Agents").clicked() {
                    let _ = self.refresh_mc3_state();
                }
                ui.label("Lyra + Claude are first-class active operators.");
            });
        });
        let agents = self.team_agents.clone();
        ui.columns(2, |columns_ui| {
            for (index, agent) in agents
                .iter()
                .filter(|agent| agent.agent_id == "lyra" || agent.agent_id == "claude")
                .enumerate()
            {
                let target_index = index.min(columns_ui.len().saturating_sub(1));
                card(
                    &mut columns_ui[target_index],
                    &format!("{} ({})", agent.name, agent.agent_id),
                    |ui| {
                        ui.label(format!("model_provider: {}", agent.model_provider));
                        ui.label(format!("model_id: {}", agent.model_id));
                        ui.label(format!("tool_profile: {}", agent.tool_profile));
                        ui.label("status: active");
                    },
                );
            }
        });
        for agent in agents
            .iter()
            .filter(|agent| agent.agent_id != "lyra" && agent.agent_id != "claude")
        {
            card(ui, &format!("{} ({})", agent.name, agent.agent_id), |ui| {
                ui.label(format!("model_provider: {}", agent.model_provider));
                ui.label(format!("model_id: {}", agent.model_id));
                ui.label(format!("tool_profile: {}", agent.tool_profile));
            });
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        self.render_mission(ui);
        self.render_auth(ui);
        self.render_channels(ui);
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
            ui.label(format!(
                "runtime schema: {} | updated_at_ms: {}",
                self.runtime_config.schema_version, self.runtime_config.updated_at
            ));
        });

        card(ui, "Mission Control Setup Wizard", |ui| {
            let steps = RuntimeWizardStep::all();
            let mut selected_step = self.runtime_wizard_step;
            let current_index = steps
                .iter()
                .position(|item| *item == self.runtime_wizard_step)
                .unwrap_or(0);
            let step_issues = self.runtime_step_issues(self.runtime_wizard_step);
            let completeness_issues = self.runtime_completeness_issues();
            let high_risk_ready = completeness_issues.is_empty();

            ui.horizontal_wrapped(|ui| {
                for step in steps {
                    let selected = selected_step == step;
                    if ui.selectable_label(selected, step.label()).clicked() {
                        selected_step = step;
                    }
                }
            });
            self.runtime_wizard_step = selected_step;
            ui.separator();
            ui.label(self.runtime_wizard_step.guidance());

            if high_risk_ready {
                ui.colored_label(
                    Color32::from_rgb(118, 255, 168),
                    "Wizard completeness: GREEN (high-risk provider OAuth can remain enabled if selected)",
                );
            } else {
                ui.colored_label(
                    Color32::from_rgb(255, 188, 104),
                    "Wizard completeness: INCOMPLETE (high-risk provider OAuth will be forced OFF on apply)",
                );
            }

            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(430.0)
                .show(ui, |ui| match self.runtime_wizard_step {
                    RuntimeWizardStep::EdgeIdentity => {
                        ui.horizontal(|ui| {
                            ui.label("tls_termination_mode");
                            egui::ComboBox::from_id_salt("runtime-tls-termination-mode")
                                .selected_text(self.runtime_config.tls_termination_mode.clone())
                                .show_ui(ui, |ui| {
                                    for mode in TLS_TERMINATION_MODES {
                                        ui.selectable_value(
                                            &mut self.runtime_config.tls_termination_mode,
                                            mode.to_string(),
                                            mode,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("public_base_url");
                            ui.text_edit_singleline(&mut self.runtime_config.public_base_url);
                        });
                        ui.label("jwt_issuer_allowlist (csv)");
                        ui.text_edit_singleline(&mut self.runtime_config.jwt_issuer_allowlist_csv);
                        ui.label("jwt_audience_allowlist (csv)");
                        ui.text_edit_singleline(
                            &mut self.runtime_config.jwt_audience_allowlist_csv,
                        );
                        ui.label("trusted_proxy_allowlist (csv)");
                        ui.text_edit_singleline(
                            &mut self.runtime_config.trusted_proxy_allowlist_csv,
                        );
                    }
                    RuntimeWizardStep::ProviderRisk => {
                        let mut remove_index: Option<usize> = None;
                        for (index, policy) in self
                            .runtime_config
                            .provider_policies
                            .iter_mut()
                            .enumerate()
                        {
                            egui::Frame::group(ui.style())
                                .fill(Color32::from_rgb(21, 24, 34))
                                .stroke(egui::Stroke::new(
                                    1.0,
                                    if policy.enabled {
                                        Color32::from_rgb(118, 255, 168)
                                    } else {
                                        Color32::from_rgb(255, 107, 107)
                                    },
                                ))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("provider");
                                        ui.text_edit_singleline(&mut policy.provider);
                                        ui.checkbox(&mut policy.enabled, "enabled");
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("kill_switch_scope");
                                        egui::ComboBox::from_id_salt(format!(
                                            "runtime-provider-scope-{}",
                                            index
                                        ))
                                        .selected_text(policy.kill_switch_scope.clone())
                                        .show_ui(ui, |ui| {
                                            for scope in KILL_SWITCH_SCOPES {
                                                ui.selectable_value(
                                                    &mut policy.kill_switch_scope,
                                                    scope.to_string(),
                                                    scope,
                                                );
                                            }
                                        });
                                    });
                                    ui.add_enabled_ui(high_risk_ready, |ui| {
                                        ui.checkbox(
                                            &mut policy.allow_consumer_oauth,
                                            "allow_consumer_oauth",
                                        );
                                    });
                                    if !high_risk_ready {
                                        ui.colored_label(
                                            Color32::from_rgb(255, 188, 104),
                                            "High-risk OAuth toggle locked until wizard completeness is green",
                                        );
                                    }
                                    if ui.button("Remove Provider Policy").clicked() {
                                        remove_index = Some(index);
                                    }
                                });
                            ui.add_space(8.0);
                        }
                        if let Some(index) = remove_index {
                            self.runtime_config.provider_policies.remove(index);
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Add Provider Policy").clicked() {
                                self.runtime_config.provider_policies.push(
                                    RuntimeProviderPolicyDraft {
                                        provider: "new-provider".to_string(),
                                        enabled: false,
                                        allow_consumer_oauth: false,
                                        kill_switch_scope: "none".to_string(),
                                    },
                                );
                            }
                            if ui.button("Normalize Provider Policy Set").clicked() {
                                normalize_provider_policies(
                                    &mut self.runtime_config.provider_policies,
                                );
                            }
                        });
                    }
                    RuntimeWizardStep::Channels => {
                        ui.heading("Discord Deployment");
                        if ui.button("Apply Channel Defaults (Local)").clicked() {
                            self.runtime_config.discord_operation_mode = "shim".to_string();
                            self.runtime_config.telegram_operation_mode = "shim".to_string();
                            if self.runtime_config.discord_intents_csv.trim().is_empty() {
                                self.runtime_config.discord_intents_csv =
                                    "guilds,guild_messages,direct_messages".to_string();
                            }
                            if self.runtime_config.telegram_webhook_mode.trim().is_empty() {
                                self.runtime_config.telegram_webhook_mode =
                                    "long_poll".to_string();
                            }
                            if self.runtime_config.telegram_staging_chat_ids_csv.trim().is_empty()
                                && !self
                                    .channel_config
                                    .telegram_allowlisted_user_ids_csv
                                    .trim()
                                    .is_empty()
                            {
                                self.runtime_config.telegram_staging_chat_ids_csv = self
                                    .channel_config
                                    .telegram_allowlisted_user_ids_csv
                                    .clone();
                            }
                            self.set_info(
                                "Applied local channel defaults (Discord intents + Telegram long_poll).",
                            );
                        }
                        ui.small(
                            "Use secret refs (not raw tokens), e.g. secret://runtime.channels.discord.bot_token",
                        );
                        ui.horizontal(|ui| {
                            ui.label("discord_bot_token (store to secret backend)");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.discord_bot_token_plaintext)
                                    .password(true),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("operation_mode");
                            egui::ComboBox::from_id_salt("runtime-discord-operation-mode")
                                .selected_text(self.runtime_config.discord_operation_mode.clone())
                                .show_ui(ui, |ui| {
                                    for mode in CHANNEL_OPERATION_MODES {
                                        ui.selectable_value(
                                            &mut self.runtime_config.discord_operation_mode,
                                            mode.to_string(),
                                            mode,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("bot_token_secret_ref");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.discord_bot_token_secret_ref,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("application_id");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.discord_application_id,
                            );
                        });
                        ui.label("intents (csv)");
                        ui.text_edit_singleline(&mut self.runtime_config.discord_intents_csv);
                        ui.label("staging_guild_ids (csv)");
                        ui.text_edit_singleline(
                            &mut self.runtime_config.discord_staging_guild_ids_csv,
                        );
                        ui.label("staging_channel_ids (csv)");
                        ui.text_edit_singleline(
                            &mut self.runtime_config.discord_staging_channel_ids_csv,
                        );
                        ui.small(
                            "Discord local soak defaults: intents can stay guilds,guild_messages,direct_messages.",
                        );

                        ui.separator();
                        ui.heading("Telegram Deployment");
                        ui.horizontal(|ui| {
                            ui.label("telegram_bot_token (store to secret backend)");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.telegram_bot_token_plaintext)
                                    .password(true),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("operation_mode");
                            egui::ComboBox::from_id_salt("runtime-telegram-operation-mode")
                                .selected_text(self.runtime_config.telegram_operation_mode.clone())
                                .show_ui(ui, |ui| {
                                    for mode in CHANNEL_OPERATION_MODES {
                                        ui.selectable_value(
                                            &mut self.runtime_config.telegram_operation_mode,
                                            mode.to_string(),
                                            mode,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("bot_token_secret_ref");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.telegram_bot_token_secret_ref,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("webhook_mode");
                            egui::ComboBox::from_id_salt("runtime-telegram-webhook-mode")
                                .selected_text(self.runtime_config.telegram_webhook_mode.clone())
                                .show_ui(ui, |ui| {
                                    for mode in TELEGRAM_WEBHOOK_MODES {
                                        ui.selectable_value(
                                            &mut self.runtime_config.telegram_webhook_mode,
                                            mode.to_string(),
                                            mode,
                                        );
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("webhook_url");
                            ui.text_edit_singleline(&mut self.runtime_config.telegram_webhook_url);
                        });
                        ui.label("staging_chat_ids (csv i64)");
                        ui.text_edit_singleline(
                            &mut self.runtime_config.telegram_staging_chat_ids_csv,
                        );
                        if ui.button("Store Channel Tokens In Secret Backend").clicked() {
                            let mut updates = Vec::new();

                            if !self.discord_bot_token_plaintext.trim().is_empty() {
                                match self.upsert_runtime_secret_ref(
                                    "channels/discord/bot_token",
                                    self.discord_bot_token_plaintext.trim(),
                                    Some(&self.runtime_config.discord_bot_token_secret_ref),
                                ) {
                                    Ok(secret_ref) => {
                                        self.runtime_config.discord_bot_token_secret_ref =
                                            secret_ref.clone();
                                        self.discord_bot_token_plaintext.clear();
                                        updates.push(format!("discord={secret_ref}"));
                                    }
                                    Err(err) => {
                                        self.set_error(err);
                                        return;
                                    }
                                }
                            }

                            if !self.telegram_bot_token_plaintext.trim().is_empty() {
                                match self.upsert_runtime_secret_ref(
                                    "channels/telegram/bot_token",
                                    self.telegram_bot_token_plaintext.trim(),
                                    Some(&self.runtime_config.telegram_bot_token_secret_ref),
                                ) {
                                    Ok(secret_ref) => {
                                        self.runtime_config.telegram_bot_token_secret_ref =
                                            secret_ref.clone();
                                        self.telegram_bot_token_plaintext.clear();
                                        updates.push(format!("telegram={secret_ref}"));
                                    }
                                    Err(err) => {
                                        self.set_error(err);
                                        return;
                                    }
                                }
                            }

                            if updates.is_empty() {
                                self.set_info(
                                    "No token values entered. Paste at least one token before storing.",
                                );
                            } else {
                                self.set_info(format!(
                                    "Stored channel token secret refs: {}",
                                    updates.join(", ")
                                ));
                            }
                        }
                        ui.small(
                            "staging_chat_ids = destination chat ID(s). For direct bot testing, this is usually your own Telegram user ID.",
                        );
                        ui.small(
                            "Use operation_mode=shim for local/no-token simulation; set transport only when bot tokens are provisioned.",
                        );
                    }
                    RuntimeWizardStep::SecurityOps => {
                        ui.horizontal(|ui| {
                            ui.label("threat_model_approver");
                            ui.text_edit_singleline(&mut self.runtime_config.threat_model_approver);
                        });
                        ui.horizontal(|ui| {
                            ui.label("risk_acceptance_owner");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.risk_acceptance_owner,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("incident_primary");
                            ui.text_edit_singleline(&mut self.runtime_config.incident_primary);
                        });
                        ui.horizontal(|ui| {
                            ui.label("incident_backup");
                            ui.text_edit_singleline(&mut self.runtime_config.incident_backup);
                        });
                        ui.horizontal(|ui| {
                            ui.label("audit_archive_target");
                            ui.text_edit_singleline(&mut self.runtime_config.audit_archive_target);
                        });
                        ui.horizontal(|ui| {
                            ui.label("audit_archive_encryption");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.audit_archive_encryption,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("audit_hot_retention_days");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.audit_hot_retention_days,
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("audit_archive_retention_days");
                            ui.text_edit_singleline(
                                &mut self.runtime_config.audit_archive_retention_days,
                            );
                        });
                    }
                    RuntimeWizardStep::ReviewApply => {
                        ui.label(format!(
                            "schema_version={} updated_at_ms={}",
                            self.runtime_config.schema_version, self.runtime_config.updated_at
                        ));
                        ui.label(format!(
                            "providers configured: {}",
                            self.runtime_config.provider_policies.len()
                        ));
                        ui.label(format!(
                            "discord secret ref set: {}",
                            !self.runtime_config.discord_bot_token_secret_ref.trim().is_empty()
                        ));
                        ui.label(format!(
                            "telegram secret ref set: {}",
                            !self.runtime_config.telegram_bot_token_secret_ref.trim().is_empty()
                        ));
                        ui.separator();
                        if completeness_issues.is_empty() {
                            ui.colored_label(
                                Color32::from_rgb(118, 255, 168),
                                "No completeness blockers. Runtime config is production-ready.",
                            );
                        } else {
                            ui.colored_label(
                                Color32::from_rgb(255, 188, 104),
                                format!(
                                    "{} completeness blockers remain. Resolve them for production signoff.",
                                    completeness_issues.len()
                                ),
                            );
                            for issue in &completeness_issues {
                                ui.colored_label(
                                    Color32::from_rgb(255, 188, 104),
                                    format!("• {}", issue),
                                );
                            }
                        }
                    }
                });

            if !step_issues.is_empty() {
                ui.separator();
                ui.colored_label(
                    Color32::from_rgb(255, 107, 107),
                    format!(
                        "Current step has {} validation issue(s):",
                        step_issues.len()
                    ),
                );
                for issue in &step_issues {
                    ui.colored_label(Color32::from_rgb(255, 107, 107), format!("• {}", issue));
                }
            }

            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(current_index > 0, egui::Button::new("Back"))
                    .clicked()
                {
                    self.runtime_wizard_step = steps[current_index - 1];
                }
                if ui
                    .add_enabled(current_index + 1 < steps.len(), egui::Button::new("Next"))
                    .clicked()
                {
                    self.runtime_wizard_step = steps[current_index + 1];
                }
                if ui.button("Reload Runtime Config").clicked() {
                    self.refresh_gateway_state();
                }
                if ui.button("Save Wizard Config").clicked() {
                    self.save_runtime_config();
                }
            });
            ui.horizontal(|ui| {
                ui.label("rollback reason (optional)");
                ui.text_edit_singleline(&mut self.runtime_wizard_rollback_reason);
                if ui.button("Rollback Runtime Config").clicked() {
                    self.rollback_runtime_config();
                }
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

fn generate_local_gateway_token() -> String {
    let mut bytes = [0_u8; 24];
    let mut rng = OsRng;
    rng.fill_bytes(&mut bytes);
    let mut token = String::from("carsinos-local-");
    for byte in bytes {
        token.push_str(&format!("{:02x}", byte));
    }
    token
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
    let runtime_config_json = fetch_json(base_url, "/api/v1/config/runtime", token)?;

    Ok((
        parse_health_snapshot(&health_json)?,
        parse_status_snapshot(&status_json)?,
        parse_sessions(&sessions_json)?,
        parse_approvals(&approvals_json)?,
        parse_auth_profiles(&auth_profiles_json)?,
        parse_channel_config(&channel_config_json)?,
        parse_runtime_config(&runtime_config_json)?,
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

fn parse_team_agents(value: &Value) -> Result<Vec<TeamAgentItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "agents.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(TeamAgentItem {
            agent_id: item
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "agent.agent_id missing".to_string())?
                .to_string(),
            name: item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "agent.name missing".to_string())?
                .to_string(),
            model_provider: item
                .get("model_provider")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "agent.model_provider missing".to_string())?
                .to_string(),
            model_id: item
                .get("model_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "agent.model_id missing".to_string())?
                .to_string(),
            tool_profile: item
                .get("tool_profile")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "agent.tool_profile missing".to_string())?
                .to_string(),
        });
    }
    Ok(out)
}

fn parse_board_summaries(value: &Value) -> Result<Vec<BoardSummaryItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "boards.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(BoardSummaryItem {
            board_id: item
                .get("board_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board.board_id missing".to_string())?
                .to_string(),
            board_key: item
                .get("board_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board.board_key missing".to_string())?
                .to_string(),
            name: item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board.name missing".to_string())?
                .to_string(),
        });
    }
    Ok(out)
}

fn parse_board_detail(value: &Value) -> Result<BoardDetailItem, String> {
    let board = value
        .get("board")
        .ok_or_else(|| "board_detail.board missing".to_string())?;
    let columns = value
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "board_detail.columns missing".to_string())?;
    let cards = value
        .get("cards")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "board_detail.cards missing".to_string())?;

    let mut parsed_columns = Vec::new();
    for column in columns {
        parsed_columns.push(BoardColumnItem {
            column_id: column
                .get("column_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_column.column_id missing".to_string())?
                .to_string(),
            column_key: column
                .get("column_key")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_column.column_key missing".to_string())?
                .to_string(),
            name: column
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_column.name missing".to_string())?
                .to_string(),
            position: column
                .get("position")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "board_column.position missing".to_string())?,
        });
    }

    let mut parsed_cards = Vec::new();
    for card in cards {
        let assets = card
            .get("assets")
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .map(|asset| {
                        Ok(BoardAssetItem {
                            card_asset_id: asset
                                .get("card_asset_id")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "board_asset.card_asset_id missing".to_string())?
                                .to_string(),
                            filename: asset
                                .get("filename")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "board_asset.filename missing".to_string())?
                                .to_string(),
                            mime: asset
                                .get("mime")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "board_asset.mime missing".to_string())?
                                .to_string(),
                            bytes: asset
                                .get("bytes")
                                .and_then(|v| v.as_i64())
                                .ok_or_else(|| "board_asset.bytes missing".to_string())?,
                        })
                    })
                    .collect::<Result<Vec<BoardAssetItem>, String>>()
            })
            .transpose()?
            .unwrap_or_default();

        parsed_cards.push(BoardCardItem {
            card_id: card
                .get("card_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_card.card_id missing".to_string())?
                .to_string(),
            column_id: card
                .get("column_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_card.column_id missing".to_string())?
                .to_string(),
            title: card
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_card.title missing".to_string())?
                .to_string(),
            description: card
                .get("description")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            owner_kind: card
                .get("owner_kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "board_card.owner_kind missing".to_string())?
                .to_string(),
            owner_agent_id: card
                .get("owner_agent_id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            script_markdown: card
                .get("script_markdown")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            linked_session_id: card
                .get("linked_session_id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            latest_run_id: card
                .get("latest_run_id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            position: card
                .get("position")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "board_card.position missing".to_string())?,
            assets,
        });
    }

    Ok(BoardDetailItem {
        board_id: board
            .get("board_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "board_detail.board.board_id missing".to_string())?
            .to_string(),
        board_key: board
            .get("board_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "board_detail.board.board_key missing".to_string())?
            .to_string(),
        board_name: board
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "board_detail.board.name missing".to_string())?
            .to_string(),
        columns: parsed_columns,
        cards: parsed_cards,
    })
}

fn parse_board_automation_rules(value: &Value) -> Result<Vec<BoardAutomationRuleItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "automation.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(BoardAutomationRuleItem {
            rule_id: item
                .get("rule_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.rule_id missing".to_string())?
                .to_string(),
            job_id: item
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.job_id missing".to_string())?
                .to_string(),
            board_id: item
                .get("board_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.board_id missing".to_string())?
                .to_string(),
            column_id: item
                .get("column_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.column_id missing".to_string())?
                .to_string(),
            target_column_id: item
                .get("target_column_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.target_column_id missing".to_string())?
                .to_string(),
            name: item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "automation.name missing".to_string())?
                .to_string(),
            enabled: item
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "automation.enabled missing".to_string())?,
            next_run_at: item.get("next_run_at").and_then(|v| v.as_i64()),
            max_cards_per_run: item
                .get("max_cards_per_run")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "automation.max_cards_per_run missing".to_string())?,
            max_runs_per_day: item
                .get("max_runs_per_day")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "automation.max_runs_per_day missing".to_string())?,
            max_attempts_per_card_per_day: item
                .get("max_attempts_per_card_per_day")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "automation.max_attempts_per_card_per_day missing".to_string())?,
            last_error: item
                .get("last_error")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
        });
    }
    Ok(out)
}

fn parse_calendar_jobs(value: &Value) -> Result<Vec<CalendarJobItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "jobs.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        out.push(CalendarJobItem {
            job_id: item
                .get("job_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job.job_id missing".to_string())?
                .to_string(),
            name: item
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job.name missing".to_string())?
                .to_string(),
            agent_id: item
                .get("agent_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job.agent_id missing".to_string())?
                .to_string(),
            enabled: item
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "job.enabled missing".to_string())?,
            next_run_at: item.get("next_run_at").and_then(|v| v.as_i64()),
            schedule_kind: item
                .get("schedule_kind")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "job.schedule_kind missing".to_string())?
                .to_string(),
        });
    }
    Ok(out)
}

fn parse_memory_notes(value: &Value) -> Result<Vec<MemoryNoteItem>, String> {
    let items = value
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "notes.items missing".to_string())?;

    let mut out = Vec::new();
    for item in items {
        let body = item
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "note.body missing".to_string())?;
        let body_preview = body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(220)
            .collect::<String>();

        out.push(MemoryNoteItem {
            note_id: item
                .get("note_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "note.note_id missing".to_string())?
                .to_string(),
            title: item
                .get("title")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            updated_at: item
                .get("updated_at")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "note.updated_at missing".to_string())?,
            body_preview,
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

fn parse_runtime_config(value: &Value) -> Result<RuntimeConfigWizardSnapshot, String> {
    let config = value
        .get("config")
        .ok_or_else(|| "runtime.config missing".to_string())?;
    let global = config
        .get("global")
        .ok_or_else(|| "runtime.config.global missing".to_string())?;
    let channels = config
        .get("channels")
        .ok_or_else(|| "runtime.config.channels missing".to_string())?;
    let discord = channels
        .get("discord")
        .ok_or_else(|| "runtime.config.channels.discord missing".to_string())?;
    let telegram = channels
        .get("telegram")
        .ok_or_else(|| "runtime.config.channels.telegram missing".to_string())?;
    let security = config
        .get("security")
        .ok_or_else(|| "runtime.config.security missing".to_string())?;

    let mut provider_policies = config
        .get("providers")
        .and_then(|value| value.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| RuntimeProviderPolicyDraft {
                    provider: row
                        .get("provider")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    enabled: row
                        .get("enabled")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(true),
                    allow_consumer_oauth: row
                        .get("allow_consumer_oauth")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false),
                    kill_switch_scope: row
                        .get("kill_switch_scope")
                        .and_then(|value| value.as_str())
                        .unwrap_or("none")
                        .to_string(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if provider_policies.is_empty() {
        provider_policies = RuntimeConfigWizardSnapshot::default().provider_policies;
    } else {
        normalize_provider_policies(&mut provider_policies);
    }

    Ok(RuntimeConfigWizardSnapshot {
        schema_version: config
            .get("schema_version")
            .and_then(|value| value.as_str())
            .unwrap_or(RUNTIME_SCHEMA_VERSION_V1)
            .to_string(),
        updated_at: config
            .get("updated_at")
            .and_then(|value| value.as_i64())
            .unwrap_or(0),
        jwt_issuer_allowlist_csv: join_string_csv(&parse_string_array(
            global.get("jwt_issuer_allowlist"),
        )),
        jwt_audience_allowlist_csv: join_string_csv(&parse_string_array(
            global.get("jwt_audience_allowlist"),
        )),
        trusted_proxy_allowlist_csv: join_string_csv(&parse_string_array(
            global.get("trusted_proxy_allowlist"),
        )),
        tls_termination_mode: global
            .get("tls_termination_mode")
            .and_then(|value| value.as_str())
            .unwrap_or("edge")
            .to_string(),
        public_base_url: parse_optional_string_from_json(global.get("public_base_url")),
        provider_policies,
        discord_operation_mode: discord
            .get("operation_mode")
            .and_then(|value| value.as_str())
            .unwrap_or("shim")
            .to_string(),
        discord_bot_token_secret_ref: parse_optional_string_from_json(
            discord.get("bot_token_secret_ref"),
        ),
        discord_application_id: parse_optional_string_from_json(discord.get("application_id")),
        discord_intents_csv: join_string_csv(&parse_string_array(discord.get("intents"))),
        discord_staging_guild_ids_csv: join_string_csv(&parse_string_array(
            discord.get("staging_guild_ids"),
        )),
        discord_staging_channel_ids_csv: join_string_csv(&parse_string_array(
            discord.get("staging_channel_ids"),
        )),
        telegram_bot_token_secret_ref: parse_optional_string_from_json(
            telegram.get("bot_token_secret_ref"),
        ),
        telegram_operation_mode: telegram
            .get("operation_mode")
            .and_then(|value| value.as_str())
            .unwrap_or("shim")
            .to_string(),
        telegram_webhook_mode: telegram
            .get("webhook_mode")
            .and_then(|value| value.as_str())
            .unwrap_or("long_poll")
            .to_string(),
        telegram_webhook_url: parse_optional_string_from_json(telegram.get("webhook_url")),
        telegram_staging_chat_ids_csv: join_i64_csv(&parse_i64_array(
            telegram.get("staging_chat_ids"),
        )),
        threat_model_approver: parse_optional_string_from_json(
            security.get("threat_model_approver"),
        ),
        risk_acceptance_owner: parse_optional_string_from_json(
            security.get("risk_acceptance_owner"),
        ),
        incident_primary: parse_optional_string_from_json(security.get("incident_primary")),
        incident_backup: parse_optional_string_from_json(security.get("incident_backup")),
        audit_archive_target: parse_optional_string_from_json(security.get("audit_archive_target")),
        audit_archive_encryption: parse_optional_string_from_json(
            security.get("audit_archive_encryption"),
        ),
        audit_hot_retention_days: security
            .get("audit_hot_retention_days")
            .and_then(|value| value.as_i64())
            .unwrap_or(90)
            .to_string(),
        audit_archive_retention_days: security
            .get("audit_archive_retention_days")
            .and_then(|value| value.as_i64())
            .unwrap_or(365)
            .to_string(),
    })
}

fn parse_optional_string_from_json(value: Option<&Value>) -> String {
    value
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn parse_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(|item| item.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn parse_i64_array(value: Option<&Value>) -> Vec<i64> {
    value
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_i64())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn optional_string_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else {
        Value::String(trimmed.to_string())
    }
}

fn parse_i64_field(field_name: &str, raw: &str) -> Result<i64, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{field_name} must not be empty"));
    }
    trimmed
        .parse::<i64>()
        .map_err(|_| format!("{field_name} must be an i64 integer"))
}

fn validate_secret_ref_format(field_name: &str, value: &str) -> Result<(), String> {
    let trimmed = value.trim();
    if !trimmed.is_empty() && !trimmed.starts_with("secret://") {
        return Err(format!(
            "{field_name} must use secret:// reference format when set"
        ));
    }
    Ok(())
}

fn normalize_provider_policies(policies: &mut Vec<RuntimeProviderPolicyDraft>) {
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for policy in policies.iter() {
        let provider = policy.provider.trim().to_ascii_lowercase();
        if provider.is_empty() {
            continue;
        }
        if !seen.insert(provider.clone()) {
            continue;
        }
        let mut kill_switch_scope = policy.kill_switch_scope.trim().to_ascii_lowercase();
        if !KILL_SWITCH_SCOPES.contains(&kill_switch_scope.as_str()) {
            kill_switch_scope = "none".to_string();
        }
        normalized.push(RuntimeProviderPolicyDraft {
            provider,
            enabled: policy.enabled,
            allow_consumer_oauth: policy.allow_consumer_oauth,
            kill_switch_scope,
        });
    }

    for fallback in ["openai", "anthropic"] {
        if seen.insert(fallback.to_string()) {
            normalized.push(RuntimeProviderPolicyDraft {
                provider: fallback.to_string(),
                enabled: true,
                allow_consumer_oauth: false,
                kill_switch_scope: "none".to_string(),
            });
        }
    }

    normalized.sort_by(|left, right| left.provider.cmp(&right.provider));
    *policies = normalized;
}

fn validate_runtime_config_draft(config: &RuntimeConfigWizardSnapshot) -> Result<(), String> {
    if config.schema_version.trim() != RUNTIME_SCHEMA_VERSION_V1 {
        return Err(format!(
            "unsupported schema_version {} (expected {})",
            config.schema_version, RUNTIME_SCHEMA_VERSION_V1
        ));
    }
    let tls_mode = config.tls_termination_mode.trim().to_ascii_lowercase();
    if !TLS_TERMINATION_MODES.contains(&tls_mode.as_str()) {
        return Err("global.tls_termination_mode must be edge|gateway|passthrough".to_string());
    }

    if config.provider_policies.is_empty() {
        return Err("providers must include at least one provider policy".to_string());
    }
    let mut seen_provider_ids = HashSet::new();
    for provider in &config.provider_policies {
        let provider_id = provider.provider.trim().to_ascii_lowercase();
        if provider_id.is_empty() {
            return Err("provider policy contains empty provider id".to_string());
        }
        if !seen_provider_ids.insert(provider_id.clone()) {
            return Err(format!("provider policy duplicated for {}", provider_id));
        }
        let scope = provider.kill_switch_scope.trim().to_ascii_lowercase();
        if !KILL_SWITCH_SCOPES.contains(&scope.as_str()) {
            return Err(
                "provider kill_switch_scope must be none|profile|provider|global".to_string(),
            );
        }
    }

    validate_secret_ref_format(
        "channels.discord.bot_token_secret_ref",
        &config.discord_bot_token_secret_ref,
    )?;
    let discord_operation_mode = config.discord_operation_mode.trim().to_ascii_lowercase();
    if !CHANNEL_OPERATION_MODES.contains(&discord_operation_mode.as_str()) {
        return Err("channels.discord.operation_mode must be shim|transport".to_string());
    }
    if discord_operation_mode == "transport"
        && config.discord_bot_token_secret_ref.trim().is_empty()
    {
        return Err(
            "channels.discord.bot_token_secret_ref is required when operation_mode=transport"
                .to_string(),
        );
    }
    validate_secret_ref_format(
        "channels.telegram.bot_token_secret_ref",
        &config.telegram_bot_token_secret_ref,
    )?;
    let telegram_operation_mode = config.telegram_operation_mode.trim().to_ascii_lowercase();
    if !CHANNEL_OPERATION_MODES.contains(&telegram_operation_mode.as_str()) {
        return Err("channels.telegram.operation_mode must be shim|transport".to_string());
    }
    if telegram_operation_mode == "transport"
        && config.telegram_bot_token_secret_ref.trim().is_empty()
    {
        return Err(
            "channels.telegram.bot_token_secret_ref is required when operation_mode=transport"
                .to_string(),
        );
    }

    let webhook_mode = config.telegram_webhook_mode.trim().to_ascii_lowercase();
    if !TELEGRAM_WEBHOOK_MODES.contains(&webhook_mode.as_str()) {
        return Err("channels.telegram.webhook_mode must be long_poll|webhook".to_string());
    }
    if webhook_mode == "webhook" && config.telegram_webhook_url.trim().is_empty() {
        return Err(
            "channels.telegram.webhook_url is required when webhook_mode=webhook".to_string(),
        );
    }
    parse_i64_csv(&config.telegram_staging_chat_ids_csv)?;

    let hot_retention = parse_i64_field(
        "security.audit_hot_retention_days",
        &config.audit_hot_retention_days,
    )?;
    let archive_retention = parse_i64_field(
        "security.audit_archive_retention_days",
        &config.audit_archive_retention_days,
    )?;
    if hot_retention < 90 {
        return Err("security.audit_hot_retention_days must be >= 90".to_string());
    }
    if archive_retention < hot_retention {
        return Err(
            "security.audit_archive_retention_days must be >= security.audit_hot_retention_days"
                .to_string(),
        );
    }

    Ok(())
}

fn runtime_wizard_completeness_issues(config: &RuntimeConfigWizardSnapshot) -> Vec<String> {
    let mut issues = Vec::new();

    if parse_string_csv(&config.jwt_issuer_allowlist_csv).is_empty() {
        issues.push("global.jwt_issuer_allowlist is empty".to_string());
    }
    if parse_string_csv(&config.jwt_audience_allowlist_csv).is_empty() {
        issues.push("global.jwt_audience_allowlist is empty".to_string());
    }
    if parse_string_csv(&config.trusted_proxy_allowlist_csv).is_empty() {
        issues.push("global.trusted_proxy_allowlist is empty".to_string());
    }
    if config.public_base_url.trim().is_empty() {
        issues.push("global.public_base_url is empty".to_string());
    }
    let discord_mode = config.discord_operation_mode.trim().to_ascii_lowercase();
    if !CHANNEL_OPERATION_MODES.contains(&discord_mode.as_str()) {
        issues.push("channels.discord.operation_mode is invalid".to_string());
    }
    let telegram_mode = config.telegram_operation_mode.trim().to_ascii_lowercase();
    if !CHANNEL_OPERATION_MODES.contains(&telegram_mode.as_str()) {
        issues.push("channels.telegram.operation_mode is invalid".to_string());
    }
    if config
        .discord_operation_mode
        .trim()
        .eq_ignore_ascii_case("transport")
        && config.discord_bot_token_secret_ref.trim().is_empty()
    {
        issues.push("channels.discord.bot_token_secret_ref is empty (transport mode)".to_string());
    }
    if config
        .telegram_operation_mode
        .trim()
        .eq_ignore_ascii_case("transport")
        && config.telegram_bot_token_secret_ref.trim().is_empty()
    {
        issues.push("channels.telegram.bot_token_secret_ref is empty (transport mode)".to_string());
    }

    for (field_name, value) in [
        (
            "security.threat_model_approver",
            &config.threat_model_approver,
        ),
        (
            "security.risk_acceptance_owner",
            &config.risk_acceptance_owner,
        ),
        ("security.incident_primary", &config.incident_primary),
        ("security.incident_backup", &config.incident_backup),
        (
            "security.audit_archive_target",
            &config.audit_archive_target,
        ),
        (
            "security.audit_archive_encryption",
            &config.audit_archive_encryption,
        ),
    ] {
        if value.trim().is_empty() {
            issues.push(format!("{field_name} is empty"));
        }
    }

    if !config
        .provider_policies
        .iter()
        .any(|provider| provider.enabled)
    {
        issues.push("no provider policy is enabled".to_string());
    }

    if let Err(err) = parse_i64_field(
        "security.audit_hot_retention_days",
        &config.audit_hot_retention_days,
    ) {
        issues.push(err);
    }
    if let Err(err) = parse_i64_field(
        "security.audit_archive_retention_days",
        &config.audit_archive_retention_days,
    ) {
        issues.push(err);
    }

    issues
}

fn runtime_config_update_payload(config: &RuntimeConfigWizardSnapshot) -> Result<Value, String> {
    let mut providers = config.provider_policies.clone();
    normalize_provider_policies(&mut providers);
    let staging_chat_ids = parse_i64_csv(&config.telegram_staging_chat_ids_csv)?;
    let hot_retention = parse_i64_field(
        "security.audit_hot_retention_days",
        &config.audit_hot_retention_days,
    )?;
    let archive_retention = parse_i64_field(
        "security.audit_archive_retention_days",
        &config.audit_archive_retention_days,
    )?;

    Ok(json!({
        "global": {
            "jwt_issuer_allowlist": parse_string_csv(&config.jwt_issuer_allowlist_csv),
            "jwt_audience_allowlist": parse_string_csv(&config.jwt_audience_allowlist_csv),
            "trusted_proxy_allowlist": parse_string_csv(&config.trusted_proxy_allowlist_csv),
            "tls_termination_mode": config.tls_termination_mode.trim().to_ascii_lowercase(),
            "public_base_url": optional_string_value(&config.public_base_url),
        },
        "providers": providers.iter().map(|provider| {
            json!({
                "provider": provider.provider.trim().to_ascii_lowercase(),
                "enabled": provider.enabled,
                "allow_consumer_oauth": provider.allow_consumer_oauth,
                "kill_switch_scope": provider.kill_switch_scope.trim().to_ascii_lowercase(),
            })
        }).collect::<Vec<_>>(),
        "channels": {
            "discord": {
                "operation_mode": config.discord_operation_mode.trim().to_ascii_lowercase(),
                "bot_token_secret_ref": optional_string_value(&config.discord_bot_token_secret_ref),
                "application_id": optional_string_value(&config.discord_application_id),
                "intents": parse_string_csv(&config.discord_intents_csv),
                "staging_guild_ids": parse_string_csv(&config.discord_staging_guild_ids_csv),
                "staging_channel_ids": parse_string_csv(&config.discord_staging_channel_ids_csv),
            },
            "telegram": {
                "operation_mode": config.telegram_operation_mode.trim().to_ascii_lowercase(),
                "bot_token_secret_ref": optional_string_value(&config.telegram_bot_token_secret_ref),
                "webhook_mode": config.telegram_webhook_mode.trim().to_ascii_lowercase(),
                "webhook_url": optional_string_value(&config.telegram_webhook_url),
                "staging_chat_ids": staging_chat_ids,
            },
        },
        "security": {
            "threat_model_approver": optional_string_value(&config.threat_model_approver),
            "risk_acceptance_owner": optional_string_value(&config.risk_acceptance_owner),
            "incident_primary": optional_string_value(&config.incident_primary),
            "incident_backup": optional_string_value(&config.incident_backup),
            "audit_archive_target": optional_string_value(&config.audit_archive_target),
            "audit_archive_encryption": optional_string_value(&config.audit_archive_encryption),
            "audit_hot_retention_days": hot_retention,
            "audit_archive_retention_days": archive_retention,
        }
    }))
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
    fn parse_team_agents_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "agent_id": "lyra",
                    "name": "Lyra",
                    "model_provider": "openai",
                    "model_id": "gpt-4.1",
                    "tool_profile": "default"
                },
                {
                    "agent_id": "claude",
                    "name": "Claude",
                    "model_provider": "anthropic",
                    "model_id": "claude-3-7-sonnet",
                    "tool_profile": "default"
                }
            ]
        });
        let agents = parse_team_agents(&value).expect("team agents parse");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].agent_id, "lyra");
        assert_eq!(agents[1].agent_id, "claude");
    }

    #[test]
    fn parse_board_summaries_and_detail_success() {
        let summaries = serde_json::json!({
            "items": [
                {
                    "board_id": "b1",
                    "board_key": "tasks",
                    "name": "Tasks"
                }
            ]
        });
        let parsed_summaries = parse_board_summaries(&summaries).expect("board summaries parse");
        assert_eq!(parsed_summaries.len(), 1);
        assert_eq!(parsed_summaries[0].board_key, "tasks");

        let detail = serde_json::json!({
            "board": {
                "board_id": "b1",
                "board_key": "tasks",
                "name": "Tasks"
            },
            "columns": [
                {
                    "column_id": "c1",
                    "column_key": "backlog",
                    "name": "Backlog",
                    "position": 10
                }
            ],
            "cards": [
                {
                    "card_id": "card1",
                    "column_id": "c1",
                    "title": "Implement gate",
                    "description": "Details",
                    "owner_kind": "agent",
                    "owner_agent_id": "lyra",
                    "script_markdown": "Run tests",
                    "linked_session_id": "s1",
                    "latest_run_id": "r1",
                    "position": 10,
                    "assets": [
                        {
                            "card_asset_id": "a1",
                            "filename": "brief.md",
                            "mime": "text/markdown",
                            "bytes": 64
                        }
                    ]
                }
            ]
        });
        let parsed_detail = parse_board_detail(&detail).expect("board detail parse");
        assert_eq!(parsed_detail.columns.len(), 1);
        assert_eq!(parsed_detail.cards.len(), 1);
        assert_eq!(parsed_detail.cards[0].assets.len(), 1);
        assert_eq!(parsed_detail.cards[0].title, "Implement gate");
    }

    #[test]
    fn parse_board_automation_rules_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "rule_id": "r1",
                    "job_id": "j1",
                    "board_id": "b1",
                    "column_id": "c1",
                    "target_column_id": "c2",
                    "name": "Script -> Thumbnail",
                    "enabled": true,
                    "next_run_at": 1000,
                    "max_cards_per_run": 2,
                    "max_runs_per_day": 24,
                    "max_attempts_per_card_per_day": 2,
                    "last_error": null
                }
            ]
        });
        let rules = parse_board_automation_rules(&value).expect("automation rules parse");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].job_id, "j1");
        assert!(rules[0].enabled);
    }

    #[test]
    fn parse_calendar_jobs_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "job_id": "job-1",
                    "name": "nightly",
                    "agent_id": "lyra",
                    "enabled": true,
                    "next_run_at": 1000,
                    "schedule_kind": "interval"
                }
            ]
        });
        let jobs = parse_calendar_jobs(&value).expect("calendar jobs parse");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_id, "job-1");
        assert!(jobs[0].enabled);
    }

    #[test]
    fn parse_memory_notes_success() {
        let value = serde_json::json!({
            "items": [
                {
                    "note_id": "n1",
                    "title": "Roadmap",
                    "body": "line one\nline two",
                    "updated_at": 44
                }
            ]
        });
        let notes = parse_memory_notes(&value).expect("memory notes parse");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].note_id, "n1");
        assert_eq!(notes[0].updated_at, 44);
        assert_eq!(notes[0].body_preview, "line one line two");
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
    fn parse_runtime_config_success() {
        let value = serde_json::json!({
            "config": {
                "schema_version": "runtime.config.v1",
                "updated_at": 33,
                "global": {
                    "jwt_issuer_allowlist": ["issuer-a"],
                    "jwt_audience_allowlist": ["aud-a", "aud-b"],
                    "trusted_proxy_allowlist": ["10.0.0.0/8"],
                    "tls_termination_mode": "edge",
                    "public_base_url": "https://ops.example.com"
                },
                "providers": [
                    {
                        "provider": "openai",
                        "enabled": true,
                        "allow_consumer_oauth": false,
                        "kill_switch_scope": "none"
                    }
                ],
                "channels": {
                    "discord": {
                        "bot_token_secret_ref": "secret://runtime.discord.bot_token",
                        "application_id": "123",
                        "intents": ["guilds", "direct_messages"],
                        "staging_guild_ids": ["g1"],
                        "staging_channel_ids": ["c1"]
                    },
                    "telegram": {
                        "bot_token_secret_ref": "secret://runtime.telegram.bot_token",
                        "webhook_mode": "long_poll",
                        "webhook_url": null,
                        "staging_chat_ids": [1001, 1002]
                    }
                },
                "security": {
                    "threat_model_approver": "owner-a",
                    "risk_acceptance_owner": "owner-b",
                    "incident_primary": "p1",
                    "incident_backup": "p2",
                    "audit_archive_target": "s3://archive",
                    "audit_archive_encryption": "kms://key",
                    "audit_hot_retention_days": 90,
                    "audit_archive_retention_days": 365
                }
            }
        });

        let parsed = parse_runtime_config(&value).expect("runtime config parse");
        assert_eq!(parsed.schema_version, "runtime.config.v1");
        assert_eq!(parsed.updated_at, 33);
        assert_eq!(parsed.jwt_audience_allowlist_csv, "aud-a,aud-b");
        assert_eq!(parsed.discord_operation_mode, "shim");
        assert_eq!(parsed.telegram_operation_mode, "shim");
        assert_eq!(parsed.discord_intents_csv, "guilds,direct_messages");
        assert_eq!(parsed.telegram_staging_chat_ids_csv, "1001,1002");
        assert_eq!(parsed.provider_policies.len(), 2);
    }

    #[test]
    fn validate_runtime_config_draft_rejects_invalid_webhook_mode() {
        let config = RuntimeConfigWizardSnapshot {
            telegram_webhook_mode: "invalid-mode".to_string(),
            ..RuntimeConfigWizardSnapshot::default()
        };
        let error = validate_runtime_config_draft(&config).expect_err("expected mode failure");
        assert!(error.contains("channels.telegram.webhook_mode"));
    }

    #[test]
    fn runtime_wizard_completeness_reports_missing_production_inputs() {
        let config = RuntimeConfigWizardSnapshot::default();
        let issues = runtime_wizard_completeness_issues(&config);
        assert!(issues
            .iter()
            .any(|entry| entry.contains("jwt_issuer_allowlist")));
        assert!(issues
            .iter()
            .any(|entry| entry.contains("threat_model_approver")));
    }

    #[test]
    fn runtime_wizard_completeness_requires_bot_secret_refs_in_transport_mode() {
        let config = RuntimeConfigWizardSnapshot {
            discord_operation_mode: "transport".to_string(),
            telegram_operation_mode: "transport".to_string(),
            ..Default::default()
        };
        let issues = runtime_wizard_completeness_issues(&config);
        assert!(issues.iter().any(|entry| {
            entry.contains("channels.discord.bot_token_secret_ref is empty (transport mode)")
        }));
        assert!(issues.iter().any(|entry| {
            entry.contains("channels.telegram.bot_token_secret_ref is empty (transport mode)")
        }));
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
    fn generate_local_gateway_token_uses_expected_format() {
        let token_a = generate_local_gateway_token();
        let token_b = generate_local_gateway_token();
        assert!(token_a.starts_with("carsinos-local-"));
        assert!(token_b.starts_with("carsinos-local-"));
        assert!(token_a.len() > "carsinos-local-".len());
        assert!(token_b.len() > "carsinos-local-".len());
        assert_ne!(token_a, token_b);
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
