---
monochange_analysis: minor
---

#### add semantic change analysis crate

Introduces a new `monochange_analysis` crate that provides intelligent, artifact-aware changeset generation for the monochange ecosystem.

**What it does:**

The crate analyzes git diffs and suggests granular changesets based on the type of code being changed:

- **Libraries**: Detects public API changes (new functions, types, traits)
- **Applications**: Identifies UI components, routes, and state changes
- **CLI tools**: Extracts command and flag modifications

**Key features:**

- **Change frame detection**: Automatically detects what to analyze based on git state (working directory, branches, PRs, CI/CD environments)
- **Artifact type classification**: Determines if a package is a library, application, CLI tool, or mixed artifact
- **Semantic extraction**: Three levels of analysis - basic (file-level), signature (function/type signatures), and semantic (full AST)
- **Adaptive grouping**: Configurable thresholds for grouping related changes vs. creating separate changesets

**Example usage:**

```rust
use monochange_analysis::{
    analyze_changes,
    ChangeFrame,
    AnalysisConfig,
    DetectionLevel,
};

// Auto-detect the change frame
let frame = ChangeFrame::detect(Path::new("."))?;

let config = AnalysisConfig {
    detection_level: DetectionLevel::Signature,
    ..Default::default()
};

let analysis = analyze_changes(Path::new("."), &frame, &config)?;

// Get suggested changesets per package
for (package_id, pkg) in &analysis.package_changes {
    for cs in &pkg.suggested_changesets {
        println!("{}: {}", package_id, cs.summary);
    }
}
```

**Supported CI/CD environments:**

- GitHub Actions
- GitLab CI
- CircleCI
- Travis CI
- Azure Pipelines
- Buildkite

This crate is the foundation for the new `mc analyze` command and MCP tools that help agents generate better changesets automatically.
