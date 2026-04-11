{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:

let
  extra = inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system};
in
{
  packages =
    with pkgs;
    [
      cargo-binstall
      cargo-run-bin
      deno
      dprint
      extra.mdt
      extra.pnpm-standalone
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
    set -e
    rustup toolchain install nightly --component rustfmt --no-self-update 2>/dev/null || true
    rustup update stable --no-self-update 2>/dev/null || true

    export PATH="$DEVENV_ROOT/scripts:$PATH"
    eval "$(pnpm-activate-env)"
  '';

  # disable dotenv since it breaks the variable interpolation supported by `direnv`
  dotenv.disableHint = true;

  git-hooks = {
    # package = pkgs.prek;

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
      "lint" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "lint";
        description = "Run the local CI lint rules suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/lint:all";
        stages = [ "pre-push" ];
      };
      "test" = {
        enable = true;
        verbose = true;
        pass_filenames = false;
        name = "test";
        description = "Run the local CI validation suite before push.";
        entry = "${config.env.DEVENV_PROFILE}/bin/test:all";
        stages = [ "pre-push" ];
      };
    };
  };

  scripts = {
    "monochange" = {
      exec = ''
        set -e
        cargo run --quiet --package monochange --bin monochange -- "$@"
      '';
      description = "The dev build of the `monochange` executable";
      binary = "bash";
    };
    "mc" = {
      exec = ''
        set -e
        cargo bin mc "$@"
      '';
      description = "The release build of the `monochange` executable";
      binary = "bash";
    };
    "install:all" = {
      exec = ''
        set -e
        install:cargo:bin
      '';
      description = "Install all packages.";
      binary = "bash";
    };
    "install:cargo:bin" = {
      exec = ''
        set -e
        cargo bin --install
      '';
      description = "Install cargo binaries locally.";
      binary = "bash";
    };
    "update:mc" = {
      exec = ''
        set -e
        rm -rf $DEVENV_ROOT/.bin/rust-*/monochange  $DEVENV_ROOT/.bin/.shims/mc
        mc --help
      '';
      description = "Alias for the `monochange` executable";
      binary = "bash";
    };
    "update:deps" = {
      exec = ''
        set -e
        update:mc
        cargo update
        devenv update
      '';
      description = "Update dependencies.";
      binary = "bash";
    };
    "build:all" = {
      exec = ''
        set -e
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
        set -e
        mdbook build docs
      '';
      description = "Build the mdbook documentation.";
      binary = "bash";
    };
    "test:all" = {
      exec = ''
        set -e
        test:cargo
        test:docs
        test:node
      '';
      description = "Run all tests across the crates and npm helper scripts.";
      binary = "bash";
    };
    "test:cargo" = {
      exec = ''
        set -e
        cargo bin cargo-nextest run --workspace --all-features --no-tests pass
      '';
      description = "Run cargo tests with nextest.";
      binary = "bash";
    };
    "test:docs" = {
      exec = ''
        set -e
        cargo test --doc --workspace --all-features
      '';
      description = "Run documentation tests.";
      binary = "bash";
    };
    "test:node" = {
      exec = ''
        set -e
        node --test npm/tests/*.test.mjs
      '';
      description = "Run npm helper and launcher tests with the built-in Node test runner.";
      binary = "bash";
    };
    "coverage:all" = {
      exec = ''
        set -euo pipefail
        mkdir -p target/coverage
        cargo bin cargo-llvm-cov clean --workspace
        cargo bin cargo-llvm-cov test --workspace --all-features --all-targets --no-report
        cargo bin cargo-llvm-cov report --summary-only --fail-under-lines 70
        cargo bin cargo-llvm-cov report --lcov --output-path target/coverage/lcov.info
      '';
      description = "Run workspace coverage, enforce a 70% line-coverage floor, and write target/coverage/lcov.info.";
      binary = "bash";
    };
    "fix:all" = {
      exec = ''
        set -e
        fix:clippy
        docs:update # runs `mdt update`
        fix:format
        mc validate
      '';
      description = "Fix all autofixable problems, including shared-doc synchronization via `mdt update`.";
      binary = "bash";
    };
    "fix:format" = {
      exec = ''
        set -e
        dprint fmt --config "$DEVENV_ROOT/dprint.json"
      '';
      description = "Format files with dprint.";
      binary = "bash";
    };
    "fix:clippy" = {
      exec = ''
        set -e
        cargo clippy --workspace --fix --allow-dirty --allow-staged --all-features --all-targets
      '';
      description = "Fix clippy lints for rust.";
      binary = "bash";
    };
    "deny:check" = {
      exec = ''
        set -e
        cargo deny check
      '';
      description = "Run cargo-deny checks for security advisories and license compliance.";
      binary = "bash";
    };
    "lint:all" = {
      exec = ''
        set -e
        lint:clippy
        lint:format
        deny:check
        docs:check
        mc validate
      '';
      description = "Run all checks.";
      binary = "bash";
    };
    "lint:format" = {
      exec = ''
        set -e
        dprint check
      '';
      description = "Check that all files are formatted.";
      binary = "bash";
    };
    "lint:clippy" = {
      exec = ''
        set -e
        # Treat all compiler and clippy warnings as errors so warning-only
        # regressions never make it into CI or a pushed branch.
        cargo clippy --workspace --all-features --all-targets -- -D warnings
      '';
      description = "Check that all rust lints are passing with warnings denied.";
      binary = "bash";
    };
    "docs:check" = {
      exec = ''
        set -e
        mdt check
      '';
      description = "Check that shared documentation blocks are synchronized.";
      binary = "bash";
    };
    "docs:update" = {
      exec = ''
        set -e
        mdt update
      '';
      description = "Update shared documentation blocks across markdown and source files.";
      binary = "bash";
    };
    "snapshot:review" = {
      exec = ''
        set -e
        cargo insta review
      '';
      description = "Review insta snapshots.";
      binary = "bash";
    };
    "snapshot:update" = {
      exec = ''
        set -e
        cargo bin cargo-nextest run --workspace --all-features --no-tests pass
        cargo insta accept
      '';
      description = "Update insta snapshots.";
      binary = "bash";
    };
    "setup:helix" = {
      exec = ''
        set -e
        rm -rf .helix
        cp -r setup/editors/helix .helix
      '';
      description = "Setup Helix editor configuration.";
      binary = "bash";
    };
    "setup:vscode" = {
      exec = ''
        set -e
        rm -rf .vscode
        cp -r ./setup/editors/vscode .vscode
      '';
      description = "Setup VS Code workspace configuration.";
      binary = "bash";
    };
  };
}
