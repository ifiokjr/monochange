#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import {
	chmodSync,
	copyFileSync,
	existsSync,
	mkdirSync,
	readdirSync,
	writeFileSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");

const platforms = [
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "arm64",
		label: "Linux arm64 (glibc)",
		libc: "glibc",
		os: "linux",
		packageName: "@monochange/cli-linux-arm64-gnu",
		target: "aarch64-unknown-linux-gnu",
	},
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "arm64",
		label: "Linux arm64 (musl)",
		libc: "musl",
		os: "linux",
		packageName: "@monochange/cli-linux-arm64-musl",
		target: "aarch64-unknown-linux-musl",
	},
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "arm64",
		label: "macOS arm64",
		os: "darwin",
		packageName: "@monochange/cli-darwin-arm64",
		target: "aarch64-apple-darwin",
	},
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "x64",
		label: "Linux x64 (glibc)",
		libc: "glibc",
		os: "linux",
		packageName: "@monochange/cli-linux-x64-gnu",
		target: "x86_64-unknown-linux-gnu",
	},
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "x64",
		label: "Linux x64 (musl)",
		libc: "musl",
		os: "linux",
		packageName: "@monochange/cli-linux-x64-musl",
		target: "x86_64-unknown-linux-musl",
	},
	{
		archiveExt: "tar.gz",
		binaryName: "monochange",
		cpu: "x64",
		label: "macOS x64",
		os: "darwin",
		packageName: "@monochange/cli-darwin-x64",
		target: "x86_64-apple-darwin",
	},
	{
		archiveExt: "zip",
		binaryName: "monochange.exe",
		cpu: "x64",
		label: "Windows x64",
		os: "win32",
		packageName: "@monochange/cli-win32-x64-msvc",
		target: "x86_64-pc-windows-msvc",
	},
	{
		archiveExt: "zip",
		binaryName: "monochange.exe",
		cpu: "arm64",
		label: "Windows arm64",
		os: "win32",
		packageName: "@monochange/cli-win32-arm64-msvc",
		target: "aarch64-pc-windows-msvc",
	},
];

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

