# Data Model: Cross-Ecosystem Release Planning Foundation

## 1. WorkspaceConfiguration

**Purpose**: Captures repository-wide defaults and overrides loaded from `monochange.toml`.

### Fields

- `root_path`: Canonical repository root.
- `enabled_ecosystems`: Set of ecosystem adapters explicitly enabled or auto-detected.
- `default_parent_bump`: Default bump rule for dependency propagation when no stronger signal exists.
- `include_paths`: Optional additional paths to scan.
- `exclude_paths`: Optional paths or patterns to exclude from discovery.
- `version_groups`: Collection of defined version groups.
- `package_overrides`: Optional per-package behavior overrides.
- `release_defaults`: Shared defaults for planning output, private-package handling, and warning behavior.

### Validation Rules

- The root path must exist and be readable.
- Version group names must be unique.
- A package may not belong to conflicting version groups.
- Explicit include and exclude patterns must resolve deterministically.

## 2. EcosystemAdapterDescriptor

**Purpose**: Describes one ecosystem integration and the capabilities it contributes.

### Fields

- `ecosystem_id`: Stable identifier such as cargo, npm, deno, or dart.
- `manifest_kinds`: Native file types recognized by the adapter.
- `workspace_kinds`: Workspace-level definitions recognized by the adapter.
- `supports_globs`: Whether native workspace patterns may include glob expansion.
- `supports_semver_provider`: Whether the adapter can contribute compatibility evidence.
- `supports_single_package_mode`: Whether standalone packages are supported.

### Relationships

- One adapter discovers many `PackageRecord` instances.
- One adapter may contribute zero or one `CompatibilityAssessment` per changed package.

## 3. PackageRecord

**Purpose**: Normalized representation of a discovered package regardless of ecosystem.

### Fields

- `package_id`: Stable internal identifier.
- `name`: Display/package name.
- `ecosystem_id`: Owning ecosystem.
- `manifest_path`: Manifest location.
- `workspace_root`: Root workspace or standalone root.
- `current_version`: Parsed current version.
- `publish_state`: Public, private, unpublished, or excluded.
- `version_group_id`: Optional link to a version group.
- `metadata`: Adapter-specific normalized metadata such as workspace membership or publish settings.

### Validation Rules

- Each manifest path must map to exactly one package record after normalization.
- Version values must be parseable or explicitly marked as unsupported.
- Package names must be unique within the same workspace scope unless the native ecosystem permits a disambiguating rule.

## 4. DependencyEdge

**Purpose**: Directed relationship from one package to another.

### Fields

- `from_package_id`: Dependent package.
- `to_package_id`: Dependency package.
- `dependency_kind`: Runtime, development, build, peer, workspace, or ecosystem-specific equivalent.
- `source_kind`: Native manifest entry, workspace-generated relationship, or normalized transitive edge.
- `version_constraint`: Declared compatibility range if available.
- `is_optional`: Whether the dependency is optional.
- `is_direct`: Whether the edge came directly from a manifest.

### Validation Rules

- Direct edges must preserve source manifest provenance.
- Duplicate direct edges between the same nodes and dependency kind should be normalized.
- Cycles must be representable without causing planner failure.

## 5. VersionGroup

**Purpose**: Keeps a set of packages on the same planned version.

### Fields

- `group_id`: Stable identifier.
- `display_name`: Human-readable name.
- `members`: Set of package identifiers.
- `version_strategy`: Shared versioning rule for the group.
- `mismatch_policy`: How existing version mismatches are surfaced before planning.

### Validation Rules

- A group must contain at least two packages unless intentionally staged for growth.
- Each member must exist in the discovered package set.
- Existing version mismatches must produce a warning or error before finalizing the plan.

## 6. ChangeSignal

**Purpose**: Captures input that a package changed and why.

### Fields

- `package_id`: Changed package.
- `requested_bump`: Explicit bump severity if supplied.
- `change_origin`: Direct code change, dependency-triggered change, or manual override.
- `evidence_refs`: References to change metadata, test fixtures, or compatibility evidence.
- `notes`: Human-readable explanation.

### Validation Rules

- Each signal must target an existing package.
- Manual overrides must outrank inferred patch propagation only when explicitly set.

## 7. CompatibilityAssessment

**Purpose**: Optional evidence that a changed package should escalate parent impact beyond the default patch rule.

### Fields

- `package_id`: Package whose compatibility impact is being assessed.
- `provider_id`: Adapter or provider that produced the assessment.
- `severity`: None, patch, minor, or major.
- `confidence`: High, medium, low, or unknown.
- `summary`: Human-readable reason for the assessment.
- `evidence_location`: Link or reference to supporting evidence.

### Validation Rules

- Assessments must be traceable to a provider.
- Unknown or failed assessments must not silently upgrade severity.
- Conflicting assessments must resolve deterministically, preferring the highest severity and retaining provenance.

## 8. ReleaseDecision

**Purpose**: Planned version outcome for one package.

### Fields

- `package_id`: Target package.
- `trigger_type`: Direct change, transitive dependency impact, version-group synchronization, or manual override.
- `recommended_bump`: Patch, minor, major, or none.
- `group_id`: Optional version group link.
- `reasons`: Ordered list of reasons contributing to the result.
- `upstream_sources`: Changed packages that caused the decision.
- `warnings`: Planning-time warnings relevant to this package.

### Validation Rules

- Each package must have at most one final release decision in a given plan.
- Group members must converge on the same final planned version.
- A package with no applicable triggers must resolve to `none`.

## 9. ReleasePlan

**Purpose**: Top-level output for one planning run.

### Fields

- `generated_at`: Plan generation timestamp.
- `workspace_root`: Repository analyzed.
- `decisions`: Final package decisions.
- `groups`: Materialized version-group outcomes.
- `warnings`: Global planning warnings.
- `unresolved_items`: Issues that blocked a clean plan but did not crash discovery.

### Validation Rules

- The plan must be reproducible from the same inputs.
- Global warnings must preserve source package or configuration references.
- A successful plan must include every discovered package, even if its decision is `none`.

## Relationships Summary

- `WorkspaceConfiguration` contains many `VersionGroup` definitions.
- `EcosystemAdapterDescriptor` discovers many `PackageRecord` entries.
- `PackageRecord` nodes connect through `DependencyEdge` relationships.
- `PackageRecord` may optionally belong to one `VersionGroup`.
- `ChangeSignal` targets a `PackageRecord`.
- `CompatibilityAssessment` enriches a changed `PackageRecord`.
- `ReleaseDecision` is produced for each `PackageRecord`.
- `ReleasePlan` contains all final `ReleaseDecision` entries.

## State Flow

1. Load `WorkspaceConfiguration`.
2. Use `EcosystemAdapterDescriptor` implementations to discover `PackageRecord` entries.
3. Build and normalize `DependencyEdge` relationships.
4. Apply `VersionGroup` membership.
5. Ingest `ChangeSignal` inputs.
6. Attach any `CompatibilityAssessment` evidence.
7. Produce `ReleaseDecision` entries.
8. Finalize one `ReleasePlan` with warnings and grouped outcomes.
