#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

const WARMUP_RUNS = 1;
const BENCHMARK_RUNS = 6;
const PHASE_COMMAND_LABELS = ["mc release --dry-run", "mc release"];
const PHASE_COMMAND_ARGS = [["release", "--dry-run"], ["release"]];
const COMMAND_LABELS = [
	"mc validate",
	"mc discover --format json",
	"mc release --dry-run",
	"mc release",
];
const COMMAND_ARGS = [
	["validate"],
	["discover", "--format", "json"],
	["release", "--dry-run"],
	["release"],
];
const SCENARIOS = [
	{ id: "baseline", name: "Baseline fixture", packages: 20, changesets: 50, commits: 50 },
	{
		id: "history_x10",
		name: "Large history fixture",
		packages: 200,
		changesets: 500,
		commits: 500,
	},
];
const scriptDir = dirname(fileURLToPath(import.meta.url));
const phaseBudgetsFile = join(scriptDir, "benchmark-phase-budgets.json");
const hyperfineBin = process.env.MONOCHANGE_HYPERFINE_BIN ?? "hyperfine";

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
		`monochange-bench-${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}${suffix}`,
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

function parseHyperfineTable(path) {
	const lines = readText(path).split(/\r?\n/);
	const results = [];
	let hasRelative = false;
	for (const raw of lines) {
		const line = raw.trim();
		if (!line || (line.startsWith("|") && line.startsWith("| Com"))) {
			if (line.includes("Relative")) hasRelative = true;
			continue;
		}
		if (line.startsWith("|:") || !line.startsWith("|")) continue;
		const cols = line
			.split("|")
			.map((col) => col.trim())
			.filter(Boolean);
		if (cols.length === 0) continue;
		const label = cols[0].replace(/^`|`$/g, "").trim();
		if (!label) continue;
		if (hasRelative && cols.length >= 5) {
			const relative = Number.parseFloat(
				cols
					.at(-1)
					.replace(/\\u00b1.*|±.*|\u00b1.*/u, "")
					.trim(),
			);
			if (Number.isNaN(relative) || label.startsWith("main")) continue;
			results.push({
				label,
				status: relative < 0.98 ? "improved" : relative > 1.02 ? "regressed" : "flat",
				relative,
			});
		}
	}
	return { results, hasRelative };
}

function parsePhaseStatus(path) {
	const counts = { improved: 0, regressed: 0, flat: 0, "over budget": 0 };
	for (const line of readText(path).split(/\r?\n/)) {
		if (line.includes("| regressed |")) counts.regressed += 1;
		else if (line.includes("| improved |")) counts.improved += 1;
		else if (line.includes("| flat |")) counts.flat += 1;
		else if (line.includes("| over budget |")) counts["over budget"] += 1;
	}
	return counts;
}

function summarizeScenarioStatus(tablePath, phaseTablePath) {
	const improved = "🟢";
	const regressed = "🔴";
	const flat = "⚪";
	const shortNames = new Map([
		["mc validate", "validate"],
		["mc discover --format json", "discover"],
		["mc release --dry-run", "dry-run"],
		["mc release", "release"],
	]);
	const hyperfine = parseHyperfineTable(tablePath);
	const phases = parsePhaseStatus(phaseTablePath);
	if (hyperfine.results.length === 0 && !Object.values(phases).some((value) => value > 0))
		return "";
	const parts = [];
	for (const result of hyperfine.results) {
		let short = result.label;
		for (const [full, abbr] of shortNames)
			if (result.label.includes(full)) {
				short = abbr;
				break;
			}
		parts.push(
			`${result.status === "improved" ? improved : result.status === "regressed" ? regressed : flat} ${short}`,
		);
	}
	if (phases.improved > 0 && !hyperfine.hasRelative) parts.push(`${improved} phases improved`);
	if (phases.regressed > 0 && !hyperfine.hasRelative) parts.push(`${regressed} phases regressed`);
	if (phases["over budget"] > 0) parts.push("🚨 over budget");
	return parts.join(" ");
}

