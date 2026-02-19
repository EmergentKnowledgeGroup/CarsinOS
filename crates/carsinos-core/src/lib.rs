use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Env,
    Generated,
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub bind: SocketAddr,
    pub token: String,
    pub token_source: TokenSource,
    pub state_dir: PathBuf,
}

impl GatewayConfig {
    pub fn load_from_env() -> Result<Self> {
        let bind = std::env::var("CARSINOS_GATEWAY_BIND")
            .unwrap_or_else(|_| "127.0.0.1:18789".to_string());
        let bind = SocketAddr::from_str(&bind)
            .with_context(|| format!("invalid CARSINOS_GATEWAY_BIND: {bind}"))?;

        let (token, token_source) = match std::env::var("CARSINOS_GATEWAY_TOKEN") {
            Ok(token) if !token.trim().is_empty() => (token, TokenSource::Env),
            _ => (uuid::Uuid::new_v4().to_string(), TokenSource::Generated),
        };

        let state_dir = match std::env::var("CARSINOS_STATE_DIR") {
            Ok(path) if !path.trim().is_empty() => PathBuf::from(path),
            _ => default_state_dir()?,
        };

        Ok(Self {
            bind,
            token,
            token_source,
            state_dir,
        })
    }
}

fn default_state_dir() -> Result<PathBuf> {
    let dirs = ProjectDirs::from("com", "carsinos", "carsinos")
        .context("failed to resolve app data directory")?;
    Ok(dirs.data_local_dir().to_path_buf())
}

