# Feature Specification: Cross-Ecosystem Release Planning Foundation

**Feature Branch**: `001-first-step-port`\
**Created**: 2026-03-25\
**Status**: Draft\
**Input**: User description: "Port the majority of core versioning and release-planning functionality from knope, add reusable ecosystem-specific support, support mixed-language workspaces and single-package repos, include version groups, transitive dependency propagation, semver-aware parent bumping, glob-based workspace discovery, `monochange.toml` defaults, and repository documentation in `docs/`, while deferring GitHub bot automation."

## User Scenarios & Testing _(mandatory)_

### User Story 1 - Discover a mixed-ecosystem workspace (Priority: P1)

As a monorepo maintainer, I want monochange to discover packages, dependency links, workspace membership, and version groups across all supported ecosystems so that I can manage releases for the whole repository without manually listing every package.

**Why this priority**: If the workspace graph is incomplete or inconsistent, every downstream versioning and release action becomes unreliable.

**Independent Test**: Can be fully tested by pointing monochange at a repository containing supported workspace styles, single-package repositories, glob-based package patterns, and grouped versions, then verifying that all intended packages and relationships are discovered without manual per-package enumeration.

**Acceptance Scenarios**:

1. **Given** a repository with Cargo, npm-family, Deno, Dart, and Flutter packages, **When** a maintainer loads the workspace, **Then** monochange identifies all supported packages, their native manifests, and their dependency relationships in a single unified graph.
2. **Given** a workspace definition that uses glob-based package discovery, **When** monochange reads the repository, **Then** it resolves all matching packages without requiring each package path to be manually declared in `monochange.toml`.
3. **Given** packages assigned to a shared version group, **When** the workspace is analyzed, **Then** monochange records the grouped relationship and treats those packages as sharing one coordinated version.

---

### User Story 2 - Plan coordinated version changes across transitive dependencies (Priority: P2)

As a release manager, I want monochange to calculate how changes propagate through direct and transitive dependencies so that parent packages receive the correct version increments automatically.

**Why this priority**: The core value of the product is correct release planning across interconnected packages. Discovery alone is not enough unless change propagation is trustworthy.

**Independent Test**: Can be fully tested by supplying change inputs for leaf packages in a dependency graph and verifying that monochange produces the expected parent increments, including grouped packages and semver-sensitive escalation rules.

**Acceptance Scenarios**:

1. **Given** a package that depends on a changed sub-dependency through one or more intermediate packages, **When** monochange calculates release impact, **Then** every affected parent package receives an increment according to the propagation rules.
2. **Given** a dependency change with no semver-breaking signal, **When** monochange propagates that change upward, **Then** each affected parent package defaults to a patch increment unless a stronger rule applies.
3. **Given** a dependency change that is identified as semver-breaking, **When** monochange propagates that change to dependent packages, **Then** parent increments are raised to reflect the higher compatibility impact instead of remaining at the default patch level.
4. **Given** a package inside a version group requires a release, **When** monochange produces the release plan, **Then** all packages in the same version group receive the same planned version.

---

### User Story 3 - Use one tool consistently across ecosystems (Priority: P3)

As a platform team, I want ecosystem support to behave consistently across supported package managers and languages so that teams can reuse monochange in different repositories without losing capabilities or learning different rule sets for each ecosystem.

**Why this priority**: The product differentiates itself by equal support across ecosystems rather than treating some package types as first-class and others as partial add-ons.

**Independent Test**: Can be fully tested by configuring representative repositories for each supported ecosystem and confirming that discovery, grouping, dependency propagation, and configuration defaults behave consistently across them.

**Acceptance Scenarios**:

1. **Given** repositories that use Cargo, npm, pnpm, Bun, Deno, Dart, and Flutter packaging models, **When** monochange is configured with equivalent intent, **Then** it supports the same core behaviors for discovery, dependency mapping, version grouping, and release planning across each ecosystem.
2. **Given** a maintainer onboarding to monochange for the first time, **When** they follow the repository documentation, **Then** they can understand configuration, supported ecosystems, grouping rules, and propagation behavior without reading source code.

### Edge Cases

- What happens when a workspace glob matches no packages or matches paths that do not contain a supported manifest?
- What happens when a package is discovered through multiple workspace definitions at once?
- How does the system handle cyclic dependency graphs when calculating transitive release impact?
- How does the system handle a version group whose members start with mismatched versions?
- What happens when semver impact cannot be determined for a changed dependency?
- How does the system behave for private packages, unpublished packages, or packages intentionally excluded from release output?
- What happens when a repository contains both workspace-managed packages and standalone single-package projects?

## Requirements _(mandatory)_

### Functional Requirements

