#!/usr/bin/env node

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path, { isAbsolute, relative, resolve } from "node:path";

function parseArgs(argv) {
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

function normalizeSourcePath(filePath, repoRoot) {
	return isAbsolute(filePath) ? filePath : resolve(repoRoot, filePath);
}

function isSubpath(candidatePath, parentPath) {
	const relativePath = relative(parentPath, candidatePath);
	return relativePath === "" || (!relativePath.startsWith("..") && !path.isAbsolute(relativePath));
}

function parsePublicCrates(repoRoot) {
	const cratesRoot = resolve(repoRoot, "crates");

	return readdirSync(cratesRoot, { withFileTypes: true })
		.filter((entry) => entry.isDirectory())
		.map((entry) => {
			const directory = resolve(cratesRoot, entry.name);
			const cargoToml = readFileSync(resolve(directory, "Cargo.toml"), "utf8");
			const nameMatch = /^name\s*=\s*"([^"]+)"/mu.exec(cargoToml);
			if (!nameMatch) {
				throw new Error(`unable to read package name from crates/${entry.name}/Cargo.toml`);
			}

			return {
				name: nameMatch[1],
				directory,
				publish: !/^publish\s*=\s*false$/mu.test(cargoToml),
			};
		})
		.filter((crateInfo) => crateInfo.publish)
		.toSorted((left, right) => left.name.localeCompare(right.name));
}

function parseLcovRecords(text, repoRoot) {
	return text
		.split(/^end_of_record$/mu)
		.map((chunk) => chunk.trim())
		.filter(Boolean)
		.map((chunk) => {
			const sourceLine = chunk.split(/\r?\n/u).find((line) => line.startsWith("SF:"));
			if (!sourceLine) {
				throw new Error("encountered LCOV record without an SF line");
			}

			return {
				sourcePath: normalizeSourcePath(sourceLine.slice(3), repoRoot),
				text: `${chunk}\nend_of_record\n`,
			};
		});
}

function splitCoverageByCrate({ lcovText, repoRoot }) {
	const publicCrates = parsePublicCrates(repoRoot);
	const records = parseLcovRecords(lcovText, repoRoot);
	const coverageByCrate = new Map(publicCrates.map((crateInfo) => [crateInfo.name, []]));

	for (const record of records) {
		const crateInfo = publicCrates.find((candidate) =>
			isSubpath(record.sourcePath, candidate.directory),
		);
		if (!crateInfo) {
			continue;
		}

		coverageByCrate.get(crateInfo.name).push(record.text);
	}

	for (const crateInfo of publicCrates) {
		if ((coverageByCrate.get(crateInfo.name) ?? []).length > 0) {
			continue;
		}

		throw new Error(`no LCOV coverage records matched public crate ${crateInfo.name}`);
	}

	return publicCrates.map((crateInfo) => ({
		name: crateInfo.name,
		records: coverageByCrate.get(crateInfo.name) ?? [],
	}));
}

function writeFlagReports(crateReports, outDir) {
	mkdirSync(outDir, { recursive: true });

	for (const crateReport of crateReports) {
		writeFileSync(resolve(outDir, `${crateReport.name}.lcov`), crateReport.records.join(""));
	}
}

function writeGitHubOutput(crateReports, githubOutputPath) {
	if (!githubOutputPath) {
		return;
	}

	writeFileSync(
		githubOutputPath,
		`flags=${JSON.stringify(crateReports.map((crateReport) => crateReport.name))}\n`,
		{ flag: "a" },
	);
}

function main(argv = process.argv.slice(2)) {
	const options = parseArgs(argv);
	const repoRoot = options["repo-root"] ? resolve(options["repo-root"]) : process.cwd();
	const lcovPath = options.lcov
		? resolve(repoRoot, options.lcov)
		: resolve(repoRoot, "target/coverage/lcov.info");
	const outDir = options["out-dir"]
		? resolve(repoRoot, options["out-dir"])
		: resolve(repoRoot, "target/coverage/flags");

	const lcovText = readFileSync(lcovPath, "utf8");
	const crateReports = splitCoverageByCrate({ lcovText, repoRoot });
	writeFlagReports(crateReports, outDir);
	writeGitHubOutput(crateReports, options["github-output"]);

	console.log(`prepared ${crateReports.length} Codecov flag report(s) in ${outDir}`);
	for (const crateReport of crateReports) {
		console.log(`- ${crateReport.name}`);
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
