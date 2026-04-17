#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { isAbsolute, resolve } from "node:path";

export function parseArgs(argv) {
	const options = {};

	for (let index = 0; index < argv.length; index += 1) {
		const token = argv[index];
		if (!token.startsWith("--")) {
			continue;
		}

		const value = argv[index + 1];
		if (value === undefined || value.startsWith("--")) {
			continue;
		}

		options[token.slice(2)] = value;
		index += 1;
	}

	return options;
}

function normalizePath(filePath, repoRoot) {
	if (filePath === "/dev/null") {
		return null;
	}

	if (filePath.startsWith("b/")) {
		filePath = filePath.slice(2);
	}

	return isAbsolute(filePath) ? filePath : resolve(repoRoot, filePath);
}

export function parseLcov(text, repoRoot = process.cwd()) {
	const coverageByFile = new Map();
	let currentFile = null;

	for (const line of text.split(/\r?\n/u)) {
		if (line.startsWith("SF:")) {
			currentFile = normalizePath(line.slice(3), repoRoot);
			if (currentFile && !coverageByFile.has(currentFile)) {
				coverageByFile.set(currentFile, new Map());
			}
			continue;
		}

		if (!currentFile || !line.startsWith("DA:")) {
			continue;
		}

		const [lineNumberToken, hitsToken] = line.slice(3).split(",", 2);
		const lineNumber = Number.parseInt(lineNumberToken, 10);
		const hits = Number.parseInt(hitsToken, 10);

		if (!Number.isFinite(lineNumber) || !Number.isFinite(hits)) {
			continue;
		}

		coverageByFile.get(currentFile).set(lineNumber, hits);
	}

	return coverageByFile;
}

function ensureLineSet(changedLinesByFile, filePath) {
	if (!changedLinesByFile.has(filePath)) {
		changedLinesByFile.set(filePath, new Set());
	}

	return changedLinesByFile.get(filePath);
}

export function parseChangedLines(text, repoRoot = process.cwd()) {
	const changedLinesByFile = new Map();
	let currentFile = null;

	for (const line of text.split(/\r?\n/u)) {
		if (line.startsWith("+++ ")) {
			currentFile = normalizePath(line.slice(4).trim(), repoRoot);
			continue;
		}

		if (!currentFile || !line.startsWith("@@ ")) {
			continue;
		}

		const match = /@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/u.exec(line);
		if (!match) {
			continue;
		}

		const start = Number.parseInt(match[1], 10);
		const count = match[2] === undefined ? 1 : Number.parseInt(match[2], 10);
		if (!Number.isFinite(start) || !Number.isFinite(count) || count === 0) {
			continue;
		}

		const lines = ensureLineSet(changedLinesByFile, currentFile);
		for (let lineNumber = start; lineNumber < start + count; lineNumber += 1) {
			lines.add(lineNumber);
		}
	}

	return changedLinesByFile;
}

export function computePatchCoverage(coverageByFile, changedLinesByFile) {
	let coveredLines = 0;
	let executableChangedLines = 0;
	const uncoveredLines = [];

	for (const [filePath, changedLines] of changedLinesByFile.entries()) {
		const lineCoverage = coverageByFile.get(filePath);
		if (!lineCoverage) {
			continue;
		}

		const sortedChangedLines = [...changedLines].sort((left, right) =>
			left - right
		);
		for (const lineNumber of sortedChangedLines) {
			if (!lineCoverage.has(lineNumber)) {
				continue;
			}

			executableChangedLines += 1;
			if ((lineCoverage.get(lineNumber) ?? 0) > 0) {
				coveredLines += 1;
				continue;
			}

			uncoveredLines.push({ filePath, lineNumber });
		}
	}

	const percentage = executableChangedLines === 0
		? 100
		: (coveredLines / executableChangedLines) * 100;

	return {
		coveredLines,
		executableChangedLines,
		percentage,
		uncoveredLines,
	};
}

export function formatCoverageSummary(result, target) {
	const percentage = result.percentage.toFixed(2);
	const summary =
		`PATCH_COVERAGE ${result.coveredLines}/${result.executableChangedLines} (${percentage}%)`;
	if (result.executableChangedLines === 0) {
		return `${summary}\nNo executable changed lines were found in the coverage report.`;
	}

	if (target === 100 && result.coveredLines === result.executableChangedLines) {
		return `${summary}\nPatch coverage meets the required 100.00% target.`;
	}

	if (result.percentage >= target) {
		return `${summary}\nPatch coverage meets the required ${
			target.toFixed(2)
		}% target.`;
	}

	const missingLines = result.uncoveredLines
		.map(({ filePath, lineNumber }) => `- ${filePath}:${lineNumber}`)
		.join("\n");
	return `${summary}\nRequired patch coverage: ${
		target.toFixed(2)
	}%\nUncovered executable changed lines:\n${missingLines}`;
}

export function verifyPatchCoverage(
	{ lcovText, diffText, repoRoot, target = 100 },
) {
	const coverageByFile = parseLcov(lcovText, repoRoot);
	const changedLinesByFile = parseChangedLines(diffText, repoRoot);
	const result = computePatchCoverage(coverageByFile, changedLinesByFile);
	const passed = target === 100
		? result.coveredLines === result.executableChangedLines
		: result.percentage >= target;

	return {
		...result,
		passed,
		summary: formatCoverageSummary(result, target),
	};
}

function run(command, args, options = {}) {
	const result = spawnSync(command, args, {
		encoding: "utf8",
		...options,
	});

	if (result.status === 0) {
		return result.stdout;
	}

	const output = result.stderr || result.stdout || "command failed";
	throw new Error(`${command} ${args.join(" ")} failed: ${output.trim()}`);
}

export function main(argv = process.argv.slice(2)) {
	const options = parseArgs(argv);
	const repoRoot = options["repo-root"]
		? resolve(options["repo-root"])
		: process.cwd();
	const base = options.base;
	const head = options.head ?? "HEAD";
	const lcovPath = options.lcov
		? resolve(repoRoot, options.lcov)
		: resolve(repoRoot, "target/coverage/lcov.info");
	const target = options.target === undefined
		? 100
		: Number.parseFloat(options.target);

	if (!base) {
		throw new Error("missing required --base <git-ref> option");
	}
	if (!Number.isFinite(target)) {
		throw new Error(`invalid --target value: ${options.target}`);
	}

	const lcovText = readFileSync(lcovPath, "utf8");
	const diffText = run(
		"git",
		[
			"diff",
			"--unified=0",
			"--no-color",
			"--no-ext-diff",
			`${base}...${head}`,
		],
		{ cwd: repoRoot },
	);

	const result = verifyPatchCoverage({ lcovText, diffText, repoRoot, target });
	console.log(result.summary);

	if (!result.passed) {
		process.exitCode = 1;
	}
}

if (import.meta.url === `file://${process.argv[1]}`) {
	try {
		main();
	} catch (error) {
		console.error(error instanceof Error ? error.message : String(error));
		process.exitCode = 1;
	}
}
