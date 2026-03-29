{
  pkgs,
  lib,
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
  '';

  # disable dotenv since it breaks the variable interpolation supported by `direnv`
  dotenv.disableHint = true;

  scripts = {
    "monochange" = {
      exec = ''
        set -e
        cargo run --quiet --package monochange --bin monochange -- "$@"
      '';
      description = "The `monochange` executable";
      binary = "bash";
    };
    "mc" = {
      exec = ''
        set -e
        cargo run --quiet --package monochange --bin mc -- "$@"
      '';
      description = "Alias for the `monochange` executable";
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
    "update:deps" = {
      exec = ''
        set -e
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
      '';
      description = "Run all tests across the crates.";
      binary = "bash";
    };
    "test:cargo" = {
      exec = ''
        set -e
        cargo nextest run --workspace --all-features --no-tests pass
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
    "coverage:all" = {
      exec = ''
        set -euo pipefail
        mkdir -p target/coverage
        cargo llvm-cov clean --workspace
        cargo llvm-cov nextest --workspace --all-features --all-targets --no-report
        cargo llvm-cov report --summary-only --fail-under-lines 70
        cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
      '';
      description = "Run workspace coverage, enforce a 70% line-coverage floor, and write target/coverage/lcov.info.";
      binary = "bash";
    };
    "fix:all" = {
      exec = ''
        set -e
        fix:clippy
        docs:update
        fix:format
      '';
      description = "Fix all autofixable problems.";
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
        cargo clippy --workspace --all-features --all-targets
      '';
      description = "Check that all rust lints are passing.";
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
        cargo nextest run --workspace --all-features --no-tests pass
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
