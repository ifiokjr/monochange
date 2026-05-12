{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  custom = inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system};
in
{
  packages =
    with pkgs;
    [
      cargo-binstall
      cargo-run-bin
      cacert
      custom.mdt
      dprint
      gh
      git
      gitleaks
      hyperfine
      jq
      mdbook
      nixfmt
      pnpm_10
      nodejs_24
      python3
      rustup
      shfmt
      taplo
      unzip
      zip
    ]
    ++ lib.optionals stdenv.isDarwin [
      coreutils
    ];

  enterShell = ''
    set -euo pipefail
    export PATH="$DEVENV_PROFILE/bin:$PATH"
  '';

  # disable dotenv since it interferes with variable interpolation in the shell
  dotenv.disableHint = true;

  git-hooks = {
    hooks = {
      "secrets:commit" = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "secrets";
        description = "Scan staged changes for leaked secrets with gitleaks.";
        entry = "${pkgs.gitleaks}/bin/gitleaks protect --staged --verbose --redact";
        stages = [ "pre-commit" ];
      };
      dprint = {
        enable = true;
        verbose = true;
        pass_filenames = true;
        name = "dprint check";
        description = "Run workspace autofixes before commit and restage the results.";
        entry = "${pkgs.dprint}/bin/dprint check --allow-no-files";
        stages = [ "pre-commit" ];
      };
      "lint:test" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "lint and test";
        description = "Run the local CI lint rules and test suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/lint:test";
        stages = [ "pre-push" ];
      };
    };
  };

  scripts = {
    "monochange" = {
      exec = ''
        set -euo pipefail
        cargo run --quiet --package monochange --bin monochange -- "$@"
      '';
      description = "The dev build of the `monochange` executable";
      binary = "bash";
    };
    "mc" = {
      exec = ''
        set -euo pipefail
        cargo run --quiet --release --package monochange --bin mc -- "$@"
      '';
      description = "The release build of the `monochange` executable";
      binary = "bash";
    };
    "install:all" = {
      exec = ''
        set -euo pipefail
        install:cargo:bin
      '';
      description = "Install all packages.";
      binary = "bash";
    };
    "install:cargo:bin" = {
      exec = ''
        set -euo pipefail
        cargo bin --install
      '';
      description = "Install cargo binaries locally.";
      binary = "bash";
    };
    "update:deps" = {
      exec = ''
        set -euo pipefail
        cargo update
        devenv update
      '';
      description = "Update dependencies.";
      binary = "bash";
    };
    "build:all" = {
      exec = ''
        set -euo pipefail
        if [ -z "''${CI:-}" ]; then
          echo "Building project locally"
          cargo build --workspace --all-features
        else
          echo "Building in CI"
          cargo build --workspace --all-features --locked
        fi
      '';
      description = "Build all crates with all features activated.";
      binary = "bash";
    };
    "build:book" = {
      exec = ''
        set -euo pipefail
        mdbook build docs
      '';
      description = "Build the mdbook documentation.";
      binary = "bash";
    };
    "publish:check" = {
      exec = ''
        set -euo pipefail
        mc publish-check || true
      '';
      description = "Check that publication is valid for this project";
      binary = "bash";
    };
    "test:all" = {
      exec = ''
        set -euo pipefail
        test:cargo
        test:docs
        test:node
      '';
      description = "Run all tests across the crates and npm helper scripts.";
      binary = "bash";
    };
    "test:cargo" = {
      exec = ''
        set -euo pipefail
        cargo insta test --workspace --exclude xtask --all-features --test-runner nextest --unreferenced=reject
      '';
      description = "Run cargo tests with nextest and reject unreferenced snapshots.";
      binary = "bash";
    };
    "test:cargo:expensive" = {
      exec = ''
        set -euo pipefail
        MONOCHANGE_EXPENSIVE_TESTS=1 cargo insta test --workspace --exclude xtask --all-features --test-runner nextest --unreferenced=reject
      '';
      description = "Run cargo tests with CI-only large-fixture cases enabled and reject unreferenced snapshots.";
      binary = "bash";
    };
    "test:docs" = {
      exec = ''
        set -euo pipefail
        cargo test --doc --workspace --exclude xtask --all-features
      '';
      description = "Run documentation tests.";
      binary = "bash";
    };
    "test:node" = {
      exec = ''
        set -euo pipefail
        pnpm vitest run --exclude 'worktrees/**' scripts/npm/tests/*.test.mjs
      '';
      description = "Run npm helper, launcher, and repository utility tests with Vitest.";
      binary = "bash";
    };
    "test:agent-evals" = {
      exec = ''
        set -euo pipefail
        cargo test --package monochange --all-features agent_eval_
      '';
      description = "Run the focused agent-style eval coverage for machine-readable workflows.";
      binary = "bash";
    };
    "coverage:all" = {
      exec = ''
        set -euo pipefail
        mkdir -p target/coverage
        cargo llvm-cov clean --workspace
        cargo llvm-cov test --workspace --exclude xtask --all-features --all-targets --no-report
        cargo llvm-cov report --ignore-filename-regex 'crates/xtask/' --summary-only --fail-under-lines 70
        cargo llvm-cov report --ignore-filename-regex 'crates/xtask/' --lcov --output-path target/coverage/lcov.info
      '';
      description = "Run workspace coverage, enforce a 70% line-coverage floor, and write target/coverage/lcov.info.";
      binary = "bash";
    };
    "coverage:patch" = {
      exec = ''
        set -euo pipefail
        base_ref="''${MONOCHANGE_PATCH_COVERAGE_BASE:-origin/main}"
        head_ref="''${MONOCHANGE_PATCH_COVERAGE_HEAD:-HEAD}"

        if [ ! -f target/coverage/lcov.info ]; then
          coverage:all
        fi

        pnpm node scripts/check-patch-coverage.mjs \
          --repo-root "$DEVENV_ROOT" \
          --lcov target/coverage/lcov.info \
          --base "$base_ref" \
          --head "$head_ref" \
          --target 100
      '';
      description = "Fail when executable changed lines fall below 100% patch coverage.";
      binary = "bash";
    };
    "fix:all" = {
      exec = ''
        set -euo pipefail
        fix:clippy
        docs:update
        schema:update
        fix:monochange
        fix:format
        fix:js
        fix:workflows
      '';
      description = "Fix all autofixable problems, including shared-doc synchronization via `mdt update`.";
      binary = "bash";
    };
    "fix:format" = {
      exec = ''
        set -euo pipefail
        repo_root="$(git rev-parse --show-toplevel)"
        dprint fmt --config "$repo_root/dprint.json"
      '';
      description = "Format files with dprint.";
      binary = "bash";
    };
    "schema:update" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema update
      '';
      description = "Regenerate committed JSON Schema assets.";
      binary = "bash";
    };
    "schema:check" = {
      exec = ''
        set -euo pipefail
        cargo xtask schema check
      '';
      description = "Check committed JSON Schema assets are up to date.";
      binary = "bash";
    };
    "fix:clippy" = {
      exec = ''
        set -euo pipefail
        cargo clippy --workspace --fix --allow-dirty --allow-staged --all-features --all-targets
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "fix:monochange" = {
      exec = ''
        set -euo pipefail
        mc validate
        mc check --fix
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "lint:workflows" = {
      exec = ''
        set -euo pipefail
        if ! command -v zizmor >/dev/null 2>&1; then
          echo "Installing zizmor via cargo-binstall..."
          cargo binstall zizmor --no-confirm
        fi
        zizmor .github/workflows/ .github/actions/
      '';
      description = "Scan GitHub Actions workflows for security vulnerabilities with zizmor.";
      binary = "bash";
    };
    "deny:check" = {
      exec = ''
        set -euo pipefail
        cargo deny check
      '';
      description = "Run cargo-deny checks for security advisories and license compliance.";
      binary = "bash";
    };
    "lint:test" = {
      exec = ''
        set -euo pipefail
        gitleaks detect --verbose --redact

        # lint:all;
        # test:all;
      '';
      description = "Used for the pre push checks";
      binary = "bash";
    };
    "lint:all" = {
      exec = ''
        set -euo pipefail
        lint:clippy
        schema:check
        lint:format
        lint:architecture
        lint:root-git-config
        lint:js
        lint:workflows
        lint:js:types
        deny:check
        docs:check
        lint:monochange
      '';
      description = "Run all checks.";
      binary = "bash";
    };
    "lint:format" = {
      exec = ''
        set -euo pipefail
        dprint check
      '';
      description = "Check that all files are formatted.";
      binary = "bash";
    };
    "lint:monochange" = {
      exec = ''
        set -euo pipefail
        mc validate
        mc check
      '';
      description = "Run manifest lint rules across all ecosystems.";
      binary = "bash";
    };
    "lint:clippy" = {
      exec = ''
        set -euo pipefail
        # Treat all compiler and clippy warnings as errors so warning-only
        # regressions never make it into CI or a pushed branch.
        cargo clippy --workspace --all-features --all-targets -- -D warnings
      '';
      description = "Check that all rust lints are passing with warnings denied.";
      binary = "bash";
    };
    "lint:architecture" = {
      exec = ''
        set -euo pipefail
        pnpm node scripts/check-architecture-boundaries.mjs
      '';
      description = "Check that provider and ecosystem dispatch stays inside the documented allowlist.";
      binary = "bash";
    };
    "lint:root-git-config" = {
      exec = ''
        set -euo pipefail
        common_git_dir="$(git rev-parse --git-common-dir)"
        config_path="$common_git_dir/config"

        if git config --file "$config_path" --get core.worktree >/dev/null 2>&1; then
          echo "error: root git config must not set core.worktree in $config_path" >&2
          git config --file "$config_path" --get core.worktree >&2
          exit 1
        fi

        if git config --file "$config_path" --get-regexp '^user\.' >/dev/null 2>&1; then
          echo "error: root git config must not contain a [user] block in $config_path" >&2
          git config --file "$config_path" --get-regexp '^user\.' >&2
          exit 1
        fi
      '';
      description = "Check that the shared root .git/config does not contain worktree or user overrides.";
      binary = "bash";
    };
    "lint:js" = {
      exec = ''
        set -euo pipefail
        pnpm oxlint --type-aware .
      '';
      description = "Lint all JS/TS files with oxlint (type-aware).";
      binary = "bash";
    };
    "lint:js:syntax" = {
      exec = ''
        set -euo pipefail
        pnpm oxlint .
      '';
      description = "Lint all JS/TS files with oxlint (syntax-only, faster).";
      binary = "bash";
    };
    "lint:js:types" = {
      exec = ''
        set -euo pipefail
        pnpm tsgo -config tsconfig.json
      '';
      description = "Type-check all JS/TS files with tsgo.";
      binary = "bash";
    };
    "fix:workflows" = {
      exec = ''
        set -euo pipefail
        if ! command -v zizmor >/dev/null 2>&1; then
          echo "Installing zizmor via cargo-binstall..."
          cargo binstall zizmor --no-confirm
        fi
        zizmor --fix .github/workflows/ .github/actions/
      '';
      description = "Auto-fix zizmor findings in GitHub Actions workflows where possible.";
      binary = "bash";
    };
    "fix:js" = {
      exec = ''
        set -euo pipefail
        pnpm oxfmt --write '**/*.{js,mjs,ts,mts}'
        pnpm oxlint --type-aware --fix .
      '';
      description = "Format all JS/TS files with oxfmt.";
      binary = "bash";
    };
    "build:js" = {
      exec = ''
        set -euo pipefail
        pnpm tsdown
      '';
      description = "Bundle JS/TS entry points with tsdown.";
      binary = "bash";
    };
    "docs:check" = {
      exec = ''
        set -euo pipefail
        mdt check
        cargo xtask skill commands check
        pnpm node scripts/check-agent-surface.mjs
      '';
      description = "Check that shared documentation blocks are synchronized and agent-facing docs stay aligned with the repo surface.";
      binary = "bash";
    };
    "docs:update" = {
      exec = ''
        set -euo pipefail
        mdt update
      '';
      description = "Update shared documentation blocks across markdown and source files.";
      binary = "bash";
    };
    "snapshot:review" = {
      exec = ''
        set -euo pipefail
        cargo insta review
      '';
      description = "Review insta snapshots.";
      binary = "bash";
    };
    "snapshot:check" = {
      exec = ''
        set -euo pipefail
        cargo insta test --workspace --exclude xtask --all-features --test-runner nextest --unreferenced=reject
      '';
      description = "Check insta snapshots and fail on unreferenced snapshot files.";
      binary = "bash";
    };
    "snapshot:update" = {
      exec = ''
        set -euo pipefail
        cargo insta test --workspace --exclude xtask --all-features --test-runner nextest --force-update-snapshots --unreferenced=delete
      '';
      description = "Update insta snapshots and delete unreferenced snapshot files.";
      binary = "bash";
    };
  };
}
