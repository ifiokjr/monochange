All edits are complete. Here's a summary of what was done:

### File 1: `migration-workflows.md`

- **Line 42**: `- [ ] Set up setup-mc...` → `- [ ] Add monochange to devenv packages (extra.monochange)`
- **5 workflow templates**: Replaced `uses: ./.github/actions/setup-mc` with `cachix/install-nix-action@v31` + `nix profile add` devenv setup
- **4 mc command steps**: Added `shell: devenv shell -- bash -e {0}` after each `mc` run step
- **Lines 268–320**: Replaced entire `### setup-mc/action.yml` section with `### Add monochange to devenv` showing `devenv.yaml` and `devenv.nix` config

### File 2: `adoption.md`

- **3 workflow templates**: Replaced `uses: monochange/setup-mc@v1` with nix/devenv setup
- **4 mc command steps**: Added `shell: devenv shell -- bash -e {0}` after each `mc` run step
- **Lines 260–312**: Replaced `### 4. .github/actions/setup-mc/action.yml` section with `### Add monochange to devenv`
- **Line 424**: Replaced `setup-mc action` → `monochange in devenv packages`

Zero `setup-mc` references remain in either file.
