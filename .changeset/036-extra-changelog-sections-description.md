---
monochange: minor
monochange_config: minor
monochange_core: minor
---

Add `description` field to `extra_changelog_sections`

This change adds an optional `description` field to the `ExtraChangelogSection` struct. The description helps LLMs and users understand when each change type should be used.

New changelog section types added to the template:

- **Testing**: Changes that only modify tests (default_bump: none)
- **Documentation**: Changes that only modify documentation (default_bump: none)
- **Security**: Security-related changes (default_bump: none)
- **Performance**: Performance improvements (default_bump: none)
- **Refactor**: Code refactoring without functional changes (default_bump: none)
