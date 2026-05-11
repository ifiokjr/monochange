# Artifact types and release notes

Write changesets for the package's outward contract.

## Libraries

Focus on APIs, behavior, dependencies, and migration paths.

- `major`: removed/renamed APIs, changed semantics, incompatible dependency or runtime requirements.
- `minor`: new APIs or compatible capabilities.
- `patch`: bug fixes and compatible behavior corrections.

## CLI tools

Focus on commands, flags, output formats, exit codes, config files, and shell integration.

Breaking examples: renamed command, changed default output, removed flag, changed config schema.

## Applications and websites

Focus on user-visible behavior, UX, permissions, data model, and deployment impact.

Use `none` for internal-only changes that users cannot observe.

## MCP/LSP/protocol servers

Focus on tools, methods, schemas, capabilities, and protocol compatibility.

Breaking examples: removed tool, changed required input, incompatible response schema.

## Release-note quality bar

- One concise heading.
- Explain impact to the consumer.
- Include migration or usage examples for breaking or behavior-changing work.
- Avoid implementation-only summaries unless the package is an internal tool where implementation is the contract.
