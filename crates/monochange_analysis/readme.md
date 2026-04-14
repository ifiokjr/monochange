# monochange_analysis

Semantic change analysis for generating granular changesets.

## Purpose

This crate provides the analysis pipeline for detecting user-facing changes across libraries, applications, and CLI tools. It extracts semantic meaning from git diffs to suggest appropriate changeset granularity.

## Features

- **Change frame detection**: Determine what changes to analyze based on git state (working directory, branch comparison, PR, etc.)
- **Artifact type classification**: Automatically detect if a package is a library, application, CLI tool, or mixed artifact
- **Semantic change extraction**: Parse diffs to extract meaningful changes:
  - Libraries: public API additions, modifications, removals
  - Applications: routes, components, user workflows
  - CLI tools: commands, flags, output format changes
- **Adaptive grouping**: Apply configurable thresholds to decide when to group related changes vs. create separate changesets

## Usage

```rust
use monochange_analysis::{
    analyze_changes,
    ChangeFrame,
    AnalysisConfig,
    DetectionLevel,
};
use std::path::Path;

// Detect the appropriate change frame
let frame = ChangeFrame::detect(Path::new("."))?;

// Configure analysis
let config = AnalysisConfig {
    detection_level: DetectionLevel::Signature,
    max_suggestions: 10,
    ..Default::default()
};

// Analyze changes
let analysis = analyze_changes(Path::new("."), &frame, &config)?;

// Get suggested changesets per package
for (package_id, package_analysis) in &analysis.package_changes {
    println!("Package: {}", package_id);
    for suggestion in &package_analysis.suggested_changesets {
        println!("  - {} ({})", suggestion.summary, suggestion.bump);
    }
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    CHANGE ANALYSIS PIPELINE                      │
├─────────────────────────────────────────────────────────────────┤
│  1. FRAME DETECTION                                              │
│     • Detect PR environment (CI/CD variables)                    │
│     • Detect branch vs default                                   │
│     • Fallback to working directory                              │
├─────────────────────────────────────────────────────────────────┤
│  2. ARTIFACT CLASSIFICATION                                      │
│     • Library: lib.rs with pub exports                           │
│     • Application: web framework patterns                        │
│     • CLI: clap/structopt patterns                               │
│     • Mixed: both lib and main present                           │
├─────────────────────────────────────────────────────────────────┤
│  3. SEMANTIC EXTRACTION (per artifact type)                      │
│     • Libraries: parse pub fn/struct/enum/trait                  │
│     • Applications: detect routes/components/state              │
│     • CLI: extract commands/flags/output changes                  │
├─────────────────────────────────────────────────────────────────┤
│  4. ADAPTIVE GROUPING                                            │
│     • Apply thresholds per artifact type                         │
│     • Separate breaking changes                                  │
│     • Group related internal changes                             │
└─────────────────────────────────────────────────────────────────┘
```

## Integration

This crate is designed to be used by:

- The `monochange` CLI for `mc analyze` command
- The MCP server for `monochange_analyze_changes` tool
- Direct integration in CI/CD pipelines

## Testing

```bash
# Run unit tests
cargo test -p monochange_analysis

# Run with all features
cargo test -p monochange_analysis --all-features
```
