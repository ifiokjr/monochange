#!/usr/bin/env node

import {
	copyFileSync,
	cpSync,
	mkdirSync,
	readFileSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");

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

function main() {
	const args = parseArgs(process.argv.slice(2));
	const version = args.version;
	const outDir = resolve(args["out-dir"] ?? "");
	if (!version || !args["out-dir"]) {
		throw new Error(
			"usage: prepare-skill-package.mjs --version <x.y.z> --out-dir <dir>",
		);
	}

	mkdirSync(outDir, { recursive: true });
	cpSync(join(repoRoot, "npm/skill"), outDir, { recursive: true });
	copyFileSync(
		join(repoRoot, "skills/monochange/SKILL.md"),
		join(outDir, "SKILL.md"),
	);
	copyFileSync(
		join(repoRoot, "skills/monochange/REFERENCE.md"),
		join(outDir, "REFERENCE.md"),
	);

	const packageJsonPath = join(outDir, "package.json");
	const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
	packageJson.version = version;
	writeFileSync(packageJsonPath, `${JSON.stringify(packageJson, null, 2)}\n`);

	console.log(`Prepared @monochange/skill@${version} in ${outDir}`);
}

main();
