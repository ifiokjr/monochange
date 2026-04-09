---
main: test
---

#### add devenv-managed git hooks for local validation

Repository contributors now get `devenv`-managed git hooks that apply autofixes before commits and run the full local validation suite before pushes.

**Before:** contributors had to remember to run formatting, lint autofixes, tests, builds, docs checks, and coverage manually.

```bash
devenv shell
fix:all
lint:all
test:all
build:all
build:book
coverage:all
```

**After:** entering the repo shell installs repo hooks automatically.

```bash
devenv shell
# pre-commit: formats staged files, applies targeted autofixes, and restages the results
# pre-push: runs lint, tests, builds, docs checks, and coverage before the push is sent
```

The new pre-commit hook prefers staged-file formatting and targeted clippy autofixes, but falls back to full-repo formatting or workspace clippy fixes when a targeted fix cannot complete cleanly. The pre-push hook mirrors the current CI-style validation flow so broken changes are caught locally before they reach GitHub.