pub const PLUGIN_MANIFEST_SCHEMA_VERSION_V1: &str = "carsinos.plugin.manifest.v1";
pub const PLUGIN_API_VERSION_V1: &str = "carsinos.plugin.api.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    #[serde(default = "default_plugin_manifest_schema_version")]
    pub schema_version: String,
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    #[serde(default = "default_plugin_api_version")]
    pub api_version: String,
    #[serde(default = "default_plugin_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub tools: Vec<PluginCapability>,
    #[serde(default)]
    pub hooks: Vec<PluginCapability>,
    #[serde(default)]
    pub providers: Vec<PluginCapability>,
    #[serde(default)]
    pub channels: Vec<PluginCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCapability {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginBindingKind {
    Tool,
    Hook,
    Provider,
    Channel,
}

impl PluginBindingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Hook => "hook",
            Self::Provider => "provider",
            Self::Channel => "channel",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginRegistry {
    manifests: BTreeMap<String, PluginManifest>,
    tool_bindings: BTreeMap<String, String>,
    hook_bindings: BTreeMap<String, String>,
    provider_bindings: BTreeMap<String, String>,
    channel_bindings: BTreeMap<String, String>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from_dirs(directories: &[PathBuf]) -> Result<Self> {
        let mut registry = Self::new();
        for directory in directories {
            let manifests = load_manifests_from_dir(directory).with_context(|| {
                format!(
                    "failed to load plugin manifests from directory {}",
                    directory.display()
                )
            })?;
            for manifest in manifests {
                let plugin_id = manifest.plugin_id.clone();
                registry.register_manifest(manifest).with_context(|| {
                    format!("failed to register plugin manifest for plugin_id={plugin_id}")
                })?;
            }
        }
        Ok(registry)
    }

    pub fn register_manifest(&mut self, mut manifest: PluginManifest) -> Result<()> {
        if manifest.schema_version.trim() != PLUGIN_MANIFEST_SCHEMA_VERSION_V1 {
            bail!(
                "unsupported plugin manifest schema_version: {}",
                manifest.schema_version.trim()
            );
        }
        if manifest.api_version.trim() != PLUGIN_API_VERSION_V1 {
            bail!(
                "unsupported plugin api_version: {}",
                manifest.api_version.trim()
            );
        }

        let plugin_id = normalize_identifier(&manifest.plugin_id)
            .context("plugin_id must be non-empty and use [a-z0-9._-] characters")?;
        if manifest.display_name.trim().is_empty() {
            bail!("display_name cannot be empty");
        }
        if manifest.plugin_version.trim().is_empty() {
            bail!("plugin_version cannot be empty");
        }
        if self.manifests.contains_key(&plugin_id) {
            bail!("plugin_id already registered: {plugin_id}");
        }

        let mut local_tool_names = BTreeSet::new();
        let mut local_hook_names = BTreeSet::new();
        let mut local_provider_names = BTreeSet::new();
        let mut local_channel_names = BTreeSet::new();

        normalize_capabilities(
            &mut manifest.capabilities.tools,
            PluginBindingKind::Tool,
            &mut local_tool_names,
        )
        .context("invalid tool capability declaration")?;
        normalize_capabilities(
            &mut manifest.capabilities.hooks,
            PluginBindingKind::Hook,
            &mut local_hook_names,
        )
        .context("invalid hook capability declaration")?;
        normalize_capabilities(
            &mut manifest.capabilities.providers,
            PluginBindingKind::Provider,
            &mut local_provider_names,
        )
        .context("invalid provider capability declaration")?;
        normalize_capabilities(
            &mut manifest.capabilities.channels,
            PluginBindingKind::Channel,
            &mut local_channel_names,
        )
        .context("invalid channel capability declaration")?;

        register_bindings(
            &mut self.tool_bindings,
            &plugin_id,
            PluginBindingKind::Tool,
            &manifest.capabilities.tools,
        )?;
        register_bindings(
            &mut self.hook_bindings,
            &plugin_id,
            PluginBindingKind::Hook,
            &manifest.capabilities.hooks,
        )?;
        register_bindings(
            &mut self.provider_bindings,
            &plugin_id,
            PluginBindingKind::Provider,
            &manifest.capabilities.providers,
        )?;
        register_bindings(
            &mut self.channel_bindings,
            &plugin_id,
            PluginBindingKind::Channel,
            &manifest.capabilities.channels,
        )?;

        manifest.plugin_id = plugin_id.clone();
        manifest.display_name = manifest.display_name.trim().to_string();
        manifest.plugin_version = manifest.plugin_version.trim().to_string();
        manifest.schema_version = PLUGIN_MANIFEST_SCHEMA_VERSION_V1.to_string();
        manifest.api_version = PLUGIN_API_VERSION_V1.to_string();
        self.manifests.insert(plugin_id, manifest);
        Ok(())
    }

    pub fn list_manifests(&self) -> Vec<PluginManifest> {
        self.manifests.values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    pub fn capability_owner(&self, kind: PluginBindingKind, name: &str) -> Option<&str> {
        let normalized = normalize_identifier(name).ok()?;
        match kind {
            PluginBindingKind::Tool => self.tool_bindings.get(&normalized).map(String::as_str),
            PluginBindingKind::Hook => self.hook_bindings.get(&normalized).map(String::as_str),
            PluginBindingKind::Provider => {
                self.provider_bindings.get(&normalized).map(String::as_str)
            }
            PluginBindingKind::Channel => {
                self.channel_bindings.get(&normalized).map(String::as_str)
            }
        }
    }
}

pub type HookHandler = Arc<dyn Fn(&HookEvent) -> Result<()> + Send + Sync + 'static>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    RunStart,
    RunEnd,
    ToolBefore,
    ToolAfter,
    CompactionBefore,
    CompactionAfter,
}

impl HookPoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RunStart => "run_start",
            Self::RunEnd => "run_end",
            Self::ToolBefore => "tool_before",
            Self::ToolAfter => "tool_after",
            Self::CompactionBefore => "compaction_before",
            Self::CompactionAfter => "compaction_after",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HookEvent {
    pub hook_point: HookPoint,
    pub run_id: String,
    pub session_id: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Clone)]
