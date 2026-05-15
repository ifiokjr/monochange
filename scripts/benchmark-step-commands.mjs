#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const DEFAULT_WARMUP_RUNS = 1;
const DEFAULT_BENCHMARK_RUNS = 6;
const hyperfineBin = process.env.MONOCHANGE_HYPERFINE_BIN ?? "hyperfine";

const RUNNABLE_STEP_COMMANDS = [
	{
		label: "mc step:config --dry-run",
		args: ["step:config", "--dry-run"],
	},
	{
		label: "mc step:validate --dry-run",
		args: ["step:validate", "--dry-run"],
	},
	{
		label: "mc step:discover --dry-run --format json",
		args: ["step:discover", "--dry-run", "--format", "json"],
	},
	{
		label: "mc step:display-versions --dry-run --format json",
		args: ["step:display-versions", "--dry-run", "--format", "json"],
	},
	{
		label: "mc step:create-change-file --dry-run",
		args: [
			"step:create-change-file",
			"--dry-run",
			"--package",
			"pkg-0",
			"--bump",
			"patch",
			"--reason",
			"Benchmark change",
			"--type",
			"patch",
		],
	},
	{
		label: "mc step:prepare-release --dry-run --format json",
		args: ["step:prepare-release", "--dry-run", "--format", "json"],
	},
	{
		label: "mc step:affected-packages --dry-run --format json",
		args: ["step:affected-packages", "--dry-run", "--format", "json", "--from", "HEAD~1"],
	},
	{
		label: "mc step:diagnose-changesets --dry-run --format json",
		args: ["step:diagnose-changesets", "--dry-run", "--format", "json"],
	},
];

const SKIPPED_STEP_COMMANDS = [
	{
		command: "mc step:commit-release",
		reason:
			"requires a PrepareRelease step in the same workflow context; the direct built-in step has no prepared-release input flag",
	},
	{
		command: "mc step:verify-release-branch",
		reason: "requires [source] configuration and release-branch provider semantics",
	},
	{
		command: "mc step:publish-release",
		reason:
			"requires a prepared release artifact plus hosted source-provider configuration; can perform provider I/O",
	},
	{
		command: "mc step:placeholder-publish",
		reason:
			"requires registry publish configuration and can perform registry/provider I/O even in dry-run planning",
	},
	{
		command: "mc step:publish-packages",
		reason: "requires registry publish configuration and package publish state",
	},
	{
		command: "mc step:plan-publish-rate-limits",
		reason:
			"requires publish-readiness/registry configuration; timed out on the offline cargo fixture",
	},
	{
		command: "mc step:open-release-request",
		reason: "requires hosted source-provider configuration and can create or update provider PRs",
	},
	{
		command: "mc step:comment-released-issues",
		reason: "requires hosted source-provider configuration and issue-comment side effects",
	},
	{
		command: "mc step:retarget-release",
		reason:
			"requires release tag/source-provider state; fixture coverage needs a dedicated isolated provider setup",
	},
];

function die(message) {
	console.error(message);
	process.exit(1);
}

function run(command, args, options = {}) {
	const result = spawnSync(command, args, {
		encoding: "utf8",
		stdio: options.stdio ?? "pipe",
		cwd: options.cwd,
		env: options.env ?? process.env,
	});
	if (result.status !== 0) {
		const detail = result.stderr || result.stdout || `exit code ${result.status ?? "unknown"}`;
		throw new Error(`${command} ${args.join(" ")} failed: ${detail}`);
	}
	return result;
}

function parseOptions(args, names) {
	const options = {};
	for (let index = 0; index < args.length; index += 1) {
		const key = args[index];
		if (!names.includes(key)) die(`unknown argument: ${key}`);
		const value = args[index + 1];
		if (value === undefined) die(`missing value for ${key}`);
		options[key.slice(2).replaceAll("-", "_")] = value;
		index += 1;
	}
	return options;
}

function tempPath(suffix) {
	return join(
		tmpdir(),
		`monochange-step-bench-${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}${suffix}`,
	);
}

function tempDir() {
	const path = tempPath("");
	mkdirSync(path, { recursive: true });
	return path;
}

function readText(path) {
	try {
		return readFileSync(path, "utf8");
	} catch {
		return "";
	}
}