function renderComment(outputPath, scenarios) {
	const lines = [
		"## Binary Benchmark: main vs PR",
		"",
		`Measured with \`hyperfine --warmup ${WARMUP_RUNS} --runs ${BENCHMARK_RUNS}\`.`,
		"",
		"Commands:",
	];
	for (const label of COMMAND_LABELS) lines.push(`- \`${label}\``);
	for (const scenario of scenarios) {
		const status = summarizeScenarioStatus(scenario.tablePath, scenario.phaseTablePath);
		lines.push("", "<details>");
		lines.push(
			status
				? `<summary><strong>${scenario.name}</strong> — ${scenario.description} &nbsp; ${status}</summary>`
				: `<summary><strong>${scenario.name}</strong> — ${scenario.description}</summary>`,
		);
		lines.push("", readText(scenario.tablePath).trimEnd());
		const phaseText = readText(scenario.phaseTablePath).trimEnd();
		if (phaseText) lines.push("", phaseText);
		lines.push("", "</details>");
	}
	writeFileSync(outputPath, `${lines.join("\n")}\n`);
}

function supportsJsonProgress(bin) {
	const result = spawnSync(bin, ["--help"], { encoding: "utf8" });
	return `${result.stdout ?? ""}${result.stderr ?? ""}`.includes("--progress-format");
}

function runPhaseCapture(bin, fixtureDir, commandArgs, eventsPath) {
	run("git", ["reset", "--hard", "HEAD"], { cwd: fixtureDir });
	run("git", ["clean", "-fd"], { cwd: fixtureDir });
	const result = spawnSync(bin, ["--progress-format", "json", ...commandArgs], {
		cwd: fixtureDir,
		encoding: "utf8",
	});
	writeFileSync(eventsPath, result.stderr ?? "");
	if (result.status !== 0)
		throw new Error(`${bin} ${commandArgs.join(" ")} failed: ${result.stderr || result.stdout}`);
}