pub struct HookRegistration {
    pub hook_id: String,
    pub plugin_id: String,
    pub hook_point: HookPoint,
    pub priority: i32,
    pub handler: HookHandler,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookRegistrationInfo {
    pub hook_id: String,
    pub plugin_id: String,
    pub hook_point: HookPoint,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookInvocationResult {
    pub hook_id: String,
    pub plugin_id: String,
    pub hook_point: HookPoint,
    pub status: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

#[derive(Clone, Default)]
pub struct HookBus {
    registrations: Arc<RwLock<Vec<HookRegistration>>>,
}

impl HookBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, mut registration: HookRegistration) -> Result<()> {
        let hook_id = normalize_identifier(&registration.hook_id)
            .context("hook_id must be non-empty and use [a-z0-9._-] characters")?;
        let plugin_id = normalize_identifier(&registration.plugin_id)
            .context("plugin_id must be non-empty and use [a-z0-9._-] characters")?;
        registration.hook_id = hook_id.clone();
        registration.plugin_id = plugin_id;

        let mut guard = self
            .registrations
            .write()
            .map_err(|_| anyhow::anyhow!("hook bus lock poisoned"))?;
        if guard.iter().any(|existing| existing.hook_id == hook_id) {
            bail!("hook_id already registered: {hook_id}");
        }

        guard.push(registration);
        guard.sort_by(|left, right| {
            left.hook_point
                .cmp(&right.hook_point)
                .then_with(|| right.priority.cmp(&left.priority))
                .then_with(|| left.hook_id.cmp(&right.hook_id))
        });
        Ok(())
    }

    pub fn list_registrations(&self) -> Vec<HookRegistrationInfo> {
        let guard = match self.registrations.read() {
            Ok(guard) => guard,
            Err(_) => return Vec::new(),
        };
        guard
            .iter()
            .map(|registration| HookRegistrationInfo {
                hook_id: registration.hook_id.clone(),
                plugin_id: registration.plugin_id.clone(),
                hook_point: registration.hook_point,
                priority: registration.priority,
            })
            .collect()
    }

    pub fn emit(&self, event: &HookEvent) -> Vec<HookInvocationResult> {
        let handlers = match self.registrations.read() {
            Ok(guard) => guard
                .iter()
                .filter(|registration| registration.hook_point == event.hook_point)
                .cloned()
                .collect::<Vec<_>>(),
            Err(_) => {
                return vec![HookInvocationResult {
                    hook_id: "hookbus.lock".to_string(),
                    plugin_id: "system".to_string(),
                    hook_point: event.hook_point,
                    status: "error".to_string(),
                    error: Some("hook bus lock poisoned".to_string()),
                    duration_ms: 0,
                }];
            }
        };

        let mut results = Vec::with_capacity(handlers.len());
        for registration in handlers {
            let started = Instant::now();
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (registration.handler)(event)
            }));
            let (status, error) = match outcome {
                Ok(Ok(())) => ("ok".to_string(), None),
                Ok(Err(err)) => ("error".to_string(), Some(err.to_string())),
                Err(_) => ("panic".to_string(), Some("hook panicked".to_string())),
            };
            results.push(HookInvocationResult {
                hook_id: registration.hook_id.clone(),
                plugin_id: registration.plugin_id.clone(),
                hook_point: registration.hook_point,
                status,
                error,
                duration_ms: started.elapsed().as_millis() as u64,
            });
        }
        results
    }
}

fn default_plugin_manifest_schema_version() -> String {
    PLUGIN_MANIFEST_SCHEMA_VERSION_V1.to_string()
}

fn default_plugin_api_version() -> String {
    PLUGIN_API_VERSION_V1.to_string()
}

fn default_plugin_enabled() -> bool {
    true
}

fn normalize_identifier(raw: &str) -> Result<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        bail!("identifier cannot be empty");
    }
    if !normalized
        .chars()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        bail!("identifier contains unsupported characters");
    }
    Ok(normalized)
}

fn normalize_capabilities(
    capabilities: &mut [PluginCapability],
    kind: PluginBindingKind,
    local_seen: &mut BTreeSet<String>,
) -> Result<()> {
    for capability in capabilities.iter_mut() {
        let name = normalize_identifier(&capability.name).with_context(|| {
            format!(
                "{} capability name must be non-empty and use [a-z0-9._-]",
                kind.as_str()
            )
        })?;
        if !local_seen.insert(name.clone()) {
            bail!(
                "duplicate {} capability name in plugin manifest: {}",
                kind.as_str(),
                name
            );
        }
        capability.name = name;
        capability.description = capability
            .description
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }
    Ok(())
}

fn register_bindings(
    bindings: &mut BTreeMap<String, String>,
    plugin_id: &str,
    kind: PluginBindingKind,
    capabilities: &[PluginCapability],
) -> Result<()> {
    for capability in capabilities {
        if let Some(owner) = bindings.get(&capability.name) {
            bail!(
                "{} capability '{}' is already registered by plugin '{}'",
                kind.as_str(),
                capability.name,
                owner
            );
        }
        bindings.insert(capability.name.clone(), plugin_id.to_string());
    }
    Ok(())
}

