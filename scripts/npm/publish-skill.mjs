#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";

function parseArgs(argv) {
	const args = {};
	for (let index = 0; index < argv.length; index += 1) {
		const key = argv[index];
		const value = argv[index + 1];
		if (!key.startsWith("--") || value === undefined) {
			continue;
		}
		args[key.slice(2)] = value;
		index += 1;
	}
	return args;
}

function run(command, args, options = {}) {
	const result = spawnSync(command, args, {
		encoding: "utf8",
		stdio: options.stdio ?? "pipe",
		cwd: options.cwd,
	});
	if (result.status !== 0) {
		const detail = result.stderr || result.stdout ||
			`exit code ${result.status ?? "unknown"}`;
		throw new Error(`${command} ${args.join(" ")} failed: ${detail}`);
	}
	return result;
}

function packageExists(name, version) {
	const result = spawnSync("npm", [
		"view",
		`${name}@${version}`,
		"version",
		"--json",
	], {
		encoding: "utf8",
		stdio: "pipe",
	});
	return result.status === 0;
}

function main() {
	const args = parseArgs(process.argv.slice(2));
	const packageDir = resolve(args["package-dir"] ?? "");
	if (!args["package-dir"]) {
		throw new Error("usage: publish-skill.mjs --package-dir <dir>");
	}

	const packageJson = JSON.parse(
		readFileSync(join(packageDir, "package.json"), "utf8"),
	);
	if (packageExists(packageJson.name, packageJson.version)) {
		console.log(
			`Skipping ${packageJson.name}@${packageJson.version}; already published.`,
		);
		return;
	}

	console.log(`Publishing ${packageJson.name}@${packageJson.version}...`);
	run("npm", ["publish", "--access", "public", "--provenance"], {
		cwd: packageDir,
		stdio: "inherit",
	});
}

main();
