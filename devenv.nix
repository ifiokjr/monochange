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
      custom.pnpm
      deno
      dprint
      gitleaks
      hyperfine
      mdbook
      nixfmt
      rustup
      shfmt
    ]
    ++ lib.optionals stdenv.isDarwin [
      coreutils
    ];

  enterShell = ''
    set -euo pipefail

    # Keep shell entry fast for local iteration. Only bootstrap missing toolchains
    # here; explicit updates happen via install/update tasks instead of every shell.
    if ! rustup toolchain list | grep -Eq '^nightly'; then
      rustup toolchain install nightly --component rustfmt --no-self-update 2>/dev/null || true
    fi
    if ! rustup toolchain list | grep -Eq '^stable'; then
      rustup toolchain install stable --no-self-update 2>/dev/null || true
    fi

    export PATH="$DEVENV_ROOT/scripts:$PATH"
    eval "$(pnpm-activate-env)"
  '';

  # disable dotenv since it breaks the variable interpolation supported by `direnv`
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
      "secrets:push" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "secrets";
        description = "Scan repository history for leaked secrets with gitleaks before push.";
        entry = "${pkgs.gitleaks}/bin/gitleaks detect --verbose --redact";
        stages = [ "pre-push" ];
      };
      "lint:test" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "lint and test";
        description = "Run the local CI lint rules and test suite before push.";
        entry = ''
          set -euo pipefail
          ${config.env.DEVENV_PROFILE}/bin/lint:all;
          ${config.env.DEVENV_PROFILE}/bin/test:all
        '';
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
    "install:toolchains" = {
      exec = ''
        set -euo pipefail
        rustup toolchain install nightly --component rustfmt --no-self-update
        rustup toolchain install stable --no-self-update
      '';
      description = "Install the Rust toolchains used by monochange.";
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
        update:toolchains
        cargo update
        devenv update
      '';
      description = "Update dependencies.";
      binary = "bash";
    };
    "update:toolchains" = {
      exec = ''
        set -euo pipefail
        rustup toolchain install nightly --component rustfmt --no-self-update
        rustup update stable --no-self-update
      '';
      description = "Refresh the Rust toolchains used by monochange.";
      binary = "bash";
    };
    "build:all" = {
      exec = ''
        set -euo pipefail
        if [ -z "$CI" ]; then
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

        lockfile_backup="$(mktemp)"
        cp Cargo.lock "$lockfile_backup"
        restore_lockfile() {
          cp "$lockfile_backup" Cargo.lock
          rm -f "$lockfile_backup"
        }
        trap restore_lockfile EXIT

        cargo workspaces publish --from-git --allow-dirty --yes --dry-run
        cp "$lockfile_backup" Cargo.lock

        cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu >/dev/null

        if ! cmp -s Cargo.lock "$lockfile_backup"; then
          echo "Cargo.lock is missing Linux-specific resolution. Run:" >&2
          echo "  cargo metadata --format-version 1 --filter-platform x86_64-unknown-linux-gnu >/dev/null" >&2
          echo "and commit the resulting Cargo.lock changes." >&2
          exit 1
        fi
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
        cargo bin cargo-nextest run --workspace --all-features --no-tests pass
      '';
      description = "Run cargo tests with nextest.";
      binary = "bash";
    };
    "test:cargo:expensive" = {
      exec = ''
        set -euo pipefail
        MONOCHANGE_EXPENSIVE_TESTS=1 cargo bin cargo-nextest run --workspace --all-features --no-tests pass
      '';
      description = "Run cargo tests with the CI-only large-fixture cases enabled.";
      binary = "bash";
    };
    "test:docs" = {
      exec = ''
        set -euo pipefail
        cargo test --doc --workspace --all-features
      '';
      description = "Run documentation tests.";
      binary = "bash";
    };
    "test:node" = {
      exec = ''
        set -euo pipefail
        node --test npm/tests/*.test.mjs scripts/npm/tests/*.test.mjs
      '';
      description = "Run npm helper, launcher, and repository utility tests with the built-in Node test runner.";
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
        cargo llvm-cov test --workspace --all-features --all-targets --no-report
        cargo llvm-cov report --summary-only --fail-under-lines 70
        cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
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

        node scripts/check-patch-coverage.mjs \
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
        fix:monochange
        fix:format
      '';
      description = "Fix all autofixable problems, including shared-doc synchronization via `mdt update`.";
      binary = "bash";
    };
    "fix:format" = {
      exec = ''
        set -euo pipefail
        dprint fmt --config "$DEVENV_ROOT/dprint.json"
      '';
      description = "Format files with dprint.";
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
    "deny:check" = {
      exec = ''
        set -euo pipefail
        cargo deny check
      '';
      description = "Run cargo-deny checks for security advisories and license compliance.";
      binary = "bash";
    };
    "lint:all" = {
      exec = ''
        set -euo pipefail
        lint:clippy
        lint:format
        lint:architecture
        lint:root-git-config
        deny:check
        docs:check
        lint:monochange
        publish:check
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
        node scripts/check-architecture-boundaries.mjs
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
    "docs:check" = {
      exec = ''
        set -euo pipefail
        mdt check
        node scripts/check-agent-surface.mjs
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
    "snapshot:update" = {
      exec = ''
        set -euo pipefail
        cargo bin cargo-nextest run --workspace --all-features --no-tests pass
        cargo insta accept
      '';
      description = "Update insta snapshots.";
      binary = "bash";
    };
    "setup:helix" = {
      exec = ''
        set -euo pipefail
        rm -rf .helix
        cp -r setup/editors/helix .helix
      '';
      description = "Setup Helix editor configuration.";
      binary = "bash";
    };
    "setup:vscode" = {
      exec = ''
        set -euo pipefail
        rm -rf .vscode
        cp -r ./setup/editors/vscode .vscode
      '';
      description = "Setup VS Code workspace configuration.";
      binary = "bash";
    };
  };
}