fn load_manifests_from_dir(directory: &Path) -> Result<Vec<PluginManifest>> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    if !directory.is_dir() {
        bail!(
            "plugin manifest path is not a directory: {}",
            directory.display()
        );
    }

    let mut entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read plugin directory {}", directory.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "failed to enumerate plugin directory {}",
                directory.display()
            )
        })?;
    entries.sort_by_key(|entry| entry.path());

    let mut manifests = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed reading plugin manifest {}", path.display()))?;
        let manifest: PluginManifest = serde_json::from_str(&raw)
            .with_context(|| format!("failed parsing plugin manifest {}", path.display()))?;
        manifests.push(manifest);
    }
    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn sample_manifest(plugin_id: &str) -> PluginManifest {
        PluginManifest {
            schema_version: PLUGIN_MANIFEST_SCHEMA_VERSION_V1.to_string(),
            plugin_id: plugin_id.to_string(),
            display_name: "Sample Plugin".to_string(),
            plugin_version: "1.0.0".to_string(),
            api_version: PLUGIN_API_VERSION_V1.to_string(),
            enabled: true,
            capabilities: PluginCapabilities {
                tools: vec![PluginCapability {
                    name: "tool.alpha".to_string(),
                    description: Some("sample tool".to_string()),
                }],
                hooks: vec![PluginCapability {
                    name: "hook.pre_run".to_string(),
                    description: None,
                }],
                providers: vec![],
                channels: vec![],
            },
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn register_manifest_rejects_unsupported_versions() {
        let mut registry = PluginRegistry::new();
        let mut manifest = sample_manifest("plugin.alpha");
        manifest.api_version = "carsinos.plugin.api.v2".to_string();
        assert!(registry.register_manifest(manifest).is_err());
    }

    #[test]
    fn register_manifest_rejects_duplicate_capability_owners() {
        let mut registry = PluginRegistry::new();
        registry
            .register_manifest(sample_manifest("plugin.alpha"))
            .expect("register first plugin");

        let mut duplicate = sample_manifest("plugin.beta");
        duplicate.capabilities.hooks.clear();
        duplicate.capabilities.tools = vec![PluginCapability {
            name: "tool.alpha".to_string(),
            description: Some("duplicate".to_string()),
        }];
        assert!(registry.register_manifest(duplicate).is_err());
    }

    #[test]
    fn load_registry_from_manifest_dir() {
        let root = std::env::temp_dir().join(format!("carsinos-plugin-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temp plugin dir");
        let manifest_path = root.join("plugin-alpha.json");
        let non_json_path = root.join("README.txt");

        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&sample_manifest("plugin.alpha"))
                .expect("serialize manifest"),
        )
        .expect("write manifest");
        fs::write(&non_json_path, "ignore").expect("write non json");

        let registry = PluginRegistry::load_from_dirs(std::slice::from_ref(&root))
            .expect("load plugin registry");
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.capability_owner(PluginBindingKind::Tool, "tool.alpha"),
            Some("plugin.alpha")
        );

        fs::remove_dir_all(&root).expect("cleanup temp plugin dir");
    }

    #[test]
    fn hook_bus_orders_by_priority_and_isolates_failures() {
        let bus = HookBus::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        let order_high = Arc::clone(&order);
        bus.register(HookRegistration {
            hook_id: "hook.high".to_string(),
            plugin_id: "plugin.alpha".to_string(),
            hook_point: HookPoint::RunStart,
            priority: 100,
            handler: Arc::new(move |_| {
                order_high
                    .lock()
                    .expect("lock order")
                    .push("hook.high".to_string());
                Ok(())
            }),
        })
        .expect("register high");

        let order_low = Arc::clone(&order);
        bus.register(HookRegistration {
            hook_id: "hook.low".to_string(),
            plugin_id: "plugin.beta".to_string(),
            hook_point: HookPoint::RunStart,
            priority: 10,
            handler: Arc::new(move |_| {
                order_low
                    .lock()
                    .expect("lock order")
                    .push("hook.low".to_string());
                anyhow::bail!("intentional low-priority failure")
            }),
        })
        .expect("register low");

        let event = HookEvent {
            hook_point: HookPoint::RunStart,
            run_id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            tool_name: None,
            metadata: serde_json::json!({}),
        };
        let results = bus.emit(&event);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].hook_id, "hook.high");
        assert_eq!(results[0].status, "ok");
        assert_eq!(results[1].hook_id, "hook.low");
        assert_eq!(results[1].status, "error");
        assert!(results[1]
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("intentional"));

        let observed = order.lock().expect("lock order");
        assert_eq!(observed.as_slice(), ["hook.high", "hook.low"]);
    }

    #[test]
    fn hook_bus_rejects_duplicate_hook_ids() {
        let bus = HookBus::new();
        let registration = HookRegistration {
            hook_id: "hook.duplicate".to_string(),
            plugin_id: "plugin.alpha".to_string(),
            hook_point: HookPoint::RunEnd,
            priority: 1,
            handler: Arc::new(|_| Ok(())),
        };
        bus.register(registration.clone())
            .expect("register first hook");
        assert!(bus.register(registration).is_err());
    }
}
