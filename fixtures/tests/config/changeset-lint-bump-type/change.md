---
core: breaking
---

## Remove legacy parser

## Impact

Consumers must move to the new parser entry points before upgrading because the old aliases are no longer exported.

## Migration

Replace direct legacy parser imports with the supported parser module.

```rust
use cargo_core::parser::Parser;
```

## Breaking

The legacy parser has been removed.
