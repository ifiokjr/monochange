# Publishing example

## Recommend this when

- the user is choosing between local-only, builtin, and external publishing
- trusted publishing or placeholder publication needs to be planned
- the workspace contains public packages

## Default recommendation

- GitHub + npm: builtin is the preferred default
- `crates.io` and `pub.dev`: external is often clearer when the registry-maintained workflow should own the publish step
- GitLab: builtin planning still fits well, but publishing is external more often
- ask about placeholder publication only when public names matter and the first real release may be delayed

## Good default output

- public vs internal package split
- recommended publish mode per ecosystem
- trusted-publishing follow-up steps
- placeholder strategy, if needed
