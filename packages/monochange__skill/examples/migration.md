# Migration example

## Recommend this when

- the repository already has release scripts, CI workflows, or tag conventions
- monochange must coexist with current tooling first
- the user wants a phased migration instead of a big-bang switch

## Default recommendation

- choose `migration` depth
- inspect existing CI files, tag conventions, changelog flow, and competitor tooling first
- keep publishing external more often during the first phase
- move config validation, discovery, and release dry-runs into CI before replacing publish jobs

## Good default output

- current workflow summary
- recommended coexistence phase
- pieces safe to migrate now
- pieces to delay until trust is established