- **FR-001**: The system MUST read repository-level configuration from `monochange.toml`.
- **FR-002**: The system MUST support sensible default behavior so that repositories can use monochange with minimal manual configuration.
- **FR-003**: The system MUST discover packages from native workspace definitions and standalone manifests across supported ecosystems.
- **FR-004**: The system MUST support Cargo workspaces and standalone Cargo packages.
- **FR-005**: The system MUST support npm workspaces, pnpm workspaces, Bun workspaces, and standalone npm-style packages.
- **FR-006**: The system MUST support Deno workspaces and standalone Deno packages.
- **FR-007**: The system MUST support Dart workspaces, standalone Dart packages, Flutter workspaces, and standalone Flutter packages.
- **FR-008**: The system MUST resolve glob-based workspace package patterns anywhere a supported ecosystem uses them for package discovery.
- **FR-009**: The system MUST build a unified dependency graph that includes direct and transitive relationships across all discovered packages.
- **FR-010**: The system MUST allow maintainers to define version groups whose members always share the same planned version.
- **FR-011**: The system MUST propagate release impact from a changed package to every dependent package in the transitive dependency graph.
- **FR-012**: The system MUST apply a patch increment by default when a dependency change affects a parent package and no stronger compatibility signal is available.
- **FR-013**: The system MUST support semver-aware dependency impact so that parent packages can receive stronger increments when a dependency change is compatibility-breaking.
- **FR-014**: The system MUST produce consistent planning behavior across all supported ecosystems for discovery, dependency mapping, version grouping, and propagation rules.
- **FR-015**: The system MUST make ecosystem support reusable outside the default all-in-one distribution so projects can consume only the ecosystem support they need.
- **FR-016**: The system MUST provide a default distribution that combines all supported ecosystem integrations into one user-facing tool.
- **FR-017**: The system MUST provide repository documentation under `docs/` that explains configuration, workspace discovery, version groups, dependency propagation, and supported ecosystem behavior.
- **FR-018**: The first delivery of this feature MUST exclude GitHub bot automation while preserving a path to add it later.
- **FR-019**: The system MUST preserve composable configuration concepts comparable to the current knope configuration model where those concepts remain compatible with the requested behavior.
- **FR-020**: The system MUST allow repositories to combine workspace-managed packages and standalone packages in the same release-planning scope.

### Key Entities _(include if feature involves data)_

- **Workspace Configuration**: Repository-level settings that define defaults, scope, grouping rules, and behavior overrides for release planning.
- **Package Record**: A discovered package with ecosystem identity, manifest location, version, privacy/publishability state, and dependency metadata.
- **Ecosystem Support Module**: A reusable unit of support for one ecosystem that knows how to discover packages, interpret native workspace rules, and expose dependency information consistently.
- **Dependency Edge**: A relationship showing that one package depends on another, including whether the relationship is direct or discovered through transitive traversal.
- **Version Group**: A named coordination rule that forces multiple packages to share one planned version.
- **Change Signal**: An input describing that a package changed and the strength of that change, including whether semver analysis raised the impact level.
- **Release Plan**: The computed output describing which packages should release and which version increments they should receive.

## Assumptions

- The first step focuses on release planning, dependency impact calculation, configuration, and documentation rather than bot-driven automation.
- Repositories may contain packages that are private, unpublished, or not intended for release, and monochange will need a way to represent them in planning output.
- When semver-specific evidence is unavailable for a changed dependency, the safe default is to treat parent propagation as a patch increment.
- Equivalent support means each supported ecosystem receives the same core product capabilities, even if the native manifest syntax differs.

## Out of Scope

- GitHub bot workflow automation, pull request commenting, and merge-driven release bots.
- Publishing credentials, release hosting, or remote service integrations beyond what is required to define and calculate the release plan.

## Success Criteria _(mandatory)_

### Measurable Outcomes

- **SC-001**: In representative fixtures for the supported ecosystems, monochange discovers 100% of intended packages defined through native workspace rules, standalone manifests, and glob-based package patterns without requiring manual per-package enumeration.
- **SC-002**: In representative dependency-graph fixtures, monochange calculates the expected release impact for direct and transitive dependents in 100% of documented test cases.
- **SC-003**: In representative version-group fixtures, 100% of grouped packages receive identical planned versions whenever any member of the group requires release.
- **SC-004**: For dependency changes without stronger compatibility evidence, 100% of affected parent packages default to patch increments in the documented release plan.
- **SC-005**: For dependency changes marked as compatibility-breaking, parent package increments are raised according to the documented semver-aware rules in 100% of documented test cases.
- **SC-006**: A new maintainer can configure a representative repository and generate an initial release-planning result by following the repository documentation in under 15 minutes without reading source code.