function summarizeProgressEvents(eventsPath, outputPath) {
	const phaseTotals = new Map();
	let stepTotalMs = 0;
	for (const line of readText(eventsPath).split(/\r?\n/)) {
		if (!line.trim()) continue;
		const event = JSON.parse(line);
		if (event.event !== "step_finished" || event.stepKind !== "PrepareRelease") continue;
		stepTotalMs += Number.parseInt(event.durationMs || 0, 10);
		for (const phase of event.phaseTimings ?? []) {
			if (!phase.label) continue;
			phaseTotals.set(
				phase.label,
				(phaseTotals.get(phase.label) ?? 0) + Number.parseInt(phase.durationMs || 0, 10),
			);
		}
	}
	const phases = [...phaseTotals]
		.toSorted((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
		.map(([label, durationMs]) => ({ label, durationMs }));
	writeFileSync(outputPath, `${JSON.stringify({ stepTotalMs, phases }, null, 2)}\n`);
}

function unavailableSummary(path) {
	writeFileSync(
		path,
		`${JSON.stringify({ available: false, stepTotalMs: null, phases: [] }, null, 2)}\n`,
	);
}

function phaseMap(summary) {
	return summary.available === false
		? new Map()
		: new Map(
				(summary.phases ?? []).map((phase) => [phase.label, Number.parseInt(phase.durationMs, 10)]),
			);
}

function statusLabel(mainMs, prMs, budgetMs) {
	if (prMs == null) return "unavailable";
	if (budgetMs != null && prMs > budgetMs) return "over budget";
	if (mainMs == null) return budgetMs != null ? "budget only" : "pr only";
	if (prMs > mainMs) return "regressed";
	if (prMs < mainMs) return "improved";
	return "flat";
}

function formatMs(value) {
	return value == null ? "n/a" : String(Number.parseInt(value, 10));
}

function delta(prMs, mainMs) {
	if (prMs == null || mainMs == null) return "n/a";
	const value = prMs - mainMs;
	return `${value >= 0 ? "+" : ""}${value}`;
}

function renderPhaseMarkdown(
	scenarioId,
	outputPath,
	violationsPath,
	dryMainPath,
	dryPrPath,
	releaseMainPath,
	releasePrPath,
) {
	const budgets = JSON.parse(readText(phaseBudgetsFile) || "{}")[scenarioId] ?? {};
	const summaries = {
		"mc release --dry-run": {
			main: JSON.parse(readText(dryMainPath)),
			pr: JSON.parse(readText(dryPrPath)),
		},
		"mc release": {
			main: JSON.parse(readText(releaseMainPath)),
			pr: JSON.parse(readText(releasePrPath)),
		},
	};
	const sections = ["#### Phase timings", ""];
	let violations = 0;
	for (const commandLabel of PHASE_COMMAND_LABELS) {
		const commandBudget = budgets[commandLabel] ?? {};
		const phaseBudget = commandBudget.phases ?? {};
		const main = summaries[commandLabel].main;
		const pr = summaries[commandLabel].pr;
		const mainPhases = phaseMap(main);
		const prPhases = phaseMap(pr);
		const rows = [
			[
				"prepare release total",
				commandBudget.stepTotalMs,
				main.stepTotalMs == null ? null : Number.parseInt(main.stepTotalMs, 10),
				pr.stepTotalMs == null ? null : Number.parseInt(pr.stepTotalMs, 10),
			],
		];
		const labels = [...new Set([...mainPhases.keys(), ...prPhases.keys()])].toSorted(
			(a, b) =>
				Math.max(prPhases.get(b) ?? 0, mainPhases.get(b) ?? 0) -
					Math.max(prPhases.get(a) ?? 0, mainPhases.get(a) ?? 0) || a.localeCompare(b),
		);
		for (const label of labels)
			rows.push([label, phaseBudget[label], mainPhases.get(label) ?? 0, prPhases.get(label) ?? 0]);
		sections.push(`##### \`${commandLabel}\``, "");
		if (main.available === false)
			sections.push(
				"_`main` does not support `--progress-format json`; phase timings are shown for the PR binary against the configured budgets only._",
				"",
			);
		sections.push(
			"| Phase | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status |",
			"|:---|---:|---:|---:|---:|:---|",
		);
		for (const [label, budgetMs, mainMs, prMs] of rows) {
			if (budgetMs != null && prMs != null && prMs > budgetMs) violations += 1;
			sections.push(
				`| \`${label}\` | ${formatMs(budgetMs)} | ${formatMs(mainMs)} | ${formatMs(prMs)} | ${delta(prMs, mainMs)} | ${statusLabel(mainMs, prMs, budgetMs)} |`,
			);
		}
		sections.push("");
	}
	writeFileSync(violationsPath, String(violations));
	writeFileSync(outputPath, `${sections.join("\n").trimEnd()}\n`);
}

function collectPhaseMarkdown(scenarioId, fixtureDir, mainBin, prBin, phasePath, violationsPath) {
	const paths = ["dry-main", "dry-pr", "release-main", "release-pr"].map((name) =>
		tempPath(`-${name}.json`),
	);
	if (supportsJsonProgress(mainBin)) {
		const dry = tempPath("-dry-main.jsonl");
		const release = tempPath("-release-main.jsonl");
		runPhaseCapture(mainBin, fixtureDir, PHASE_COMMAND_ARGS[0], dry);
		runPhaseCapture(mainBin, fixtureDir, PHASE_COMMAND_ARGS[1], release);
		summarizeProgressEvents(dry, paths[0]);
		summarizeProgressEvents(release, paths[2]);
	} else {
		unavailableSummary(paths[0]);
		unavailableSummary(paths[2]);
	}
	if (supportsJsonProgress(prBin)) {
		const dry = tempPath("-dry-pr.jsonl");
		const release = tempPath("-release-pr.jsonl");
		runPhaseCapture(prBin, fixtureDir, PHASE_COMMAND_ARGS[0], dry);
		runPhaseCapture(prBin, fixtureDir, PHASE_COMMAND_ARGS[1], release);
		summarizeProgressEvents(dry, paths[1]);
		summarizeProgressEvents(release, paths[3]);
	} else {
		unavailableSummary(paths[1]);
		unavailableSummary(paths[3]);
	}
	renderPhaseMarkdown(scenarioId, phasePath, violationsPath, ...paths);
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

function runScenario(mainBin, prBin, fixtureDir, tablePath) {
	const args = [
		"--prepare",
		"git reset --hard HEAD >/dev/null && git clean -fd >/dev/null",
		"--style",
		"basic",
		"--warmup",
		String(WARMUP_RUNS),
		"--runs",
		String(BENCHMARK_RUNS),
		"--time-unit",
		"millisecond",
		"--export-markdown",
		tablePath,
	];
	for (let i = 0; i < COMMAND_LABELS.length; i += 1) {
		args.push(
			"--command-name",
			`main · ${COMMAND_LABELS[i]}`,
			`${mainBin} ${COMMAND_ARGS[i].join(" ")}`,
		);
		args.push(
			"--command-name",
			`pr · ${COMMAND_LABELS[i]}`,
			`${prBin} ${COMMAND_ARGS[i].join(" ")}`,
		);
	}
	run(hyperfineBin, args, { cwd: fixtureDir, stdio: "inherit" });
}

function runFixtureMode(args) {
	const o = parseOptions(args, [
		"--main-bin",
		"--pr-bin",
		"--fixture-dir",
		"--scenario-id",
		"--scenario-name",
		"--scenario-description",
		"--output",
		"--violations-output",
	]);
	for (const key of [
		"main_bin",
		"pr_bin",
		"fixture_dir",
		"scenario_id",
		"scenario_name",
		"scenario_description",
		"output",
	])
		if (!o[key])
			die(
				"run-fixture requires --main-bin, --pr-bin, --fixture-dir, --scenario-id, --scenario-name, --scenario-description, and --output",
			);
	const tablePath = tempPath("-table.md");
	const phasePath = tempPath("-phases.md");
	const violationsPath = tempPath("-violations.txt");
	runScenario(o.main_bin, o.pr_bin, o.fixture_dir, tablePath);
	collectPhaseMarkdown(
		o.scenario_id,
		o.fixture_dir,
		o.main_bin,
		o.pr_bin,
		phasePath,
		violationsPath,
	);
	renderComment(o.output, [
		{
			name: o.scenario_name,
			description: o.scenario_description,
			tablePath,
			phaseTablePath: phasePath,
		},
	]);
	if (o.violations_output) writeFileSync(o.violations_output, readText(violationsPath));
}

function runMode(args) {
	const o = parseOptions(args, ["--main-bin", "--pr-bin", "--output", "--violations-output"]);
	for (const key of ["main_bin", "pr_bin", "output"])
		if (!o[key]) die("run requires --main-bin, --pr-bin, and --output");
	const renderArgs = [];
	let totalViolations = 0;
	for (const scenario of SCENARIOS) {
		const tablePath = tempPath("-table.md");
		const phasePath = tempPath("-phases.md");
		const violationsPath = tempPath("-violations.txt");
		const fixtureDir = tempDir();
		generateFixture(fixtureDir, scenario.packages, scenario.changesets, scenario.commits);
		runScenario(o.main_bin, o.pr_bin, fixtureDir, tablePath);
		collectPhaseMarkdown(scenario.id, fixtureDir, o.main_bin, o.pr_bin, phasePath, violationsPath);
		totalViolations += Number.parseInt(readText(violationsPath) || "0", 10);
		rmSync(fixtureDir, { recursive: true, force: true });
		renderArgs.push({
			name: scenario.name,
			description: `${scenario.packages} packages, ${scenario.changesets} changesets, ${scenario.commits} commits`,
			tablePath,
			phaseTablePath: phasePath,
		});
	}
	renderComment(o.output, renderArgs);
	if (o.violations_output) writeFileSync(o.violations_output, `${totalViolations}\n`);
}

function renderFixtureMode(args) {
	const o = parseOptions(args, ["--fixture-dir", "--output"]);
	if (!o.fixture_dir || !o.output) die("render-fixture requires --fixture-dir and --output");
	renderComment(o.output, [
		{
			name: "Baseline fixture",
			description: "20 packages, 50 changesets, 50 commits",
			tablePath: join(o.fixture_dir, "baseline.md"),
			phaseTablePath: join(o.fixture_dir, "baseline-phases.md"),
		},
		{
			name: "Large history fixture",
			description: "200 packages, 500 changesets, 500 commits",
			tablePath: join(o.fixture_dir, "history_x10.md"),
			phaseTablePath: join(o.fixture_dir, "history_x10-phases.md"),
		},
	]);
}

const [mode, ...args] = process.argv.slice(2);
try {
	if (mode === "run") runMode(args);
	else if (mode === "run-fixture") runFixtureMode(args);
	else if (mode === "render-fixture") renderFixtureMode(args);
	else die(`usage: ${process.argv[1]} <run|run-fixture|render-fixture> [args...]`);
} catch (error) {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
}
