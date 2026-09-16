/// z42 project manifest data types.
///
/// Both single-project and workspace configurations live in `z42.toml`.
/// The presence of `[project]` vs `[workspace]` tables determines the file's role
/// (they may coexist when a workspace root is also a project).
///
/// File name constant: [`FILE_NAME`].
/// See docs/reference/src/toolchain/z42-toml.md for the full design.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Canonical file name for z42 project / workspace manifests.
pub const FILE_NAME: &str = "z42.toml";

// ── Shared primitives ─────────────────────────────────────────────────────────

/// VM execution mode selectable per profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecModeConfig {
    Interp,
    Jit,
    Aot,
}

impl Default for ExecModeConfig {
    fn default() -> Self { Self::Interp }
}

/// Compilation output granularity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmitKind {
    /// Debug IR JSON (`.z42ir.json`)
    Ir,
    /// Per-file bytecode (`.zbc`)
    Zbc,
    /// Module manifest + per-file `.zbc` under `.cache/` (`.zpkg` indexed mode)
    Zmod,
    /// Bundled assembly (`.zpkg` packed mode)
    Zlib,
}

impl Default for EmitKind {
    fn default() -> Self { Self::Zlib }
}

// ── `.z42proj` ────────────────────────────────────────────────────────────────

/// `[project]` table — project identity and entry point.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectMeta {
    /// Project name; also used as the output file base name.
    pub name: String,
    /// SemVer string, e.g. `"0.1.0"`.
    pub version: String,
    /// `"exe"` (has entry point) or `"lib"` (library).
    pub kind: ProjectKind,
    /// Required for `kind = "exe"`: fully-qualified entry function.
    /// Example: `"Hello.Main"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    /// Root namespace; defaults to `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Exe,
    Lib,
}

/// `[sources]` table — source file discovery via glob patterns.
#[derive(Debug, Serialize, Deserialize)]
pub struct SourcesConfig {
    /// Glob patterns relative to the `.z42proj` file.
    /// Default: `["src/**/*.z42"]`
    #[serde(default = "default_include")]
    pub include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

fn default_include() -> Vec<String> {
    vec!["src/**/*.z42".to_owned()]
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self { include: default_include(), exclude: vec![] }
    }
}

/// `[build]` table — default build options (overridable per profile).
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    pub emit: EmitKind,
    #[serde(default)]
    pub mode: ExecModeConfig,
    /// Enable incremental compilation via source-hash comparison.
    #[serde(default = "default_true")]
    pub incremental: bool,
    /// Output directory for compiled artifacts.
    #[serde(default = "default_out_dir")]
    pub out_dir: String,
}

fn default_true() -> bool { true }
fn default_out_dir() -> String { "dist".to_owned() }

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            emit: EmitKind::default(),
            mode: ExecModeConfig::default(),
            incremental: true,
            out_dir: default_out_dir(),
        }
    }
}

/// `[[dependency]]` array entry.
#[derive(Debug, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    /// SemVer constraint, e.g. `">=0.1"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Path to a local `.z42proj`; takes priority over registry lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `[profile.<name>]` table — per-profile overrides.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<ExecModeConfig>,
    /// Optimisation level 0–3.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optimize: Option<u8>,
    /// Include debug information (default: true in debug, false in release).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
    /// Strip debug information from output (default: false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strip: Option<bool>,
}

/// Root of a `z42.toml` file that contains a `[project]` table.
#[derive(Debug, Serialize, Deserialize)]
pub struct Z42Proj {
    pub project: ProjectMeta,
    #[serde(default)]
    pub sources: SourcesConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default, rename = "dependency")]
    pub dependencies: Vec<Dependency>,
    /// Named profiles, e.g. `profile.debug`, `profile.release`.
    #[serde(default)]
    pub profile: HashMap<String, ProfileConfig>,
}

impl Z42Proj {
    /// Parse a `z42.toml` file from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// Serialise back to TOML (for generation / normalisation).
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Effective namespace: explicit or falls back to project name.
    pub fn namespace(&self) -> &str {
        self.project.namespace.as_deref().unwrap_or(&self.project.name)
    }

    /// Resolve the active `ProfileConfig` by name, merged on top of `[build]` defaults.
    pub fn resolved_profile(&self, profile_name: &str) -> ResolvedProfile {
        let p = self.profile.get(profile_name);
        ResolvedProfile {
            mode:     p.and_then(|p| p.mode.clone()).unwrap_or_else(|| self.build.mode.clone()),
            optimize: p.and_then(|p| p.optimize).unwrap_or(0),
            debug:    p.and_then(|p| p.debug).unwrap_or(profile_name == "debug"),
            strip:    p.and_then(|p| p.strip).unwrap_or(false),
        }
    }
}

/// Fully-resolved build profile after merging `[build]` + `[profile.<name>]`.
#[derive(Debug)]
pub struct ResolvedProfile {
    pub mode:     ExecModeConfig,
    pub optimize: u8,
    pub debug:    bool,
    pub strip:    bool,
}

// ── `.z42sln` ─────────────────────────────────────────────────────────────────

/// Inline dependency spec used in `[workspace.dependencies]`.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceDep {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// `[workspace]` table inside a `.z42sln`.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkspaceMeta {
    /// Relative paths to directories containing a `.z42proj` each.
    pub members: Vec<String>,
    /// Shared dependency versions; individual projects reference them
    /// with `version = "workspace"`.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dependencies: HashMap<String, WorkspaceDep>,
}

/// Root of a `z42.toml` file that contains a `[workspace]` table.
#[derive(Debug, Serialize, Deserialize)]
pub struct Z42Sln {
    pub workspace: WorkspaceMeta,
    /// Workspace-level profile overrides inherited by all members.
    #[serde(default)]
    pub profile: HashMap<String, ProfileConfig>,
}

impl Z42Sln {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}
