//! Mechanical compilation of ExecAss dispatches into frozen leaf-action manifests.
//!
//! This module deliberately has no policy, approval, actor-verification, persistence,
//! or effect-execution logic.  A structure that cannot be mechanically resolved pauses;
//! it is never classified as forbidden merely because it is composite, aliased, plugin,
//! shell, or child work.

use crate::execass_actor::{VerifiedHumanEvidenceRef, VerifiedOwnerAuthority};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    Integer(i64),
    String(String),
    Array(Vec<CanonicalValue>),
    Object(Vec<CanonicalField>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalField {
    pub key: String,
    pub value: CanonicalValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetSnapshotInput {
    /// An explicit broad instruction is valid only after this exact set is frozen.
    pub targets: Vec<CanonicalValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolIdentityInput {
    pub tool_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLeafInput {
    pub logical_action_id: String,
    pub action_kind: String,
    pub tool: ToolIdentityInput,
    pub operands: CanonicalValue,
    pub target_snapshot: TargetSnapshotInput,
    /// Digest of exact payload/material when the action has one.
    pub material_digest: Option<String>,
    pub owner_authority: VerifiedOwnerAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchNode {
    pub node_id: String,
    pub action: DispatchAction,
}

/// Closed input vocabulary.  No free-form action category can bypass resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchAction {
    ResolvedLeaf(Box<ResolvedLeafInput>),
    Composite {
        children: Vec<String>,
    },
    Alias {
        alias: String,
    },
    Plugin {
        plugin_id: String,
        version: String,
    },
    Shell {
        shell_id: String,
        shell_version: String,
        command: String,
    },
    Child {
        child_identity: String,
        version: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum IndirectionDescriptor {
    Alias {
        alias: String,
    },
    Plugin {
        plugin_id: String,
        version: String,
    },
    Shell {
        shell_id: String,
        shell_version: String,
        command: String,
    },
    Child {
        child_identity: String,
        version: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerResolutionEntryInput {
    pub descriptor: IndirectionDescriptor,
    pub target_node_id: String,
    pub resolver_id: String,
    pub resolver_version: String,
    pub resolution_material_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolutionTarget {
    target_node_id: String,
    resolver_id: String,
    resolver_version: String,
    resolution_material_digest: Sha256Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerResolutionRegistry {
    entries: BTreeMap<IndirectionDescriptor, Vec<ResolutionTarget>>,
}

impl ServerResolutionRegistry {
    pub fn try_new(entries: Vec<ServerResolutionEntryInput>) -> Result<Self, String> {
        let mut registry = Self::default();
        for entry in entries {
            let descriptor = normalized_descriptor(entry.descriptor)?;
            let target = ResolutionTarget {
                target_node_id: normalized_identifier(&entry.target_node_id)?,
                resolver_id: normalized_identifier(&entry.resolver_id)?,
                resolver_version: normalized_identifier(&entry.resolver_version)?,
                resolution_material_digest: parse_digest(&entry.resolution_material_digest)?,
            };
            let candidates = registry.entries.entry(descriptor).or_default();
            if candidates.contains(&target) {
                return Err("duplicate server resolution entry".to_string());
            }
            candidates.push(target);
        }
        Ok(registry)
    }
}

/// References are used instead of recursive boxes so cycles can be detected and paused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchTree {
    pub root_id: String,
    pub nodes: Vec<DispatchNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CompileLimits {
    max_depth: usize,
    max_nodes: usize,
    max_leaves: usize,
}

const ABSOLUTE_MAX_DEPTH: usize = 32;
const ABSOLUTE_MAX_NODES: usize = 256;
const ABSOLUTE_MAX_LEAVES: usize = 128;

impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_nodes: 256,
            max_leaves: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestCompilation {
    Ready(CanonicalLeafManifest),
    MechanicalResolutionRequired(MechanicalResolutionPause),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MechanicalResolutionPause {
    pub reason: MechanicalResolutionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MechanicalResolutionReason {
    EmptyRoot,
    EmptyNodeIdentity,
    DuplicateNodeIdentity { node_id: String },
    MissingNode { node_id: String },
    CycleDetected { node_id: String },
    EmptyComposite { node_id: String },
    UnresolvedAlias { node_id: String },
    AmbiguousAlias { node_id: String },
    UnresolvedPlugin { node_id: String },
    AmbiguousPlugin { node_id: String },
    UnresolvedShell { node_id: String },
    AmbiguousShell { node_id: String },
    UnresolvedChild { node_id: String },
    AmbiguousChild { node_id: String },
    DepthLimitExceeded { max_depth: usize },
    NodeLimitExceeded { max_nodes: usize },
    LeafLimitExceeded { max_leaves: usize },
    DuplicateLogicalActionIdentity { logical_action_id: String },
    InvalidLeaf { node_id: String, detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalLeafManifest {
    leaves: Vec<CanonicalLeafAction>,
    canonical: FrozenBytes,
}

impl CanonicalLeafManifest {
    pub fn leaves(&self) -> &[CanonicalLeafAction] {
        &self.leaves
    }
    pub fn canonical(&self) -> &FrozenBytes {
        &self.canonical
    }
}

/// The only occurrence-specific fields that may change when a persisted routine
/// manifest is restored. The persisted logical identity names the leaf to rebind;
/// all other action-envelope fields are recovered from the authenticated manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineOccurrenceLeafBinding {
    pub persisted_logical_action_id: String,
    pub occurrence_logical_action_id: String,
    pub target_snapshot: TargetSnapshotInput,
}

/// Restores an exact canonical manifest and rebinds only occurrence-local identity
/// and targets. The expected digest must come from the trusted persistence boundary.
/// Any malformed, duplicate, unknown, non-canonical, or digest-mismatched input fails.
pub fn rebind_persisted_manifest_for_routine_occurrence(
    persisted_canonical_json: &[u8],
    expected_manifest_digest: &str,
    bindings: &[RoutineOccurrenceLeafBinding],
) -> Result<CanonicalLeafManifest, String> {
    let expected_manifest_digest = parse_digest(expected_manifest_digest)?;
    let persisted = parse_persisted_manifest_with_limit(
        persisted_canonical_json,
        MAX_PERSISTED_MANIFEST_BYTES,
    )?;
    if persisted.schema != "carsinos.execass.leaf_action_manifest.v2" {
        return Err("unsupported persisted manifest schema".to_string());
    }
    if persisted.leaves.is_empty() || persisted.leaves.len() > ABSOLUTE_MAX_LEAVES {
        return Err("persisted manifest leaf count is outside allowed bounds".to_string());
    }

    let mut persisted_logical_ids = BTreeSet::new();
    let mut leaves = Vec::with_capacity(persisted.leaves.len());
    for leaf in persisted.leaves {
        let leaf = restore_persisted_leaf(leaf)?;
        if !persisted_logical_ids.insert(leaf.logical_action_id.clone()) {
            return Err("persisted manifest has duplicate logical action identity".to_string());
        }
        leaves.push(leaf);
    }
    let canonical = freeze(canonical_manifest_bytes(&leaves)?);
    if canonical.bytes() != persisted_canonical_json {
        return Err("persisted manifest is not exact canonical JSON".to_string());
    }
    if canonical.digest() != &expected_manifest_digest {
        return Err("persisted manifest digest mismatch".to_string());
    }

    let mut bindings_by_id = BTreeMap::new();
    for binding in bindings {
        let persisted_id = normalized_identifier(&binding.persisted_logical_action_id)?;
        if persisted_id != binding.persisted_logical_action_id {
            return Err("persisted logical action identity is not canonical".to_string());
        }
        if bindings_by_id.insert(persisted_id, binding).is_some() {
            return Err("duplicate routine occurrence leaf binding".to_string());
        }
    }
    if bindings_by_id.len() != leaves.len() {
        return Err("routine occurrence bindings must cover every persisted leaf".to_string());
    }

    let mut occurrence_ids = BTreeSet::new();
    for leaf in &mut leaves {
        let binding = bindings_by_id
            .remove(&leaf.logical_action_id)
            .ok_or_else(|| {
                "routine occurrence binding does not match persisted leaf".to_string()
            })?;
        let occurrence_id = normalized_identifier(&binding.occurrence_logical_action_id)?;
        if occurrence_id != binding.occurrence_logical_action_id {
            return Err("occurrence logical action identity is not canonical".to_string());
        }
        if !occurrence_ids.insert(occurrence_id.clone()) {
            return Err("duplicate occurrence logical action identity".to_string());
        }
        leaf.logical_action_id = occurrence_id;
        leaf.target_snapshot = freeze(canonical_targets_bytes(&binding.target_snapshot)?);
        leaf.canonical = freeze(canonical_leaf_bytes(&LeafCanonicalInputs {
            node_id: &leaf.node_id,
            logical_action_id: &leaf.logical_action_id,
            action_kind: &leaf.action_kind,
            tool: &leaf.tool,
            operands: &leaf.operands,
            target_snapshot: &leaf.target_snapshot,
            material_digest: leaf.material_digest.as_ref(),
            resolution_path: &leaf.resolution_path,
            authority: &leaf.owner_authority,
        }));
    }
    if !bindings_by_id.is_empty() {
        return Err("routine occurrence binding does not match persisted leaf".to_string());
    }
    let canonical = freeze(canonical_manifest_bytes(&leaves)?);
    Ok(CanonicalLeafManifest { leaves, canonical })
}

fn parse_persisted_manifest_with_limit(
    persisted_canonical_json: &[u8],
    byte_limit: usize,
) -> Result<PersistedManifest, String> {
    if persisted_canonical_json.len() > byte_limit {
        return Err("persisted manifest exceeds byte limit".to_string());
    }
    serde_json::from_slice(persisted_canonical_json)
        .map_err(|error| format!("invalid persisted manifest JSON: {error}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalLeafAction {
    node_id: String,
    logical_action_id: String,
    action_kind: String,
    tool: CanonicalToolIdentity,
    operands: FrozenBytes,
    target_snapshot: FrozenBytes,
    material_digest: Option<Sha256Digest>,
    resolution_path: Vec<CanonicalResolutionStep>,
    owner_authority: CanonicalOwnerAuthority,
    canonical: FrozenBytes,
}

impl CanonicalLeafAction {
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    pub fn logical_action_id(&self) -> &str {
        &self.logical_action_id
    }
    pub fn action_kind(&self) -> &str {
        &self.action_kind
    }
    pub fn tool(&self) -> &CanonicalToolIdentity {
        &self.tool
    }
    pub fn operands(&self) -> &FrozenBytes {
        &self.operands
    }
    pub fn target_snapshot(&self) -> &FrozenBytes {
        &self.target_snapshot
    }
    pub fn material_digest(&self) -> Option<&Sha256Digest> {
        self.material_digest.as_ref()
    }
    pub fn resolution_path(&self) -> &[CanonicalResolutionStep] {
        &self.resolution_path
    }
    pub fn owner_authority(&self) -> &CanonicalOwnerAuthority {
        &self.owner_authority
    }
    pub fn canonical(&self) -> &FrozenBytes {
        &self.canonical
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResolutionStep {
    descriptor: IndirectionDescriptor,
    target_node_id: String,
    resolver_id: String,
    resolver_version: String,
    resolution_material_digest: Sha256Digest,
}

impl CanonicalResolutionStep {
    pub fn descriptor(&self) -> &IndirectionDescriptor {
        &self.descriptor
    }
    pub fn target_node_id(&self) -> &str {
        &self.target_node_id
    }
    pub fn resolver_id(&self) -> &str {
        &self.resolver_id
    }
    pub fn resolver_version(&self) -> &str {
        &self.resolver_version
    }
    pub fn resolution_material_digest(&self) -> &Sha256Digest {
        &self.resolution_material_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalToolIdentity {
    tool_id: String,
    version: String,
}
impl CanonicalToolIdentity {
    pub fn tool_id(&self) -> &str {
        &self.tool_id
    }
    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalOwnerAuthority {
    owner_evidence: CanonicalOwnerEvidence,
    authority_provenance_id: String,
    normalized_intent_digest: Sha256Digest,
    instruction_revision: String,
    instruction_digest: Sha256Digest,
    owner_envelope_revision: String,
    owner_envelope_digest: Sha256Digest,
    authority_kind: String,
    normalized_scope: FrozenBytes,
    policy_revision: i64,
    bound_decision_id: Option<String>,
    bound_decision_revision: Option<i64>,
    bound_manifest_digest: Option<Sha256Digest>,
    bound_challenge_nonce_digest: Option<Sha256Digest>,
    evidence_digest: Sha256Digest,
    created_at: i64,
    expires_at: Option<i64>,
}
impl CanonicalOwnerAuthority {
    pub fn owner_evidence(&self) -> &CanonicalOwnerEvidence {
        &self.owner_evidence
    }
    pub fn authority_provenance_id(&self) -> &str {
        &self.authority_provenance_id
    }
    pub fn normalized_intent_digest(&self) -> &Sha256Digest {
        &self.normalized_intent_digest
    }
    pub fn instruction_revision(&self) -> &str {
        &self.instruction_revision
    }
    pub fn instruction_digest(&self) -> &Sha256Digest {
        &self.instruction_digest
    }
    pub fn owner_envelope_revision(&self) -> &str {
        &self.owner_envelope_revision
    }
    pub fn owner_envelope_digest(&self) -> &Sha256Digest {
        &self.owner_envelope_digest
    }
    pub fn authority_kind(&self) -> &str {
        &self.authority_kind
    }
    pub fn normalized_scope_json(&self) -> &str {
        std::str::from_utf8(self.normalized_scope.bytes()).expect("canonical JSON is UTF-8")
    }
    pub fn policy_revision(&self) -> i64 {
        self.policy_revision
    }
    pub fn bound_decision_id(&self) -> Option<&str> {
        self.bound_decision_id.as_deref()
    }
    pub fn bound_decision_revision(&self) -> Option<i64> {
        self.bound_decision_revision
    }
    pub fn bound_manifest_digest(&self) -> Option<&Sha256Digest> {
        self.bound_manifest_digest.as_ref()
    }
    pub fn bound_challenge_nonce_digest(&self) -> Option<&Sha256Digest> {
        self.bound_challenge_nonce_digest.as_ref()
    }
    pub fn evidence_digest(&self) -> &Sha256Digest {
        &self.evidence_digest
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn expires_at(&self) -> Option<i64> {
        self.expires_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalOwnerEvidence {
    LocalInteractive {
        authenticated_client_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        request_correlation_id: String,
        source_message_id: Option<String>,
    },
    RemoteAuthenticated {
        adapter_id: String,
        provider_account_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        source_message_id: String,
        request_correlation_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenBytes {
    bytes: Vec<u8>,
    digest: Sha256Digest,
}
impl FrozenBytes {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &Sha256Digest {
        &self.digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sha256Digest(String);
impl Sha256Digest {
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

pub fn compile_dispatch(
    tree: &DispatchTree,
    resolutions: &ServerResolutionRegistry,
) -> ManifestCompilation {
    compile_dispatch_with_limits(tree, resolutions, CompileLimits::default())
}

fn compile_dispatch_with_limits(
    tree: &DispatchTree,
    resolutions: &ServerResolutionRegistry,
    limits: CompileLimits,
) -> ManifestCompilation {
    let limits = CompileLimits {
        max_depth: limits.max_depth.min(ABSOLUTE_MAX_DEPTH),
        max_nodes: limits.max_nodes.min(ABSOLUTE_MAX_NODES),
        max_leaves: limits.max_leaves.min(ABSOLUTE_MAX_LEAVES),
    };
    let root_id = match normalized_identifier(&tree.root_id) {
        Ok(value) => value,
        Err(_) => return pause(MechanicalResolutionReason::EmptyRoot),
    };
    let mut lookup = BTreeMap::new();
    for (index, node) in tree.nodes.iter().enumerate() {
        let node_id = match normalized_identifier(&node.node_id) {
            Ok(value) => value,
            Err(_) => return pause(MechanicalResolutionReason::EmptyNodeIdentity),
        };
        if lookup.insert(node_id.clone(), index).is_some() {
            return pause(MechanicalResolutionReason::DuplicateNodeIdentity { node_id });
        }
    }
    let mut compiler = Compiler {
        tree,
        resolutions,
        lookup,
        limits,
        visited_nodes: 0,
        leaves: Vec::new(),
        active: BTreeSet::new(),
        logical_action_ids: BTreeSet::new(),
    };
    if let Err(reason) = compiler.visit(&root_id, 0, &mut Vec::new()) {
        return pause(reason);
    }
    let canonical = match canonical_manifest_bytes(&compiler.leaves) {
        Ok(bytes) => freeze(bytes),
        Err(detail) => {
            return pause(MechanicalResolutionReason::InvalidLeaf {
                node_id: root_id,
                detail,
            })
        }
    };
    ManifestCompilation::Ready(CanonicalLeafManifest {
        leaves: compiler.leaves,
        canonical,
    })
}

struct Compiler<'a> {
    tree: &'a DispatchTree,
    resolutions: &'a ServerResolutionRegistry,
    lookup: BTreeMap<String, usize>,
    limits: CompileLimits,
    visited_nodes: usize,
    leaves: Vec<CanonicalLeafAction>,
    active: BTreeSet<String>,
    logical_action_ids: BTreeSet<String>,
}

impl Compiler<'_> {
    fn visit(
        &mut self,
        node_id: &str,
        depth: usize,
        resolution_path: &mut Vec<CanonicalResolutionStep>,
    ) -> Result<(), MechanicalResolutionReason> {
        if depth > self.limits.max_depth {
            return Err(MechanicalResolutionReason::DepthLimitExceeded {
                max_depth: self.limits.max_depth,
            });
        }
        if self.visited_nodes >= self.limits.max_nodes {
            return Err(MechanicalResolutionReason::NodeLimitExceeded {
                max_nodes: self.limits.max_nodes,
            });
        }
        let node_id = normalized_identifier(node_id)
            .map_err(|_| MechanicalResolutionReason::EmptyNodeIdentity)?;
        if !self.active.insert(node_id.clone()) {
            return Err(MechanicalResolutionReason::CycleDetected { node_id });
        }
        self.visited_nodes += 1;
        let result = self.visit_present(&node_id, depth, resolution_path);
        self.active.remove(&node_id);
        result
    }

    fn visit_present(
        &mut self,
        node_id: &str,
        depth: usize,
        resolution_path: &mut Vec<CanonicalResolutionStep>,
    ) -> Result<(), MechanicalResolutionReason> {
        let Some(&index) = self.lookup.get(node_id) else {
            return Err(MechanicalResolutionReason::MissingNode {
                node_id: node_id.to_string(),
            });
        };
        match &self.tree.nodes[index].action {
            DispatchAction::ResolvedLeaf(leaf) => {
                if self.leaves.len() >= self.limits.max_leaves {
                    return Err(MechanicalResolutionReason::LeafLimitExceeded {
                        max_leaves: self.limits.max_leaves,
                    });
                }
                let leaf = canonical_leaf(node_id, leaf, resolution_path)?;
                if !self
                    .logical_action_ids
                    .insert(leaf.logical_action_id.clone())
                {
                    return Err(MechanicalResolutionReason::DuplicateLogicalActionIdentity {
                        logical_action_id: leaf.logical_action_id.clone(),
                    });
                }
                self.leaves.push(leaf);
                Ok(())
            }
            DispatchAction::Composite { children } => {
                if children.is_empty() {
                    return Err(MechanicalResolutionReason::EmptyComposite {
                        node_id: node_id.to_string(),
                    });
                }
                for child in children {
                    self.visit(child, depth + 1, resolution_path)?;
                }
                Ok(())
            }
            action @ DispatchAction::Alias { .. } => self.visit_expansion(
                node_id,
                descriptor_for_action(node_id, action)?,
                depth,
                StructureKind::Alias,
                resolution_path,
            ),
            action @ DispatchAction::Plugin { .. } => self.visit_expansion(
                node_id,
                descriptor_for_action(node_id, action)?,
                depth,
                StructureKind::Plugin,
                resolution_path,
            ),
            action @ DispatchAction::Shell { .. } => self.visit_expansion(
                node_id,
                descriptor_for_action(node_id, action)?,
                depth,
                StructureKind::Shell,
                resolution_path,
            ),
            action @ DispatchAction::Child { .. } => self.visit_expansion(
                node_id,
                descriptor_for_action(node_id, action)?,
                depth,
                StructureKind::Child,
                resolution_path,
            ),
        }
    }

    fn visit_expansion(
        &mut self,
        node_id: &str,
        descriptor: IndirectionDescriptor,
        depth: usize,
        kind: StructureKind,
        resolution_path: &mut Vec<CanonicalResolutionStep>,
    ) -> Result<(), MechanicalResolutionReason> {
        let Some(candidates) = self.resolutions.entries.get(&descriptor) else {
            return Err(kind.unresolved(node_id));
        };
        if candidates.len() != 1 {
            return Err(kind.ambiguous(node_id));
        }
        let target = &candidates[0];
        resolution_path.push(CanonicalResolutionStep {
            descriptor,
            target_node_id: target.target_node_id.clone(),
            resolver_id: target.resolver_id.clone(),
            resolver_version: target.resolver_version.clone(),
            resolution_material_digest: target.resolution_material_digest.clone(),
        });
        let result = self.visit(&target.target_node_id, depth + 1, resolution_path);
        resolution_path.pop();
        result
    }
}

#[derive(Clone, Copy)]
enum StructureKind {
    Alias,
    Plugin,
    Shell,
    Child,
}
impl StructureKind {
    fn unresolved(&self, node_id: &str) -> MechanicalResolutionReason {
        match self {
            Self::Alias => MechanicalResolutionReason::UnresolvedAlias {
                node_id: node_id.to_string(),
            },
            Self::Plugin => MechanicalResolutionReason::UnresolvedPlugin {
                node_id: node_id.to_string(),
            },
            Self::Shell => MechanicalResolutionReason::UnresolvedShell {
                node_id: node_id.to_string(),
            },
            Self::Child => MechanicalResolutionReason::UnresolvedChild {
                node_id: node_id.to_string(),
            },
        }
    }
    fn ambiguous(&self, node_id: &str) -> MechanicalResolutionReason {
        match self {
            Self::Alias => MechanicalResolutionReason::AmbiguousAlias {
                node_id: node_id.to_string(),
            },
            Self::Plugin => MechanicalResolutionReason::AmbiguousPlugin {
                node_id: node_id.to_string(),
            },
            Self::Shell => MechanicalResolutionReason::AmbiguousShell {
                node_id: node_id.to_string(),
            },
            Self::Child => MechanicalResolutionReason::AmbiguousChild {
                node_id: node_id.to_string(),
            },
        }
    }
}

fn descriptor_for_action(
    node_id: &str,
    action: &DispatchAction,
) -> Result<IndirectionDescriptor, MechanicalResolutionReason> {
    let descriptor = match action {
        DispatchAction::Alias { alias } => IndirectionDescriptor::Alias {
            alias: alias.clone(),
        },
        DispatchAction::Plugin { plugin_id, version } => IndirectionDescriptor::Plugin {
            plugin_id: plugin_id.clone(),
            version: version.clone(),
        },
        DispatchAction::Shell {
            shell_id,
            shell_version,
            command,
        } => IndirectionDescriptor::Shell {
            shell_id: shell_id.clone(),
            shell_version: shell_version.clone(),
            command: command.clone(),
        },
        DispatchAction::Child {
            child_identity,
            version,
        } => IndirectionDescriptor::Child {
            child_identity: child_identity.clone(),
            version: version.clone(),
        },
        DispatchAction::ResolvedLeaf(_) | DispatchAction::Composite { .. } => {
            unreachable!("leaf and composite actions do not require resolution")
        }
    };
    normalized_descriptor(descriptor).map_err(|detail| MechanicalResolutionReason::InvalidLeaf {
        node_id: node_id.to_string(),
        detail,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedManifest {
    leaves: Vec<PersistedLeaf>,
    schema: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedLeaf {
    action_kind: String,
    logical_action_id: String,
    material_digest: Option<String>,
    node_id: String,
    operands: CanonicalValue,
    owner_authority: PersistedOwnerAuthority,
    resolution_path: Vec<PersistedResolutionStep>,
    schema: String,
    target_snapshot: PersistedTargetSnapshot,
    tool: PersistedToolIdentity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedToolIdentity {
    tool_id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedTargetSnapshot {
    targets: Vec<CanonicalValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedResolutionStep {
    descriptor: PersistedDescriptor,
    resolution_material_digest: String,
    resolver_id: String,
    resolver_version: String,
    target_node_id: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PersistedDescriptor {
    Alias {
        alias: String,
    },
    Plugin {
        plugin_id: String,
        version: String,
    },
    Shell {
        command: String,
        shell_id: String,
        shell_version: String,
    },
    Child {
        child_identity: String,
        version: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedOwnerAuthority {
    authority_kind: String,
    authority_provenance_id: String,
    bound_challenge_nonce_digest: Option<String>,
    bound_decision_id: Option<String>,
    bound_decision_revision: Option<i64>,
    bound_manifest_digest: Option<String>,
    created_at: i64,
    evidence_digest: String,
    expires_at: Option<i64>,
    instruction_digest: String,
    instruction_revision: String,
    normalized_intent_digest: String,
    normalized_scope: CanonicalValue,
    owner_envelope_digest: String,
    owner_envelope_revision: String,
    owner_evidence: PersistedOwnerEvidence,
    policy_revision: i64,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PersistedOwnerEvidence {
    LocalInteractive {
        authenticated_client_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        request_correlation_id: String,
        source_message_id: Option<String>,
    },
    RemoteAuthenticated {
        adapter_id: String,
        authenticated_ingress: String,
        channel_assurance: String,
        provider_account_id: String,
        request_correlation_id: String,
        source_message_id: String,
    },
}

impl<'de> Deserialize<'de> for CanonicalValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CanonicalValueVisitor;
        impl<'de> Visitor<'de> for CanonicalValueVisitor {
            type Value = CanonicalValue;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("canonical JSON without floats or duplicate object keys")
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(CanonicalValue::Null)
            }
            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(CanonicalValue::Bool(value))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(CanonicalValue::Integer(value))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                i64::try_from(value)
                    .map(CanonicalValue::Integer)
                    .map_err(|_| E::custom("integer exceeds signed 64-bit canonical range"))
            }
            fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Err(E::custom("floating-point JSON is not canonical"))
            }
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(CanonicalValue::String(value.to_string()))
            }
            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(CanonicalValue::String(value))
            }
            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(CanonicalValue::Null)
            }
            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                CanonicalValue::deserialize(deserializer)
            }
            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element()? {
                    values.push(value);
                    if values.len() > MAX_CANONICAL_ARRAY_ITEMS {
                        return Err(de::Error::custom("canonical array exceeds item limit"));
                    }
                }
                Ok(CanonicalValue::Array(values))
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut fields = Vec::new();
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key.clone()) {
                        return Err(de::Error::custom("object has duplicate exact keys"));
                    }
                    fields.push(CanonicalField {
                        key,
                        value: map.next_value()?,
                    });
                    if fields.len() > MAX_CANONICAL_OBJECT_FIELDS {
                        return Err(de::Error::custom("canonical object exceeds field limit"));
                    }
                }
                Ok(CanonicalValue::Object(fields))
            }
        }
        deserializer.deserialize_any(CanonicalValueVisitor)
    }
}

fn restore_persisted_leaf(input: PersistedLeaf) -> Result<CanonicalLeafAction, String> {
    if input.schema != "carsinos.execass.leaf_action.v2" {
        return Err("unsupported persisted leaf schema".to_string());
    }
    if input.resolution_path.len() > ABSOLUTE_MAX_DEPTH {
        return Err("persisted resolution path exceeds depth limit".to_string());
    }
    let node_id = normalized_identifier(&input.node_id)?;
    let logical_action_id = normalized_identifier(&input.logical_action_id)?;
    let action_kind = normalized_identifier(&input.action_kind)?;
    let tool = CanonicalToolIdentity {
        tool_id: normalized_identifier(&input.tool.tool_id)?,
        version: normalized_identifier(&input.tool.version)?,
    };
    let operands = freeze(canonical_value_bytes(&input.operands)?);
    let target_snapshot = freeze(canonical_targets_bytes(&TargetSnapshotInput {
        targets: input.target_snapshot.targets,
    })?);
    let material_digest = input
        .material_digest
        .as_deref()
        .map(parse_digest)
        .transpose()?;
    let resolution_path = input
        .resolution_path
        .into_iter()
        .map(restore_resolution_step)
        .collect::<Result<Vec<_>, _>>()?;
    let owner_authority = restore_owner_authority(input.owner_authority)?;
    let canonical = freeze(canonical_leaf_bytes(&LeafCanonicalInputs {
        node_id: &node_id,
        logical_action_id: &logical_action_id,
        action_kind: &action_kind,
        tool: &tool,
        operands: &operands,
        target_snapshot: &target_snapshot,
        material_digest: material_digest.as_ref(),
        resolution_path: &resolution_path,
        authority: &owner_authority,
    }));
    Ok(CanonicalLeafAction {
        node_id,
        logical_action_id,
        action_kind,
        tool,
        operands,
        target_snapshot,
        material_digest,
        resolution_path,
        owner_authority,
        canonical,
    })
}

fn restore_resolution_step(
    input: PersistedResolutionStep,
) -> Result<CanonicalResolutionStep, String> {
    let descriptor = match input.descriptor {
        PersistedDescriptor::Alias { alias } => IndirectionDescriptor::Alias { alias },
        PersistedDescriptor::Plugin { plugin_id, version } => {
            IndirectionDescriptor::Plugin { plugin_id, version }
        }
        PersistedDescriptor::Shell {
            command,
            shell_id,
            shell_version,
        } => IndirectionDescriptor::Shell {
            shell_id,
            shell_version,
            command,
        },
        PersistedDescriptor::Child {
            child_identity,
            version,
        } => IndirectionDescriptor::Child {
            child_identity,
            version,
        },
    };
    Ok(CanonicalResolutionStep {
        descriptor: normalized_descriptor(descriptor)?,
        target_node_id: normalized_identifier(&input.target_node_id)?,
        resolver_id: normalized_identifier(&input.resolver_id)?,
        resolver_version: normalized_identifier(&input.resolver_version)?,
        resolution_material_digest: parse_digest(&input.resolution_material_digest)?,
    })
}

fn restore_owner_authority(
    input: PersistedOwnerAuthority,
) -> Result<CanonicalOwnerAuthority, String> {
    let owner_evidence = match input.owner_evidence {
        PersistedOwnerEvidence::LocalInteractive {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => CanonicalOwnerEvidence::LocalInteractive {
            authenticated_client_id: normalized_identifier(&authenticated_client_id)?,
            authenticated_ingress: normalized_identifier(&authenticated_ingress)?,
            channel_assurance: normalized_identifier(&channel_assurance)?,
            request_correlation_id: normalized_identifier(&request_correlation_id)?,
            source_message_id: source_message_id
                .as_deref()
                .map(normalized_identifier)
                .transpose()?,
        },
        PersistedOwnerEvidence::RemoteAuthenticated {
            adapter_id,
            authenticated_ingress,
            channel_assurance,
            provider_account_id,
            request_correlation_id,
            source_message_id,
        } => CanonicalOwnerEvidence::RemoteAuthenticated {
            adapter_id: normalized_identifier(&adapter_id)?,
            provider_account_id: normalized_identifier(&provider_account_id)?,
            authenticated_ingress: normalized_identifier(&authenticated_ingress)?,
            channel_assurance: normalized_identifier(&channel_assurance)?,
            source_message_id: normalized_identifier(&source_message_id)?,
            request_correlation_id: normalized_identifier(&request_correlation_id)?,
        },
    };
    Ok(CanonicalOwnerAuthority {
        owner_evidence,
        authority_provenance_id: normalized_identifier(&input.authority_provenance_id)?,
        normalized_intent_digest: parse_digest(&input.normalized_intent_digest)?,
        instruction_revision: normalized_identifier(&input.instruction_revision)?,
        instruction_digest: parse_digest(&input.instruction_digest)?,
        owner_envelope_revision: normalized_identifier(&input.owner_envelope_revision)?,
        owner_envelope_digest: parse_digest(&input.owner_envelope_digest)?,
        authority_kind: normalized_identifier(&input.authority_kind)?,
        normalized_scope: freeze(canonical_value_bytes(&input.normalized_scope)?),
        policy_revision: input.policy_revision,
        bound_decision_id: input
            .bound_decision_id
            .as_deref()
            .map(normalized_identifier)
            .transpose()?,
        bound_decision_revision: input.bound_decision_revision,
        bound_manifest_digest: input
            .bound_manifest_digest
            .as_deref()
            .map(parse_digest)
            .transpose()?,
        bound_challenge_nonce_digest: input
            .bound_challenge_nonce_digest
            .as_deref()
            .map(parse_digest)
            .transpose()?,
        evidence_digest: parse_digest(&input.evidence_digest)?,
        created_at: input.created_at,
        expires_at: input.expires_at,
    })
}

fn canonical_leaf(
    node_id: &str,
    input: &ResolvedLeafInput,
    resolution_path: &[CanonicalResolutionStep],
) -> Result<CanonicalLeafAction, MechanicalResolutionReason> {
    let invalid = |detail| MechanicalResolutionReason::InvalidLeaf {
        node_id: node_id.to_string(),
        detail,
    };
    let logical_action_id = normalized_identifier(&input.logical_action_id).map_err(&invalid)?;
    let action_kind = normalized_identifier(&input.action_kind).map_err(&invalid)?;
    let tool = CanonicalToolIdentity {
        tool_id: normalized_identifier(&input.tool.tool_id).map_err(&invalid)?,
        version: normalized_identifier(&input.tool.version).map_err(&invalid)?,
    };
    let operands = freeze(canonical_value_bytes(&input.operands).map_err(&invalid)?);
    let target_snapshot =
        freeze(canonical_targets_bytes(&input.target_snapshot).map_err(&invalid)?);
    let material_digest = input
        .material_digest
        .as_deref()
        .map(parse_digest)
        .transpose()
        .map_err(&invalid)?;
    let owner_authority = canonicalize_owner_authority(&input.owner_authority).map_err(&invalid)?;
    let bytes = canonical_leaf_bytes(&LeafCanonicalInputs {
        node_id,
        logical_action_id: &logical_action_id,
        action_kind: &action_kind,
        tool: &tool,
        operands: &operands,
        target_snapshot: &target_snapshot,
        material_digest: material_digest.as_ref(),
        resolution_path,
        authority: &owner_authority,
    });
    Ok(CanonicalLeafAction {
        node_id: node_id.to_string(),
        logical_action_id,
        action_kind,
        tool,
        operands,
        target_snapshot,
        material_digest,
        resolution_path: resolution_path.to_vec(),
        owner_authority,
        canonical: freeze(bytes),
    })
}

pub fn canonicalize_owner_authority(
    input: &VerifiedOwnerAuthority,
) -> Result<CanonicalOwnerAuthority, String> {
    if input.normalized_scope_json().len() > MAX_CANONICAL_VALUE_BYTES {
        return Err("normalized authority scope exceeds canonical byte limit".to_string());
    }
    let owner_evidence = match input.evidence() {
        VerifiedHumanEvidenceRef::Local {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => CanonicalOwnerEvidence::LocalInteractive {
            authenticated_client_id: normalized_identifier(authenticated_client_id)?,
            authenticated_ingress: normalized_identifier(authenticated_ingress)?,
            channel_assurance: normalized_identifier(channel_assurance)?,
            request_correlation_id: normalized_identifier(request_correlation_id)?,
            source_message_id: source_message_id.map(normalized_identifier).transpose()?,
        },
        VerifiedHumanEvidenceRef::Remote {
            adapter_id,
            provider_account_id,
            authenticated_ingress,
            channel_assurance,
            source_message_id,
            request_correlation_id,
        } => CanonicalOwnerEvidence::RemoteAuthenticated {
            adapter_id: normalized_identifier(adapter_id)?,
            provider_account_id: normalized_identifier(provider_account_id)?,
            authenticated_ingress: normalized_identifier(authenticated_ingress)?,
            channel_assurance: normalized_identifier(channel_assurance)?,
            source_message_id: normalized_identifier(source_message_id)?,
            request_correlation_id: normalized_identifier(request_correlation_id)?,
        },
    };
    Ok(CanonicalOwnerAuthority {
        owner_evidence,
        authority_provenance_id: normalized_identifier(input.authority_provenance_id())?,
        normalized_intent_digest: parse_digest(input.normalized_intent_digest())?,
        instruction_revision: normalized_identifier(input.instruction_revision())?,
        instruction_digest: parse_digest(input.instruction_digest())?,
        owner_envelope_revision: normalized_identifier(input.owner_envelope_revision())?,
        owner_envelope_digest: parse_digest(input.owner_envelope_digest())?,
        authority_kind: normalized_identifier(input.authority_kind())?,
        normalized_scope: freeze(input.normalized_scope_json().as_bytes().to_vec()),
        policy_revision: input.policy_revision(),
        bound_decision_id: input
            .bound_decision_id()
            .map(normalized_identifier)
            .transpose()?,
        bound_decision_revision: input.bound_decision_revision(),
        bound_manifest_digest: input
            .bound_manifest_digest()
            .map(parse_digest)
            .transpose()?,
        bound_challenge_nonce_digest: input
            .bound_challenge_nonce_digest()
            .map(parse_digest)
            .transpose()?,
        evidence_digest: parse_digest(input.evidence_digest())?,
        created_at: input.created_at(),
        expires_at: input.expires_at(),
    })
}

fn canonical_manifest_bytes(leaves: &[CanonicalLeafAction]) -> Result<Vec<u8>, String> {
    if leaves.is_empty() {
        return Err("manifest contains no resolved leaves".to_string());
    }
    let mut out = b"{\"leaves\":[".to_vec();
    for (index, leaf) in leaves.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        out.extend_from_slice(leaf.canonical.bytes());
    }
    out.extend_from_slice(b"],\"schema\":\"carsinos.execass.leaf_action_manifest.v2\"}");
    Ok(out)
}

struct LeafCanonicalInputs<'a> {
    node_id: &'a str,
    logical_action_id: &'a str,
    action_kind: &'a str,
    tool: &'a CanonicalToolIdentity,
    operands: &'a FrozenBytes,
    target_snapshot: &'a FrozenBytes,
    material_digest: Option<&'a Sha256Digest>,
    resolution_path: &'a [CanonicalResolutionStep],
    authority: &'a CanonicalOwnerAuthority,
}

fn canonical_leaf_bytes(input: &LeafCanonicalInputs<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"{\"action_kind\":");
    push_json_string(&mut out, input.action_kind);
    out.extend_from_slice(b",\"logical_action_id\":");
    push_json_string(&mut out, input.logical_action_id);
    out.extend_from_slice(b",\"material_digest\":");
    match input.material_digest {
        Some(value) => push_json_string(&mut out, value.as_hex()),
        None => out.extend_from_slice(b"null"),
    }
    out.extend_from_slice(b",\"node_id\":");
    push_json_string(&mut out, input.node_id);
    out.extend_from_slice(b",\"operands\":");
    out.extend_from_slice(input.operands.bytes());
    out.extend_from_slice(b",\"owner_authority\":");
    canonical_authority_bytes(&mut out, input.authority);
    out.extend_from_slice(b",\"resolution_path\":[");
    for (index, step) in input.resolution_path.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        canonical_resolution_step_bytes(&mut out, step);
    }
    out.extend_from_slice(b"],\"schema\":\"carsinos.execass.leaf_action.v2\"");
    out.extend_from_slice(b",\"target_snapshot\":");
    out.extend_from_slice(input.target_snapshot.bytes());
    out.extend_from_slice(b",\"tool\":{\"tool_id\":");
    push_json_string(&mut out, input.tool.tool_id());
    out.extend_from_slice(b",\"version\":");
    push_json_string(&mut out, input.tool.version());
    out.extend_from_slice(b"}}");
    out
}

fn canonical_resolution_step_bytes(out: &mut Vec<u8>, step: &CanonicalResolutionStep) {
    out.extend_from_slice(b"{\"descriptor\":");
    canonical_descriptor_bytes(out, &step.descriptor);
    out.extend_from_slice(b",\"resolution_material_digest\":");
    push_json_string(out, step.resolution_material_digest.as_hex());
    out.extend_from_slice(b",\"resolver_id\":");
    push_json_string(out, &step.resolver_id);
    out.extend_from_slice(b",\"resolver_version\":");
    push_json_string(out, &step.resolver_version);
    out.extend_from_slice(b",\"target_node_id\":");
    push_json_string(out, &step.target_node_id);
    out.push(b'}');
}

fn canonical_descriptor_bytes(out: &mut Vec<u8>, descriptor: &IndirectionDescriptor) {
    match descriptor {
        IndirectionDescriptor::Alias { alias } => {
            out.extend_from_slice(b"{\"alias\":");
            push_json_string(out, alias);
            out.extend_from_slice(b",\"kind\":\"alias\"}");
        }
        IndirectionDescriptor::Plugin { plugin_id, version } => {
            out.extend_from_slice(b"{\"kind\":\"plugin\",\"plugin_id\":");
            push_json_string(out, plugin_id);
            out.extend_from_slice(b",\"version\":");
            push_json_string(out, version);
            out.push(b'}');
        }
        IndirectionDescriptor::Shell {
            shell_id,
            shell_version,
            command,
        } => {
            out.extend_from_slice(b"{\"command\":");
            push_json_string(out, command);
            out.extend_from_slice(b",\"kind\":\"shell\",\"shell_id\":");
            push_json_string(out, shell_id);
            out.extend_from_slice(b",\"shell_version\":");
            push_json_string(out, shell_version);
            out.push(b'}');
        }
        IndirectionDescriptor::Child {
            child_identity,
            version,
        } => {
            out.extend_from_slice(b"{\"child_identity\":");
            push_json_string(out, child_identity);
            out.extend_from_slice(b",\"kind\":\"child\",\"version\":");
            push_json_string(out, version);
            out.push(b'}');
        }
    }
}

fn canonical_authority_bytes(out: &mut Vec<u8>, value: &CanonicalOwnerAuthority) {
    out.extend_from_slice(b"{\"authority_kind\":");
    push_json_string(out, &value.authority_kind);
    out.extend_from_slice(b",\"authority_provenance_id\":");
    push_json_string(out, &value.authority_provenance_id);
    out.extend_from_slice(b",\"bound_challenge_nonce_digest\":");
    push_optional_digest(out, value.bound_challenge_nonce_digest.as_ref());
    out.extend_from_slice(b",\"bound_decision_id\":");
    push_optional_string(out, value.bound_decision_id.as_deref());
    out.extend_from_slice(b",\"bound_decision_revision\":");
    match value.bound_decision_revision {
        Some(revision) => out.extend_from_slice(revision.to_string().as_bytes()),
        None => out.extend_from_slice(b"null"),
    }
    out.extend_from_slice(b",\"bound_manifest_digest\":");
    push_optional_digest(out, value.bound_manifest_digest.as_ref());
    out.extend_from_slice(b",\"created_at\":");
    out.extend_from_slice(value.created_at.to_string().as_bytes());
    out.extend_from_slice(b",\"evidence_digest\":");
    push_json_string(out, value.evidence_digest.as_hex());
    out.extend_from_slice(b",\"expires_at\":");
    match value.expires_at {
        Some(expires_at) => out.extend_from_slice(expires_at.to_string().as_bytes()),
        None => out.extend_from_slice(b"null"),
    }
    out.extend_from_slice(b",\"instruction_digest\":");
    push_json_string(out, value.instruction_digest.as_hex());
    out.extend_from_slice(b",\"instruction_revision\":");
    push_json_string(out, &value.instruction_revision);
    out.extend_from_slice(b",\"normalized_intent_digest\":");
    push_json_string(out, value.normalized_intent_digest.as_hex());
    out.extend_from_slice(b",\"normalized_scope\":");
    out.extend_from_slice(value.normalized_scope.bytes());
    out.extend_from_slice(b",\"owner_envelope_digest\":");
    push_json_string(out, value.owner_envelope_digest.as_hex());
    out.extend_from_slice(b",\"owner_envelope_revision\":");
    push_json_string(out, &value.owner_envelope_revision);
    out.extend_from_slice(b",\"owner_evidence\":{");
    match &value.owner_evidence {
        CanonicalOwnerEvidence::LocalInteractive {
            authenticated_client_id,
            authenticated_ingress,
            channel_assurance,
            request_correlation_id,
            source_message_id,
        } => {
            out.extend_from_slice(b"\"authenticated_client_id\":");
            push_json_string(out, authenticated_client_id);
            out.extend_from_slice(b",\"authenticated_ingress\":");
            push_json_string(out, authenticated_ingress);
            out.extend_from_slice(b",\"channel_assurance\":");
            push_json_string(out, channel_assurance);
            out.extend_from_slice(b",\"kind\":\"local_interactive\",\"request_correlation_id\":");
            push_json_string(out, request_correlation_id);
            out.extend_from_slice(b",\"source_message_id\":");
            push_optional_string(out, source_message_id.as_deref());
        }
        CanonicalOwnerEvidence::RemoteAuthenticated {
            adapter_id,
            provider_account_id,
            authenticated_ingress,
            channel_assurance,
            source_message_id,
            request_correlation_id,
        } => {
            out.extend_from_slice(b"\"adapter_id\":");
            push_json_string(out, adapter_id);
            out.extend_from_slice(b",\"authenticated_ingress\":");
            push_json_string(out, authenticated_ingress);
            out.extend_from_slice(b",\"channel_assurance\":");
            push_json_string(out, channel_assurance);
            out.extend_from_slice(b",\"kind\":\"remote_authenticated\",\"provider_account_id\":");
            push_json_string(out, provider_account_id);
            out.extend_from_slice(b",\"request_correlation_id\":");
            push_json_string(out, request_correlation_id);
            out.extend_from_slice(b",\"source_message_id\":");
            push_json_string(out, source_message_id);
        }
    }
    out.extend_from_slice(b"},\"policy_revision\":");
    out.extend_from_slice(value.policy_revision.to_string().as_bytes());
    out.push(b'}');
}

fn push_optional_string(out: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => push_json_string(out, value),
        None => out.extend_from_slice(b"null"),
    }
}

fn push_optional_digest(out: &mut Vec<u8>, value: Option<&Sha256Digest>) {
    push_optional_string(out, value.map(Sha256Digest::as_hex));
}

const MAX_CANONICAL_VALUE_DEPTH: usize = 32;
const MAX_CANONICAL_ARRAY_ITEMS: usize = 1024;
const MAX_CANONICAL_OBJECT_FIELDS: usize = 1024;
const MAX_CANONICAL_STRING_BYTES: usize = 16 * 1024;
const MAX_CANONICAL_VALUE_BYTES: usize = 64 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 256;

// A canonical leaf contains at most three 64-KiB value regions (operands,
// targets, and authority scope), 16 non-resolution identifiers, and six
// identifiers per resolution step. JSON escaping can expand an identifier by
// at most 6x. The additional 128 KiB per leaf covers fixed field names, schema,
// punctuation, nulls, integers, and every fixed-width digest. This conservative
// ceiling therefore covers every output admitted by the compiler's absolute
// 128-leaf/32-step limits without permitting unbounded pre-parse allocation.
const MAX_JSON_ESCAPED_IDENTIFIER_BYTES: usize = MAX_IDENTIFIER_BYTES * 6;
const MAX_PERSISTED_LEAF_BYTES: usize = 3 * MAX_CANONICAL_VALUE_BYTES
    + (16 + 6 * ABSOLUTE_MAX_DEPTH) * MAX_JSON_ESCAPED_IDENTIFIER_BYTES
    + 128 * 1024;
const MAX_PERSISTED_MANIFEST_BYTES: usize = 1024 + ABSOLUTE_MAX_LEAVES * MAX_PERSISTED_LEAF_BYTES;

fn canonical_targets_bytes(input: &TargetSnapshotInput) -> Result<Vec<u8>, String> {
    if input.targets.is_empty() {
        return Err("target snapshot cannot be empty".to_string());
    }
    let mut targets = input
        .targets
        .iter()
        .map(canonical_value_bytes)
        .collect::<Result<Vec<_>, _>>()?;
    targets.sort();
    if targets.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("target snapshot has duplicate canonical targets".to_string());
    }
    let mut out = b"{\"targets\":[".to_vec();
    for (index, target) in targets.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        out.extend_from_slice(target);
    }
    out.extend_from_slice(b"]}");
    if out.len() > MAX_CANONICAL_VALUE_BYTES {
        return Err("target snapshot exceeds canonical byte limit".to_string());
    }
    Ok(out)
}

fn canonical_value_bytes(value: &CanonicalValue) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    canonical_value_into(&mut out, value, 0)?;
    if out.len() > MAX_CANONICAL_VALUE_BYTES {
        return Err("canonical value exceeds byte limit".to_string());
    }
    Ok(out)
}

fn canonical_value_into(
    out: &mut Vec<u8>,
    value: &CanonicalValue,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_CANONICAL_VALUE_DEPTH {
        return Err("canonical value exceeds depth limit".to_string());
    }
    match value {
        CanonicalValue::Null => out.extend_from_slice(b"null"),
        CanonicalValue::Bool(value) => {
            out.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        CanonicalValue::Integer(value) => out.extend_from_slice(value.to_string().as_bytes()),
        CanonicalValue::String(value) => {
            if value.len() > MAX_CANONICAL_STRING_BYTES {
                return Err("canonical string exceeds byte limit".to_string());
            }
            push_json_string(out, value)
        }
        CanonicalValue::Array(values) => {
            if values.len() > MAX_CANONICAL_ARRAY_ITEMS {
                return Err("canonical array exceeds item limit".to_string());
            }
            out.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                canonical_value_into(out, value, depth + 1)?;
            }
            out.push(b']');
        }
        CanonicalValue::Object(fields) => {
            if fields.len() > MAX_CANONICAL_OBJECT_FIELDS {
                return Err("canonical object exceeds field limit".to_string());
            }
            let mut normalized = Vec::with_capacity(fields.len());
            for field in fields {
                let key = field.key.clone();
                if key.is_empty() {
                    return Err("object key cannot be empty".to_string());
                }
                if key.len() > MAX_CANONICAL_STRING_BYTES {
                    return Err("object key exceeds byte limit".to_string());
                }
                let mut nested = Vec::new();
                canonical_value_into(&mut nested, &field.value, depth + 1)?;
                normalized.push((key, nested));
            }
            normalized.sort_by(|left, right| left.0.cmp(&right.0));
            if normalized.windows(2).any(|pair| pair[0].0 == pair[1].0) {
                return Err("object has duplicate exact keys".to_string());
            }
            out.push(b'{');
            for (index, (key, value)) in normalized.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                push_json_string(out, key);
                out.push(b':');
                out.extend_from_slice(value);
            }
            out.push(b'}');
        }
    };
    Ok(())
}

fn freeze(bytes: Vec<u8>) -> FrozenBytes {
    FrozenBytes {
        digest: digest_bytes(&bytes),
        bytes,
    }
}
fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(format!("{:x}", Sha256::digest(bytes)))
}
fn parse_digest(value: &str) -> Result<Sha256Digest, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("digest must be a 64-character SHA-256 hex value".to_string());
    }
    Ok(Sha256Digest(value))
}
fn normalized_descriptor(
    descriptor: IndirectionDescriptor,
) -> Result<IndirectionDescriptor, String> {
    Ok(match descriptor {
        IndirectionDescriptor::Alias { alias } => IndirectionDescriptor::Alias {
            alias: normalized_identifier(&alias)?,
        },
        IndirectionDescriptor::Plugin { plugin_id, version } => IndirectionDescriptor::Plugin {
            plugin_id: normalized_identifier(&plugin_id)?,
            version: normalized_identifier(&version)?,
        },
        IndirectionDescriptor::Shell {
            shell_id,
            shell_version,
            command,
        } => {
            if command.is_empty() {
                return Err("shell command cannot be empty".to_string());
            }
            if command.len() > MAX_CANONICAL_STRING_BYTES {
                return Err("shell command exceeds byte limit".to_string());
            }
            IndirectionDescriptor::Shell {
                shell_id: normalized_identifier(&shell_id)?,
                shell_version: normalized_identifier(&shell_version)?,
                command,
            }
        }
        IndirectionDescriptor::Child {
            child_identity,
            version,
        } => IndirectionDescriptor::Child {
            child_identity: normalized_identifier(&child_identity)?,
            version: normalized_identifier(&version)?,
        },
    })
}
fn normalized_identifier(value: &str) -> Result<String, String> {
    let value = value.trim().nfc().collect::<String>();
    if value.is_empty() {
        Err("value cannot be empty".to_string())
    } else if value.len() > MAX_IDENTIFIER_BYTES {
        Err("identifier exceeds byte limit".to_string())
    } else {
        Ok(value)
    }
}
fn push_json_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(
        serde_json::to_string(value)
            .expect("string serialization cannot fail")
            .as_bytes(),
    );
}
fn pause(reason: MechanicalResolutionReason) -> ManifestCompilation {
    ManifestCompilation::MechanicalResolutionRequired(MechanicalResolutionPause { reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execass_actor::{
        bind_verified_owner_authority, derive_base_actor_assurance, CallerActorClaims,
        LocalInteractiveEvidence, OwnerAuthoritySourceInput, ServerIngressObservation,
    };

    const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn leaf(label: &str) -> DispatchNode {
        DispatchNode {
            node_id: label.to_string(),
            action: DispatchAction::ResolvedLeaf(Box::new(ResolvedLeafInput {
                logical_action_id: format!("logical.{label}"),
                action_kind: "tool_call".to_string(),
                tool: ToolIdentityInput {
                    tool_id: "connector.send".to_string(),
                    version: "1.0.0".to_string(),
                },
                operands: CanonicalValue::Object(vec![
                    CanonicalField {
                        key: "message".to_string(),
                        value: CanonicalValue::String("cafe\u{301}".to_string()),
                    },
                    CanonicalField {
                        key: "retries".to_string(),
                        value: CanonicalValue::Integer(2),
                    },
                ]),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![
                        CanonicalValue::String("beta".to_string()),
                        CanonicalValue::String("alpha".to_string()),
                    ],
                },
                material_digest: Some(DIGEST.to_string()),
                owner_authority: authority(),
            })),
        }
    }
    fn authority() -> VerifiedOwnerAuthority {
        authority_with("desktop-client", "request-1", 0)
    }
    fn authority_with(client: &str, correlation: &str, mutation: usize) -> VerifiedOwnerAuthority {
        let mut evidence = LocalInteractiveEvidence {
            authenticated_client_id: client.to_string(),
            authenticated_ingress: "native-control".to_string(),
            channel_assurance: "interactive-local".to_string(),
            request_correlation_id: correlation.to_string(),
            source_message_id: Some("message-1".to_string()),
            interactive_owner_verified: true,
        };
        match mutation {
            15 => evidence.authenticated_ingress = "other-ingress".to_string(),
            16 => evidence.channel_assurance = "other-assurance".to_string(),
            17 => evidence.source_message_id = Some("message-2".to_string()),
            _ => {}
        }
        let actor = derive_base_actor_assurance(
            &ServerIngressObservation::LocalInteractive(Box::new(evidence)),
            &CallerActorClaims::default(),
        );
        let mut source = OwnerAuthoritySourceInput {
            normalized_intent: "send the exact message".to_string(),
            instruction_revision: "rev-1".to_string(),
            instruction_bytes: b"send the exact message".to_vec(),
            owner_envelope_revision: "envelope-1".to_string(),
            owner_envelope_json: r#"{"action":"send"}"#.to_string(),
            authority_kind: "original_request".to_string(),
            normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.to_string(),
            policy_revision: 1,
            bound_decision_id: Some("decision-1".to_string()),
            bound_decision_revision: Some(1),
            bound_manifest_bytes: Some(b"manifest-one".to_vec()),
            challenge_nonce_bytes: Some(b"nonce-one".to_vec()),
            created_at: 1_800_000_000_000,
            expires_at: Some(1_800_000_000_100),
        };
        match mutation {
            1 => source.normalized_intent = "send a different message".to_string(),
            2 => source.instruction_revision = "rev-2".to_string(),
            3 => source.instruction_bytes = b"send a different message".to_vec(),
            4 => source.owner_envelope_revision = "envelope-2".to_string(),
            5 => source.owner_envelope_json = r#"{"action":"other"}"#.to_string(),
            6 => source.authority_kind = "policy_snapshot".to_string(),
            7 => source.normalized_scope_json = r#"{"workspace":"Z:\\other"}"#.to_string(),
            8 => source.policy_revision = 2,
            9 => source.bound_decision_id = Some("decision-2".to_string()),
            10 => source.bound_decision_revision = Some(2),
            11 => source.bound_manifest_bytes = Some(b"manifest-two".to_vec()),
            12 => source.challenge_nonce_bytes = Some(b"nonce-two".to_vec()),
            13 => source.created_at += 1,
            14 => source.expires_at = Some(1_800_000_000_200),
            18 => {
                source.normalized_scope_json =
                    format!("\"{}\"", "x".repeat(MAX_CANONICAL_VALUE_BYTES))
            }
            _ => {}
        }
        bind_verified_owner_authority(&actor, source).expect("verified owner authority")
    }
    fn tree(nodes: Vec<DispatchNode>) -> DispatchTree {
        DispatchTree {
            root_id: "root".to_string(),
            nodes,
        }
    }
    fn ready(tree: &DispatchTree) -> CanonicalLeafManifest {
        ready_with(tree, &ServerResolutionRegistry::default())
    }
    fn ready_with(
        tree: &DispatchTree,
        resolutions: &ServerResolutionRegistry,
    ) -> CanonicalLeafManifest {
        match compile_dispatch(tree, resolutions) {
            ManifestCompilation::Ready(value) => value,
            other => panic!("expected ready, got {other:?}"),
        }
    }
    fn paused(tree: &DispatchTree) -> MechanicalResolutionReason {
        paused_with(tree, &ServerResolutionRegistry::default())
    }
    fn paused_with(
        tree: &DispatchTree,
        resolutions: &ServerResolutionRegistry,
    ) -> MechanicalResolutionReason {
        match compile_dispatch(tree, resolutions) {
            ManifestCompilation::MechanicalResolutionRequired(value) => value.reason,
            other => panic!("expected pause, got {other:?}"),
        }
    }
    fn descriptor_for(kind: StructureKind, label: &str) -> IndirectionDescriptor {
        match kind {
            StructureKind::Alias => IndirectionDescriptor::Alias {
                alias: label.to_string(),
            },
            StructureKind::Plugin => IndirectionDescriptor::Plugin {
                plugin_id: label.to_string(),
                version: "1.0.0".to_string(),
            },
            StructureKind::Shell => IndirectionDescriptor::Shell {
                shell_id: "powershell".to_string(),
                shell_version: "7.5".to_string(),
                command: label.to_string(),
            },
            StructureKind::Child => IndirectionDescriptor::Child {
                child_identity: label.to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }
    fn action_for(kind: StructureKind, label: &str) -> DispatchAction {
        match kind {
            StructureKind::Alias => DispatchAction::Alias {
                alias: label.to_string(),
            },
            StructureKind::Plugin => DispatchAction::Plugin {
                plugin_id: label.to_string(),
                version: "1.0.0".to_string(),
            },
            StructureKind::Shell => DispatchAction::Shell {
                shell_id: "powershell".to_string(),
                shell_version: "7.5".to_string(),
                command: label.to_string(),
            },
            StructureKind::Child => DispatchAction::Child {
                child_identity: label.to_string(),
                version: "1.0.0".to_string(),
            },
        }
    }
    fn resolution(descriptor: IndirectionDescriptor, target: &str) -> ServerResolutionEntryInput {
        ServerResolutionEntryInput {
            descriptor,
            target_node_id: target.to_string(),
            resolver_id: "server-resolver".to_string(),
            resolver_version: "1.0.0".to_string(),
            resolution_material_digest: DIGEST.to_string(),
        }
    }

    #[test]
    fn key_order_nfc_and_restart_determinism() {
        let first = tree(vec![leaf("root")]);
        let mut reordered = leaf("root");
        if let DispatchAction::ResolvedLeaf(leaf) = &mut reordered.action {
            leaf.operands = CanonicalValue::Object(vec![
                CanonicalField {
                    key: "retries".to_string(),
                    value: CanonicalValue::Integer(2),
                },
                CanonicalField {
                    key: "message".to_string(),
                    value: CanonicalValue::String("cafe\u{301}".to_string()),
                },
            ]);
        }
        let second = tree(vec![reordered]);
        assert_eq!(
            ready(&first).canonical().bytes(),
            ready(&second).canonical().bytes()
        );
        assert_eq!(ready(&first), ready(&first));
    }
    #[test]
    fn persisted_manifest_exact_roundtrip_rebind_is_identical() {
        let manifest = ready(&tree(vec![leaf("root")]));
        let rebound = rebind_persisted_manifest_for_routine_occurrence(
            manifest.canonical().bytes(),
            manifest.canonical().digest().as_hex(),
            &[RoutineOccurrenceLeafBinding {
                persisted_logical_action_id: "logical.root".to_string(),
                occurrence_logical_action_id: "logical.root".to_string(),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![
                        CanonicalValue::String("alpha".to_string()),
                        CanonicalValue::String("beta".to_string()),
                    ],
                },
            }],
        )
        .expect("exact persisted manifest should restore");
        assert_eq!(rebound, manifest);
    }

    #[test]
    fn routine_occurrence_rebind_changes_only_identity_and_target_snapshot() {
        let dispatch = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Alias {
                    alias: "saved-alias".to_string(),
                },
            },
            leaf("target"),
        ]);
        let registry = ServerResolutionRegistry::try_new(vec![resolution(
            IndirectionDescriptor::Alias {
                alias: "saved-alias".to_string(),
            },
            "target",
        )])
        .unwrap();
        let persisted = ready_with(&dispatch, &registry);
        let rebound = rebind_persisted_manifest_for_routine_occurrence(
            persisted.canonical().bytes(),
            persisted.canonical().digest().as_hex(),
            &[RoutineOccurrenceLeafBinding {
                persisted_logical_action_id: "logical.target".to_string(),
                occurrence_logical_action_id: "routine-7.occurrence-42.action-1".to_string(),
                target_snapshot: TargetSnapshotInput {
                    targets: vec![CanonicalValue::String("fresh-member".to_string())],
                },
            }],
        )
        .expect("valid occurrence binding");
        let before = &persisted.leaves()[0];
        let after = &rebound.leaves()[0];
        assert_eq!(
            after.logical_action_id(),
            "routine-7.occurrence-42.action-1"
        );
        assert_ne!(after.target_snapshot(), before.target_snapshot());
        assert_ne!(after.canonical(), before.canonical());
        assert_ne!(rebound.canonical(), persisted.canonical());
        assert_eq!(after.node_id(), before.node_id());
        assert_eq!(after.action_kind(), before.action_kind());
        assert_eq!(after.tool(), before.tool());
        assert_eq!(after.operands(), before.operands());
        assert_eq!(after.material_digest(), before.material_digest());
        assert_eq!(after.resolution_path(), before.resolution_path());
        assert_eq!(after.owner_authority(), before.owner_authority());
    }

    #[test]
    fn trusted_digest_prevents_material_envelope_substitution() {
        let manifest = ready(&tree(vec![leaf("root")]));
        let canonical = std::str::from_utf8(manifest.canonical().bytes()).unwrap();
        let binding = [RoutineOccurrenceLeafBinding {
            persisted_logical_action_id: "logical.root".to_string(),
            occurrence_logical_action_id: "occurrence.action".to_string(),
            target_snapshot: TargetSnapshotInput {
                targets: vec![CanonicalValue::String("fresh-member".to_string())],
            },
        }];
        for substituted in [
            canonical.replacen("connector.send", "connector.drop", 1),
            canonical.replacen("\"retries\":2", "\"retries\":3", 1),
            canonical.replacen(DIGEST, &"b".repeat(64), 1),
            canonical.replacen("desktop-client", "hostile-client", 1),
        ] {
            assert!(rebind_persisted_manifest_for_routine_occurrence(
                substituted.as_bytes(),
                manifest.canonical().digest().as_hex(),
                &binding,
            )
            .is_err());
        }
    }

    #[test]
    fn persisted_manifest_rejects_malformed_unknown_duplicate_noncanonical_and_bad_digest() {
        let manifest = ready(&tree(vec![leaf("root")]));
        let canonical = std::str::from_utf8(manifest.canonical().bytes()).unwrap();
        let binding = [RoutineOccurrenceLeafBinding {
            persisted_logical_action_id: "logical.root".to_string(),
            occurrence_logical_action_id: "occurrence.action".to_string(),
            target_snapshot: TargetSnapshotInput {
                targets: vec![CanonicalValue::String("fresh-member".to_string())],
            },
        }];
        let malformed = &canonical.as_bytes()[..canonical.len() - 1];
        let unknown = canonical.replacen("{\"leaves\":", "{\"unknown\":true,\"leaves\":", 1);
        let duplicate = canonical.replacen("{\"leaves\":", "{\"leaves\":[],\"leaves\":", 1);
        let noncanonical = canonical.replacen("{\"leaves\":", "{ \"leaves\":", 1);
        for hostile in [
            malformed,
            unknown.as_bytes(),
            duplicate.as_bytes(),
            noncanonical.as_bytes(),
        ] {
            assert!(rebind_persisted_manifest_for_routine_occurrence(
                hostile,
                manifest.canonical().digest().as_hex(),
                &binding,
            )
            .is_err());
        }
        assert!(rebind_persisted_manifest_for_routine_occurrence(
            manifest.canonical().bytes(),
            &"b".repeat(64),
            &binding,
        )
        .is_err());
    }

    #[test]
    fn oversized_persisted_manifest_is_rejected_before_json_parse() {
        assert_eq!(MAX_PERSISTED_MANIFEST_BYTES, 79 * 1024 * 1024 + 1024);
        assert!(normalized_identifier(&"x".repeat(MAX_IDENTIFIER_BYTES + 1)).is_err());
        assert!(
            canonicalize_owner_authority(&authority_with("desktop-client", "request-1", 18))
                .is_err()
        );
        let invalid_json = b"definitely-not-json";
        assert_eq!(
            parse_persisted_manifest_with_limit(invalid_json, invalid_json.len() - 1)
                .err()
                .as_deref(),
            Some("persisted manifest exceeds byte limit")
        );
    }

    #[test]
    fn nfc_distinct_operand_bytes_and_digests_remain_distinct() {
        let decomposed = tree(vec![leaf("root")]);
        let mut composed = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut composed.action {
            input.operands = CanonicalValue::Object(vec![
                CanonicalField {
                    key: "message".to_string(),
                    value: CanonicalValue::String("caf\u{e9}".to_string()),
                },
                CanonicalField {
                    key: "retries".to_string(),
                    value: CanonicalValue::Integer(2),
                },
            ]);
        }
        let composed = tree(vec![composed]);
        assert_ne!(
            ready(&decomposed).leaves()[0].operands().bytes(),
            ready(&composed).leaves()[0].operands().bytes()
        );
        assert_ne!(
            ready(&decomposed).leaves()[0].canonical().digest(),
            ready(&composed).leaves()[0].canonical().digest()
        );
    }
    #[test]
    fn composite_preserves_intentional_execution_order() {
        let manifest = ready(&tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["second".to_string(), "first".to_string()],
                },
            },
            leaf("first"),
            leaf("second"),
        ]));
        assert_eq!(
            manifest
                .leaves()
                .iter()
                .map(|leaf| leaf.node_id())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
    }
    #[test]
    fn unresolved_indirections_pause_with_zero_leaves() {
        for kind in [
            StructureKind::Alias,
            StructureKind::Plugin,
            StructureKind::Shell,
            StructureKind::Child,
        ] {
            let reason = paused(&tree(vec![DispatchNode {
                node_id: "root".to_string(),
                action: action_for(kind, "descriptor"),
            }]));
            assert!(matches!(
                reason,
                MechanicalResolutionReason::UnresolvedAlias { .. }
                    | MechanicalResolutionReason::UnresolvedPlugin { .. }
                    | MechanicalResolutionReason::UnresolvedShell { .. }
                    | MechanicalResolutionReason::UnresolvedChild { .. }
            ));
        }
    }
    #[test]
    fn ambiguous_indirections_pause_with_zero_leaves() {
        for kind in [
            StructureKind::Alias,
            StructureKind::Plugin,
            StructureKind::Shell,
            StructureKind::Child,
        ] {
            let descriptor = descriptor_for(kind, "descriptor");
            let registry = ServerResolutionRegistry::try_new(vec![
                resolution(descriptor.clone(), "one"),
                resolution(descriptor, "two"),
            ])
            .unwrap();
            let reason = paused_with(
                &tree(vec![DispatchNode {
                    node_id: "root".to_string(),
                    action: action_for(kind, "descriptor"),
                }]),
                &registry,
            );
            assert!(matches!(
                reason,
                MechanicalResolutionReason::AmbiguousAlias { .. }
                    | MechanicalResolutionReason::AmbiguousPlugin { .. }
                    | MechanicalResolutionReason::AmbiguousShell { .. }
                    | MechanicalResolutionReason::AmbiguousChild { .. }
            ));
        }
    }
    #[test]
    fn resolved_adversarial_indirections_bind_every_wrapper() {
        let direct = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["a".to_string(), "b".to_string()],
                },
            },
            leaf("a"),
            leaf("b"),
        ]);
        let indirect = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Alias {
                    alias: "alias".to_string(),
                },
            },
            DispatchNode {
                node_id: "plugin".to_string(),
                action: DispatchAction::Plugin {
                    plugin_id: "p".to_string(),
                    version: "1.0.0".to_string(),
                },
            },
            DispatchNode {
                node_id: "shell".to_string(),
                action: DispatchAction::Shell {
                    shell_id: "powershell".to_string(),
                    shell_version: "7.5".to_string(),
                    command: "cmd".to_string(),
                },
            },
            DispatchNode {
                node_id: "child".to_string(),
                action: DispatchAction::Child {
                    child_identity: "worker".to_string(),
                    version: "1.0.0".to_string(),
                },
            },
            DispatchNode {
                node_id: "real-root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["a".to_string(), "b".to_string()],
                },
            },
            leaf("a"),
            leaf("b"),
        ]);
        let registry = ServerResolutionRegistry::try_new(vec![
            resolution(
                IndirectionDescriptor::Alias {
                    alias: "alias".into(),
                },
                "plugin",
            ),
            resolution(
                IndirectionDescriptor::Plugin {
                    plugin_id: "p".into(),
                    version: "1.0.0".into(),
                },
                "shell",
            ),
            resolution(
                IndirectionDescriptor::Shell {
                    shell_id: "powershell".into(),
                    shell_version: "7.5".into(),
                    command: "cmd".into(),
                },
                "child",
            ),
            resolution(
                IndirectionDescriptor::Child {
                    child_identity: "worker".into(),
                    version: "1.0.0".into(),
                },
                "real-root",
            ),
        ])
        .unwrap();
        let direct_leaves = ready(&direct)
            .leaves()
            .iter()
            .map(|leaf| leaf.canonical().digest().clone())
            .collect::<Vec<_>>();
        let indirect_manifest = ready_with(&indirect, &registry);
        let indirect_leaves = indirect_manifest
            .leaves()
            .iter()
            .map(|leaf| leaf.canonical().digest().clone())
            .collect::<Vec<_>>();
        assert_ne!(direct_leaves, indirect_leaves);
        assert!(indirect_manifest
            .leaves()
            .iter()
            .all(|leaf| leaf.resolution_path().len() == 4));
    }
    #[test]
    fn duplicates_cycles_and_limits_pause() {
        let duplicate = tree(vec![leaf("root"), leaf("root")]);
        assert!(matches!(
            paused(&duplicate),
            MechanicalResolutionReason::DuplicateNodeIdentity { .. }
        ));
        let cycle = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["other".to_string()],
                },
            },
            DispatchNode {
                node_id: "other".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["root".to_string()],
                },
            },
        ]);
        assert!(matches!(
            paused(&cycle),
            MechanicalResolutionReason::CycleDetected { .. }
        ));
        let overflow = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["a".to_string(), "b".to_string()],
                },
            },
            leaf("a"),
            leaf("b"),
        ]);
        assert!(matches!(
            compile_dispatch_with_limits(
                &overflow,
                &ServerResolutionRegistry::default(),
                CompileLimits {
                    max_depth: 4,
                    max_nodes: 8,
                    max_leaves: 1
                }
            ),
            ManifestCompilation::MechanicalResolutionRequired(MechanicalResolutionPause {
                reason: MechanicalResolutionReason::LeafLimitExceeded { .. }
            })
        ));
        let depth = tree(vec![
            DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite {
                    children: vec!["a".to_string()],
                },
            },
            leaf("a"),
        ]);
        assert!(matches!(
            compile_dispatch_with_limits(
                &depth,
                &ServerResolutionRegistry::default(),
                CompileLimits {
                    max_depth: 0,
                    max_nodes: 8,
                    max_leaves: 8
                }
            ),
            ManifestCompilation::MechanicalResolutionRequired(MechanicalResolutionPause {
                reason: MechanicalResolutionReason::DepthLimitExceeded { .. }
            })
        ));
        assert!(matches!(
            compile_dispatch_with_limits(
                &depth,
                &ServerResolutionRegistry::default(),
                CompileLimits {
                    max_depth: 8,
                    max_nodes: 1,
                    max_leaves: 8
                }
            ),
            ManifestCompilation::MechanicalResolutionRequired(MechanicalResolutionPause {
                reason: MechanicalResolutionReason::NodeLimitExceeded { .. }
            })
        ));
    }
    #[test]
    fn mutations_change_leaf_digest() {
        let baseline = ready(&tree(vec![leaf("root")]));
        let digest = baseline.leaves()[0].canonical().digest().clone();
        for mutate in 0..24 {
            let mut node = leaf("root");
            if let DispatchAction::ResolvedLeaf(input) = &mut node.action {
                match mutate {
                    0 => input.operands = CanonicalValue::Integer(9),
                    1 => {
                        input.target_snapshot.targets =
                            vec![CanonicalValue::String("other".to_string())]
                    }
                    2 => input.tool.tool_id = "connector.other".to_string(),
                    3 => input.tool.version = "2.0.0".to_string(),
                    4 => {
                        input.material_digest = Some(
                            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                                .to_string(),
                        )
                    }
                    5..=21 => {
                        input.owner_authority =
                            authority_with("desktop-client", "request-1", mutate - 4)
                    }
                    22 => {
                        input.owner_authority = authority_with("desktop-client-2", "request-1", 0)
                    }
                    _ => input.owner_authority = authority_with("desktop-client", "request-2", 0),
                }
            }
            assert_ne!(
                ready(&tree(vec![node])).leaves()[0].canonical().digest(),
                &digest
            );
        }
    }
    #[test]
    fn nonhuman_actor_cannot_issue_manifest_authority() {
        let actor = derive_base_actor_assurance(
            &ServerIngressObservation::Model {
                model_id: "model-1".to_string(),
            },
            &CallerActorClaims::default(),
        );
        assert!(bind_verified_owner_authority(
            &actor,
            OwnerAuthoritySourceInput {
                normalized_intent: "forged intent".to_string(),
                instruction_revision: "rev-1".to_string(),
                instruction_bytes: b"forged instruction".to_vec(),
                owner_envelope_revision: "envelope-1".to_string(),
                owner_envelope_json: r#"{"action":"forged"}"#.to_string(),
                authority_kind: "original_request".to_string(),
                normalized_scope_json: r#"{"workspace":"Z:\\carsinos"}"#.to_string(),
                policy_revision: 1,
                bound_decision_id: None,
                bound_decision_revision: None,
                bound_manifest_bytes: None,
                challenge_nonce_bytes: None,
                created_at: 1_800_000_000_000,
                expires_at: None,
            }
        )
        .is_err());
    }
    #[test]
    fn broad_targets_freeze_in_sorted_order_and_duplicates_pause() {
        let one = ready(&tree(vec![leaf("root")]));
        let mut node = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut node.action {
            input.target_snapshot.targets.reverse();
        }
        let two = ready(&tree(vec![node]));
        assert_eq!(
            one.leaves()[0].target_snapshot().bytes(),
            two.leaves()[0].target_snapshot().bytes()
        );
        let mut duplicate = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut duplicate.action {
            input.target_snapshot.targets = vec![
                CanonicalValue::String("a".to_string()),
                CanonicalValue::String("a".to_string()),
            ];
        }
        assert!(matches!(
            paused(&tree(vec![duplicate])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
    }
    #[test]
    fn rejects_duplicate_exact_keys_and_empty_composite() {
        let mut node = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut node.action {
            input.operands = CanonicalValue::Object(vec![
                CanonicalField {
                    key: "same".to_string(),
                    value: CanonicalValue::Integer(1),
                },
                CanonicalField {
                    key: "same".to_string(),
                    value: CanonicalValue::Integer(2),
                },
            ]);
        }
        assert!(matches!(
            paused(&tree(vec![node])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
        assert!(matches!(
            paused(&tree(vec![DispatchNode {
                node_id: "root".to_string(),
                action: DispatchAction::Composite { children: vec![] }
            }])),
            MechanicalResolutionReason::EmptyComposite { .. }
        ));
    }
    #[test]
    fn duplicate_logical_action_and_value_bounds_pause() {
        let mut duplicate = leaf("second");
        if let DispatchAction::ResolvedLeaf(input) = &mut duplicate.action {
            input.logical_action_id = "logical.first".to_string();
        }
        assert!(matches!(
            paused(&tree(vec![
                DispatchNode {
                    node_id: "root".to_string(),
                    action: DispatchAction::Composite {
                        children: vec!["first".to_string(), "second".to_string()]
                    }
                },
                leaf("first"),
                duplicate
            ])),
            MechanicalResolutionReason::DuplicateLogicalActionIdentity { .. }
        ));
        let mut hostile = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut hostile.action {
            input.operands = CanonicalValue::String("x".repeat(MAX_CANONICAL_STRING_BYTES + 1));
        }
        assert!(matches!(
            paused(&tree(vec![hostile])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
        let mut nested = CanonicalValue::Null;
        for _ in 0..=MAX_CANONICAL_VALUE_DEPTH {
            nested = CanonicalValue::Array(vec![nested]);
        }
        let mut depth = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut depth.action {
            input.operands = nested;
        }
        assert!(matches!(
            paused(&tree(vec![depth])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
        let mut array = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut array.action {
            input.operands =
                CanonicalValue::Array(vec![CanonicalValue::Null; MAX_CANONICAL_ARRAY_ITEMS + 1]);
        }
        assert!(matches!(
            paused(&tree(vec![array])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
        let mut total_bytes = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut total_bytes.action {
            input.operands = CanonicalValue::Array(vec![
                CanonicalValue::String("x".repeat(128));
                MAX_CANONICAL_ARRAY_ITEMS
            ]);
        }
        assert!(matches!(
            paused(&tree(vec![total_bytes])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
        let mut object = leaf("root");
        if let DispatchAction::ResolvedLeaf(input) = &mut object.action {
            input.operands = CanonicalValue::Object(
                (0..=MAX_CANONICAL_OBJECT_FIELDS)
                    .map(|index| CanonicalField {
                        key: index.to_string(),
                        value: CanonicalValue::Null,
                    })
                    .collect(),
            );
        }
        assert!(matches!(
            paused(&tree(vec![object])),
            MechanicalResolutionReason::InvalidLeaf { .. }
        ));
    }
    #[test]
    fn indirection_descriptor_and_resolution_mutations_change_or_pause() {
        let cases = [
            (StructureKind::Alias, "alias-one", "alias-two"),
            (StructureKind::Plugin, "plugin-one", "plugin-two"),
            (StructureKind::Shell, "Write-Output one", "Write-Output two"),
            (StructureKind::Child, "child-one", "child-two"),
        ];
        for (kind, first, second) in cases {
            let first_descriptor = descriptor_for(kind, first);
            let first_tree = tree(vec![
                DispatchNode {
                    node_id: "root".to_string(),
                    action: action_for(kind, first),
                },
                leaf("target"),
            ]);
            let first_registry =
                ServerResolutionRegistry::try_new(vec![resolution(first_descriptor, "target")])
                    .unwrap();
            let first_digest = ready_with(&first_tree, &first_registry)
                .canonical()
                .digest()
                .clone();

            let second_descriptor = descriptor_for(kind, second);
            let second_tree = tree(vec![
                DispatchNode {
                    node_id: "root".to_string(),
                    action: action_for(kind, second),
                },
                leaf("target"),
            ]);
            let mut second_entry = resolution(second_descriptor, "target");
            second_entry.resolver_version = "2.0.0".to_string();
            second_entry.resolution_material_digest =
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
            let second_registry = ServerResolutionRegistry::try_new(vec![second_entry]).unwrap();
            let second_digest = ready_with(&second_tree, &second_registry)
                .canonical()
                .digest()
                .clone();
            assert_ne!(first_digest, second_digest);
            assert!(matches!(
                paused_with(&second_tree, &first_registry),
                MechanicalResolutionReason::UnresolvedAlias { .. }
                    | MechanicalResolutionReason::UnresolvedPlugin { .. }
                    | MechanicalResolutionReason::UnresolvedShell { .. }
                    | MechanicalResolutionReason::UnresolvedChild { .. }
            ));
        }
    }
    #[test]
    fn oversized_internal_limits_are_clamped_to_absolute_caps() {
        let mut nodes = Vec::new();
        for index in 0..=ABSOLUTE_MAX_DEPTH {
            let node_id = if index == 0 {
                "root".to_string()
            } else {
                format!("node-{index}")
            };
            let child = format!("node-{}", index + 1);
            nodes.push(DispatchNode {
                node_id,
                action: DispatchAction::Composite {
                    children: vec![child],
                },
            });
        }
        nodes.push(leaf(&format!("node-{}", ABSOLUTE_MAX_DEPTH + 1)));
        assert!(matches!(
            compile_dispatch_with_limits(
                &tree(nodes),
                &ServerResolutionRegistry::default(),
                CompileLimits {
                    max_depth: usize::MAX,
                    max_nodes: usize::MAX,
                    max_leaves: usize::MAX,
                }
            ),
            ManifestCompilation::MechanicalResolutionRequired(MechanicalResolutionPause {
                reason: MechanicalResolutionReason::DepthLimitExceeded {
                    max_depth: ABSOLUTE_MAX_DEPTH
                }
            })
        ));
    }
    #[test]
    fn sha256_matches_standard_vector() {
        assert_eq!(
            digest_bytes(b"abc").as_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