function shellQuote(value) {
	return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function commandString(bin, args) {
	return `${shellQuote(bin)} ${args.map(shellQuote).join(" ")} >/dev/null 2>/dev/null`;
}

function parseHyperfineMeans(path) {
	const lines = readText(path).split(/\r?\n/);
	const means = new Map();
	for (const raw of lines) {
		const line = raw.trim();
		if (!line.startsWith("|") || line.startsWith("|:") || line.startsWith("| Com")) continue;
		const cols = line
			.split("|")
			.map((col) => col.trim())
			.filter(Boolean);
		if (cols.length < 2) continue;
		const label = cols[0].replace(/^`|`$/g, "").trim();
		const mean = Number.parseFloat(cols[1]);
		if (label && !Number.isNaN(mean)) means.set(label, mean);
	}
	return means;
}

function pairwiseResults(tablePath) {
	const means = parseHyperfineMeans(tablePath);
	return RUNNABLE_STEP_COMMANDS.map((command) => {
		const main = means.get(`main · ${command.label}`);
		const pr = means.get(`pr · ${command.label}`);
		const delta = main == null || pr == null ? null : pr - main;
		const ratio = main == null || pr == null || main === 0 ? null : pr / main;
		const status =
			ratio == null
				? "unavailable"
				: ratio < 0.98
					? "improved"
					: ratio > 1.02
						? "regressed"
						: "flat";
		return { command: command.label, main, pr, delta, ratio, status };
	});
}

function formatNumber(value, digits = 1) {
	return value == null ? "n/a" : value.toFixed(digits);
}

function formatDelta(value) {
	return value == null ? "n/a" : `${value >= 0 ? "+" : ""}${value.toFixed(1)}`;
}

function formatRatio(value) {
	return value == null ? "n/a" : `${value.toFixed(2)}×`;
}

function renderPairwiseSummary(tablePath) {
	const rows = [
		"### Pairwise summary",
		"",
		"| Command | main [ms] | pr [ms] | Δ pr-main [ms] | pr/main | Status |",
		"|:---|---:|---:|---:|---:|:---|",
	];
	for (const result of pairwiseResults(tablePath)) {
		rows.push(
			`| \`${result.command}\` | ${formatNumber(result.main)} | ${formatNumber(result.pr)} | ${formatDelta(result.delta)} | ${formatRatio(result.ratio)} | ${result.status} |`,
		);
	}
	return rows.join("\n");
}

function renderComment(outputPath, tablePath, fixtureDescription, runs, warmup) {
	const rows = [
		"## Step Command Benchmark: main vs PR",
		"",
		`Measured with \`hyperfine --warmup ${warmup} --runs ${runs}\` on ${fixtureDescription}.`,
		"",
		"Runnable built-in step commands:",
	];
	for (const command of RUNNABLE_STEP_COMMANDS) rows.push(`- \`${command.label}\``);
	rows.push(
		"",
		renderPairwiseSummary(tablePath),
		"",
		"### Raw hyperfine table",
		"",
		readText(tablePath).trimEnd(),
		"",
		"### Skipped built-in step commands",
		"",
	);
	rows.push("| Command | Reason |", "|:---|:---|");
	for (const command of SKIPPED_STEP_COMMANDS)
		rows.push(`| \`${command.command}\` | ${command.reason} |`);
	writeFileSync(outputPath, `${rows.join("\n").trimEnd()}\n`);
}

function renderViolations(path, tablePath) {
	const regressions = pairwiseResults(tablePath).filter(
		(result) => result.ratio != null && result.ratio > 1.02,
	);
	writeFileSync(path, `${regressions.length}\n`);
}

function gitCommit(root, message) {
	run("git", [
		"-C",
		root,
		"-c",
		"user.name=bench",
		"-c",
		"user.email=bench@test",
		"-c",
		"commit.gpgsign=false",
		"commit",
		"-m",
		message,
	]);
}

function generateFixture(root, packages, changesets, commits) {
	mkdirSync(join(root, "crates"), { recursive: true });
	mkdirSync(join(root, ".changeset"), { recursive: true });
	writeFileSync(
		join(root, "Cargo.toml"),
		`[workspace]\nmembers = [\n${Array.from({ length: packages }, (_, i) => `  "crates/pkg-${i}",`).join("\n")}\n]\nresolver = "2"\n`,
	);
	let config = '[defaults]\npackage_type = "cargo"\n\n';
	for (let i = 0; i < packages; i += 1) {
		config += `[package.pkg-${i}]\npath = "crates/pkg-${i}"\n\n`;
		mkdirSync(join(root, `crates/pkg-${i}`), { recursive: true });
		writeFileSync(
			join(root, `crates/pkg-${i}/Cargo.toml`),
			`[package]\nname = "pkg-${i}"\nversion = "1.0.0"\nedition = "2021"\ndescription = "Benchmark fixture package ${i}"\nlicense = "MIT"\nrepository = "https://github.com/monochange/monochange"\n`,
		);
	}
	config +=
		'[ecosystems.cargo]\nenabled = true\n\n[cli.discover]\ninputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]\nsteps = [{ type = "Discover" }]\n\n[cli.release]\nsteps = [{ type = "PrepareRelease" }]\n';
	writeFileSync(join(root, "monochange.toml"), config);
	run("git", ["-C", root, "init", "-b", "main"]);
	run("git", ["-C", root, "add", "."]);
	gitCommit(root, "initial");
	for (let commit = 0; commit < commits; commit += 1) {
		const pkg = commit % packages;
		writeFileSync(join(root, `crates/pkg-${pkg}/src.rs`), `// commit ${commit}\n`);
		if (commit < changesets)
			writeFileSync(
				join(root, `.changeset/change-${String(commit).padStart(4, "0")}.md`),
				`---\npkg-${pkg}: patch\n---\n\n# Fix issue ${commit}\n\nFix issue #${commit}.\n`,
			);
		run("git", ["-C", root, "add", "."]);
		gitCommit(root, `change ${commit}`);
	}
	for (let i = commits; i < changesets; i += 1) {
		const pkg = i % packages;
		writeFileSync(
			join(root, `.changeset/change-${String(i).padStart(4, "0")}.md`),
			`---\npkg-${pkg}: patch\n---\n\n# Fix issue ${i}\n\nFix issue #${i}.\n`,
		);
		run("git", ["-C", root, "add", "."]);
		gitCommit(root, `changeset ${i}`);
	}
}

function runBenchmarks(mainBin, prBin, fixtureDir, tablePath, runs, warmup) {
	const args = [
		"--prepare",
		"git reset --hard HEAD >/dev/null && git clean -fd >/dev/null",
		"--style",
		"basic",
		"--warmup",
		String(warmup),
		"--runs",
		String(runs),
		"--time-unit",
		"millisecond",
		"--export-markdown",
		tablePath,
	];
	for (const command of RUNNABLE_STEP_COMMANDS) {
		args.push("--command-name", `main · ${command.label}`, commandString(mainBin, command.args));
		args.push("--command-name", `pr · ${command.label}`, commandString(prBin, command.args));
	}
	run(hyperfineBin, args, { cwd: fixtureDir, stdio: "inherit" });
}

function runMode(args) {
	const o = parseOptions(args, [
		"--main-bin",
		"--pr-bin",
		"--fixture-dir",
		"--output",
		"--violations-output",
		"--packages",
		"--changesets",
		"--commits",
		"--runs",
		"--warmup",
	]);
	for (const key of ["main_bin", "pr_bin", "output"])
		if (!o[key]) die("run requires --main-bin, --pr-bin, and --output");

	const packages = Number.parseInt(o.packages ?? "200", 10);
	const changesets = Number.parseInt(o.changesets ?? "500", 10);
	const commits = Number.parseInt(o.commits ?? "500", 10);
	const runs = Number.parseInt(o.runs ?? String(DEFAULT_BENCHMARK_RUNS), 10);
	const warmup = Number.parseInt(o.warmup ?? String(DEFAULT_WARMUP_RUNS), 10);
	const generatedFixture = !o.fixture_dir;
	const fixtureDir = o.fixture_dir ?? tempDir();
	const tablePath = tempPath("-steps.md");
	const description = generatedFixture
		? `${packages} packages, ${changesets} changesets, ${commits} commits`
		: `fixture \`${fixtureDir}\``;

	if (generatedFixture) generateFixture(fixtureDir, packages, changesets, commits);
	try {
		runBenchmarks(o.main_bin, o.pr_bin, fixtureDir, tablePath, runs, warmup);
		renderComment(o.output, tablePath, description, runs, warmup);
		if (o.violations_output) renderViolations(o.violations_output, tablePath);
	} finally {
		if (generatedFixture) rmSync(fixtureDir, { recursive: true, force: true });
	}
}

const [mode, ...args] = process.argv.slice(2);
try {
	if (mode === "run") runMode(args);
	else
		die(
			`usage: ${process.argv[1]} run --main-bin <path> --pr-bin <path> --output <path> [--fixture-dir <path>]`,
		);
} catch (error) {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
}