function ensureDirectory(path) {
	mkdirSync(path, { recursive: true });
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

function findArchive(assetsDir, target, releaseTag, archiveExt) {
	const archiveName = `monochange-${target}-${releaseTag}.${archiveExt}`;
	const archivePath = join(assetsDir, archiveName);
	if (!existsSync(archivePath)) {
		throw new Error(`missing release asset: ${archiveName}`);
	}
	return archivePath;
}

function* walk(dir) {
	const entries = readdirSync(dir, { withFileTypes: true });
	for (const entry of entries) {
		const entryPath = join(dir, entry.name);
		if (entry.isDirectory()) {
			yield* walk(entryPath);
		} else {
			yield entryPath;
		}
	}
}

function extractArchive(archivePath, destinationDir) {
	ensureDirectory(destinationDir);
	if (archivePath.endsWith(".zip")) {
		run("unzip", ["-q", archivePath, "-d", destinationDir]);
		return;
	}
	if (archivePath.endsWith(".tar.gz")) {
		run("tar", ["-xzf", archivePath, "-C", destinationDir]);
		return;
	}
	throw new Error(`unsupported archive: ${basename(archivePath)}`);
}

function findBinary(extractedDir, binaryName) {
	for (const filePath of walk(extractedDir)) {
		if (basename(filePath) === binaryName) {
			return filePath;
		}
	}
	throw new Error(`could not find ${binaryName} in ${extractedDir}`);
}

function writeJson(path, value) {
	writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

function createPlatformPackage(
	{ outDir, spec, version, releaseTag, assetsDir },
) {
	const archivePath = findArchive(
		assetsDir,
		spec.target,
		releaseTag,
		spec.archiveExt,
	);
	const extractedDir = join(outDir, ".tmp", spec.target);
	const packageDir = join(
		outDir,
		"platform",
		spec.packageName.replace("/", "__"),
	);
	const binDir = join(packageDir, "bin");

	extractArchive(archivePath, extractedDir);
	const binaryPath = findBinary(extractedDir, spec.binaryName);

	ensureDirectory(binDir);
	copyFileSync(binaryPath, join(binDir, spec.binaryName));
	if (spec.binaryName === "monochange") {
		chmodSync(join(binDir, spec.binaryName), 0o755);
	}
	copyFileSync(join(repoRoot, "license"), join(packageDir, "LICENSE"));

	const packageJson = {
		name: spec.packageName,
		version,
		description: `Prebuilt monochange binary for ${spec.label}`,
		license: "Unlicense",
		repository: {
			type: "git",
			url: "git+https://github.com/ifiokjr/monochange.git",
		},
		homepage: "https://github.com/ifiokjr/monochange",
		bugs: {
			url: "https://github.com/ifiokjr/monochange/issues",
		},
		os: [spec.os],
		cpu: [spec.cpu],
		files: ["bin", "LICENSE"],
		publishConfig: {
			access: "public",
			provenance: true,
		},
	};
	if (spec.libc) {
		packageJson.libc = [spec.libc];
	}

	writeJson(join(packageDir, "package.json"), packageJson);
}

function createRootPackage({ outDir, version }) {
	const packageDir = join(outDir, "root");
	const binDir = join(packageDir, "bin");
	ensureDirectory(binDir);
	copyFileSync(
		join(repoRoot, "npm/bin/monochange.js"),
		join(binDir, "monochange.js"),
	);
	chmodSync(join(binDir, "monochange.js"), 0o755);
	copyFileSync(join(repoRoot, "readme.md"), join(packageDir, "README.md"));
	copyFileSync(join(repoRoot, "license"), join(packageDir, "LICENSE"));

	const optionalDependencies = Object.fromEntries(
		platforms.map((spec) => [spec.packageName, version]),
	);

	writeJson(join(packageDir, "package.json"), {
		name: "@monochange/cli",
		version,
		description:
			"CLI for cross-ecosystem monorepo release planning with monochange",
		license: "Unlicense",
		repository: {
			type: "git",
			url: "git+https://github.com/ifiokjr/monochange.git",
		},
		homepage: "https://github.com/ifiokjr/monochange",
		bugs: {
			url: "https://github.com/ifiokjr/monochange/issues",
		},
		keywords: [
			"monochange",
			"releases",
			"changesets",
			"cli",
			"mcp",
			"monorepo",
		],
		engines: {
			node: ">=18",
		},
		bin: {
			monochange: "bin/monochange.js",
			mc: "bin/monochange.js",
		},
		files: ["bin", "README.md", "LICENSE"],
		optionalDependencies,
		publishConfig: {
			access: "public",
			provenance: true,
		},
	});
}

function main() {
	const args = parseArgs(process.argv.slice(2));
	const version = args.version;
	const releaseTag = args["release-tag"];
	const assetsDir = resolve(args["assets-dir"] ?? "");
	const outDir = resolve(args["out-dir"] ?? "");

	if (!version || !releaseTag || !args["assets-dir"] || !args["out-dir"]) {
		throw new Error(
			"usage: build-packages.mjs --version <x.y.z> --release-tag <vX.Y.Z> --assets-dir <dir> --out-dir <dir>",
		);
	}

	ensureDirectory(outDir);
	for (const spec of platforms) {
		createPlatformPackage({ outDir, spec, version, releaseTag, assetsDir });
	}
	createRootPackage({ outDir, version });

	const summary = {
		platformPackages: platforms.map((spec) => spec.packageName),
		rootPackage: "@monochange/cli",
		version,
	};
	writeFileSync(
		join(outDir, "summary.json"),
		`${JSON.stringify(summary, null, 2)}\n`,
	);
	console.log(`Generated npm packages for ${version} in ${outDir}`);
}

main();
