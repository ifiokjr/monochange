# Hosted release benchmarks

The default binary benchmark workflow uses synthetic local fixtures so every pull request can run quickly in CI.

When you need to measure hosted-provider overhead for the real `mc release` path, use a dedicated hosted fixture repository instead. That lets the benchmark include GitHub request cost, realistic history shape, and changesets that actually arrived through pull requests.

## Create the fixture repository

Use the helper script in this repository to create a repeatable fixture with:

- multiple Cargo packages
- more than 200 commits by default
- release changesets introduced from PR-shaped branches

Local dry run:

```bash
scripts/setup_hosted_benchmark_fixture.sh \
  --local-only \
  --output-dir /tmp/monochange-release-benchmark-fixture \
  --owner ifiokjr \
  --repo monochange-release-benchmark-fixture
```

Hosted GitHub repo:

```bash
scripts/setup_hosted_benchmark_fixture.sh \
  --output-dir /tmp/monochange-release-benchmark-fixture \
  --owner ifiokjr \
  --repo monochange-release-benchmark-fixture
```

The hosted mode requires:

- `gh auth status` to succeed with `repo` scope
- permission to create repositories under the chosen owner

The generator stores an authenticated HTTPS remote in the disposable fixture clone so it can push the seeded PR branches without depending on SSH agent state. Use a temporary output directory for hosted runs.

## Benchmark the hosted fixture

Build the `main` and PR binaries first, then run the benchmark script against a clone of the hosted fixture repository:

```bash
gh repo clone ifiokjr/monochange-release-benchmark-fixture /tmp/monochange-release-benchmark-fixture

.github/scripts/benchmark_cli.sh run-fixture \
  --main-bin /tmp/mc-main \
  --pr-bin /tmp/mc-pr \
  --fixture-dir /tmp/monochange-release-benchmark-fixture \
  --scenario-id hosted_github \
  --scenario-name "Hosted GitHub fixture" \
  --scenario-description "8 packages, >200 commits, PR-originated changesets" \
  --output /tmp/hosted-benchmark.md \
  --violations-output /tmp/hosted-benchmark-violations.txt
```

This produces the same markdown summary format as the CI benchmark comment, but it benchmarks a real hosted repository checkout instead of a synthetic local fixture.

## Reading the result

Focus on:

- the overall `mc release` delta between `main` and the PR binary
- the `prepare release total` row in the phase table
- hosted-specific phases such as `enrich changeset context via github`

If the hosted run still shows a regression or an unexpectedly large absolute cost, capture a trace against the same fixture checkout and attach both the benchmark markdown and trace notes to the relevant issue or pull request.
