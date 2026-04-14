#!/usr/bin/env node

import { spawnSync as nodeSpawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const PLATFORM_PACKAGE_DIRS = [
	"monochange__cli-darwin-arm64",
	"monochange__cli-darwin-x64",
	"monochange__cli-linux-arm64-gnu",
	"monochange__cli-linux-arm64-musl",
	"monochange__cli-linux-x64-gnu",
	"monochange__cli-linux-x64-musl",
	"monochange__cli-win32-x64-msvc",
	"monochange__cli-win32-arm64-msvc",
];

export const CLI_PACKAGE_DIR = "monochange__cli";

let _spawnSync = nodeSpawnSync;

export function _setSpawnSync(fn) {
	_spawnSync = fn;
}

export function _resetSpawnSync() {
	_spawnSync = nodeSpawnSync;
}

export function parseArgs(argv) {
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

export function run(command, args, options = {}) {
	const result = _spawnSync(command, args, {
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

export function packageMetadata(dir) {
	return JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
}

export function packageExists(name, version) {
	const result = _spawnSync("npm", [
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

export function hasBinary(dir) {
	const binDir = join(dir, "bin");
	if (!existsSync(binDir)) {
		return false;
	}

	const entries = readdirSync(binDir);
	return entries.some((entry) => entry.startsWith("monochange"));
}

export function publishPackage(dir) {
	const pkg = packageMetadata(dir);
	if (hasBinary(dir) === false) {
		throw new Error(
			`Cannot publish ${pkg.name}@${pkg.version}: no binary found in ${
				join(dir, "bin")
			}. ` +
				"Run build-packages.mjs first to populate platform binaries.",
		);
	}

	if (packageExists(pkg.name, pkg.version)) {
		console.log(`Skipping ${pkg.name}@${pkg.version}; already published.`);
		return;
	}

	console.log(`Publishing ${pkg.name}@${pkg.version}...`);
	run("npm", ["publish", "--access", "public", "--provenance"], {
		cwd: dir,
		stdio: "inherit",
	});
}

export function main(argv = process.argv.slice(2)) {
	const args = parseArgs(argv);
	if (!args["packages-dir"]) {
		throw new Error("usage: publish-packages.mjs --packages-dir <dir>");
	}

	const packagesDir = resolve(args["packages-dir"]);

	for (const dirName of PLATFORM_PACKAGE_DIRS) {
		publishPackage(join(packagesDir, dirName));
	}

	publishPackage(join(packagesDir, CLI_PACKAGE_DIR));
}

if (
	process.argv[1] &&
	resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))
) {
	main();
}
