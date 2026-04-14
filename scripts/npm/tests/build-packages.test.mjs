import assert from "node:assert/strict";
import { execSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test, { afterEach, describe } from "node:test";
import {
	_resetSpawnSync,
	_setSpawnSync,
	ensureDirectory,
	extractArchive,
	findArchive,
	findBinary,
	main as buildMain,
	packageNameToDirName,
	parseArgs,
	populatePlatformPackage,
	run,
	walk,
} from "../build-packages.mjs";

function makeSandbox() {
	const base = join(process.cwd(), ".tmp-test-build-packages");
	const sandbox = join(
		base,
		`test-${Date.now()}-${Math.random().toString(36).slice(2)}`,
	);
	mkdirSync(sandbox, { recursive: true });
	return sandbox;
}

afterEach(() => {
	_resetSpawnSync();
});

describe("parseArgs", () => {
	test("parses --release-tag and --assets-dir", () => {
		const result = parseArgs([
			"--release-tag",
			"v1.0.0",
			"--assets-dir",
			"/tmp",
		]);
		assert.deepEqual(result, { "release-tag": "v1.0.0", "assets-dir": "/tmp" });
	});

	test("skips non-flag arguments", () => {
		const result = parseArgs(["positional", "--release-tag", "v1.0.0"]);
		assert.deepEqual(result, { "release-tag": "v1.0.0" });
	});

	test("skips flags without values", () => {
		const result = parseArgs(["--release-tag"]);
		assert.deepEqual(result, {});
	});

	test("handles empty argv", () => {
		const result = parseArgs([]);
		assert.deepEqual(result, {});
	});

	test("takes next flag as value for previous flag", () => {
		const result = parseArgs(["--release-tag", "--assets-dir", "/tmp"]);
		assert.deepEqual(result, { "release-tag": "--assets-dir" });
	});
});

describe("ensureDirectory", () => {
	test("creates nested directories", () => {
		const dir = join(makeSandbox(), "a", "b", "c");
		ensureDirectory(dir);
		assert.ok(existsSync(dir));
	});

	test("succeeds on existing directory", () => {
		const dir = makeSandbox();
		ensureDirectory(dir);
		assert.ok(existsSync(dir));
	});
});

describe("run", () => {
	test("returns result on success", () => {
		const result = run("echo", ["hello"]);
		assert.equal(result.status, 0);
		assert.match(result.stdout, /hello/);
	});

	test("throws on non-zero exit with stderr", () => {
		assert.throws(
			() => run("sh", ["-c", "echo err >&2; exit 1"]),
			{ message: /failed/ },
		);
	});

	test("throws on non-zero exit with stdout when stderr is empty", () => {
		assert.throws(
			() => run("sh", ["-c", "echo out; exit 1"]),
			{ message: /failed/ },
		);
	});

	test("handles null status", () => {
		_setSpawnSync(() => ({ status: null, stderr: "", stdout: "" }));
		assert.throws(
			() => run("noop", []),
			{ message: /exit code unknown/ },
		);
	});

	test("respects cwd option", () => {
		const sandbox = makeSandbox();
		const result = run("sh", ["-c", "pwd"], { cwd: sandbox });
		assert.match(
			result.stdout.trim(),
			new RegExp(sandbox.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
		);
	});

	test("respects stdio option", () => {
		const result = run("echo", ["test"], { stdio: "pipe" });
		assert.equal(result.status, 0);
	});
});

describe("findArchive", () => {
	test("returns archive path when file exists", () => {
		const sandbox = makeSandbox();
		const archivePath = join(
			sandbox,
			"monochange-x86_64-apple-darwin-v1.0.0.tar.gz",
		);
		writeFileSync(archivePath, "");
		const result = findArchive(
			sandbox,
			"x86_64-apple-darwin",
			"v1.0.0",
			"tar.gz",
		);
		assert.equal(result, archivePath);
	});

	test("throws when archive is missing", () => {
		const sandbox = makeSandbox();
		assert.throws(
			() => findArchive(sandbox, "x86_64-apple-darwin", "v1.0.0", "tar.gz"),
			{ message: /missing release asset/ },
		);
	});
});

describe("walk", () => {
	test("walks nested directories yielding file paths", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "sub"), { recursive: true });
		writeFileSync(join(sandbox, "a.txt"), "");
		writeFileSync(join(sandbox, "sub", "b.txt"), "");

		const files = [...walk(sandbox)];
		assert.ok(files.some((f) => f.endsWith("a.txt")));
		assert.ok(files.some((f) => f.endsWith("b.txt")));
		assert.equal(files.length, 2);
	});

	test("yields nothing for empty directory", () => {
		const sandbox = makeSandbox();
		const files = [...walk(sandbox)];
		assert.equal(files.length, 0);
	});
});

describe("extractArchive", () => {
	test("throws for unsupported archive type", () => {
		const sandbox = makeSandbox();
		const outDir = join(sandbox, "out");
		assert.throws(
			() => extractArchive(join(sandbox, "file.rar"), outDir),
			{ message: /unsupported archive/ },
		);
	});

	test("extracts .tar.gz archives", () => {
		const sandbox = makeSandbox();
		const srcDir = join(sandbox, "src");
		mkdirSync(srcDir);
		writeFileSync(join(srcDir, "monochange"), "#!/bin/sh\necho hi\n");
		chmodSync(join(srcDir, "monochange"), 0o755);
		execSync(`tar -czf ${join(sandbox, "archive.tar.gz")} -C ${sandbox} src`);

		const outDir = join(sandbox, "out");
		extractArchive(join(sandbox, "archive.tar.gz"), outDir);
		assert.ok(existsSync(join(outDir, "src", "monochange")));
	});

	test("extracts .zip archives", () => {
		const sandbox = makeSandbox();
		const srcDir = join(sandbox, "src");
		mkdirSync(srcDir);
		writeFileSync(join(srcDir, "monochange.exe"), "binary");
		execSync(`cd ${sandbox} && zip -q archive.zip src/monochange.exe`);

		const outDir = join(sandbox, "out");
		extractArchive(join(sandbox, "archive.zip"), outDir);
		assert.ok(existsSync(join(outDir, "src", "monochange.exe")));
	});
});

describe("findBinary", () => {
	test("finds binary in a flat directory", () => {
		const sandbox = makeSandbox();
		writeFileSync(join(sandbox, "monochange"), "");
		const result = findBinary(sandbox, "monochange");
		assert.equal(result, join(sandbox, "monochange"));
	});

	test("finds binary in nested directory", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "sub"), { recursive: true });
		writeFileSync(join(sandbox, "sub", "monochange.exe"), "");
		const result = findBinary(sandbox, "monochange.exe");
		assert.equal(result, join(sandbox, "sub", "monochange.exe"));
	});

	test("throws when binary not found", () => {
		const sandbox = makeSandbox();
		assert.throws(
			() => findBinary(sandbox, "monochange"),
			{ message: /could not find/ },
		);
	});
});

describe("packageNameToDirName", () => {
	test("converts scoped package name to directory name", () => {
		assert.equal(packageNameToDirName("@monochange/cli"), "monochange__cli");
	});

	test("converts scoped package with platform suffix", () => {
		assert.equal(
			packageNameToDirName("@monochange/cli-darwin-arm64"),
			"monochange__cli-darwin-arm64",
		);
	});

	test("passes through unscoped names", () => {
		assert.equal(packageNameToDirName("monochange"), "monochange");
	});
});

describe("populatePlatformPackage", () => {
	test("populates binary for a .tar.gz platform without modifying package.json", () => {
		const sandbox = makeSandbox();
		const pkgDir = join(sandbox, "monochange__cli-darwin-arm64");
		mkdirSync(join(pkgDir, "bin"), { recursive: true });
		writeFileSync(
			join(pkgDir, "package.json"),
			JSON.stringify({
				name: "@monochange/cli-darwin-arm64",
				version: "0.0.0",
			}),
		);

		const assetsDir = join(sandbox, "assets");
		mkdirSync(assetsDir);
		const srcBinDir = join(assetsDir, "src");
		mkdirSync(srcBinDir);
		writeFileSync(join(srcBinDir, "monochange"), "#!/bin/sh\necho hi\n");
		chmodSync(join(srcBinDir, "monochange"), 0o755);
		execSync(
			`tar -czf ${
				join(assetsDir, "monochange-aarch64-apple-darwin-v1.2.3.tar.gz")
			} -C ${assetsDir} src`,
		);

		populatePlatformPackage({
			packagesDir: sandbox,
			spec: {
				archiveExt: "tar.gz",
				binaryName: "monochange",
				packageName: "@monochange/cli-darwin-arm64",
				target: "aarch64-apple-darwin",
			},
			releaseTag: "v1.2.3",
			assetsDir,
			tmpDir: join(sandbox, ".tmp"),
		});

		assert.ok(existsSync(join(pkgDir, "bin", "monochange")));
	});

	test("populates .exe binary for windows platform", () => {
		const sandbox = makeSandbox();
		const pkgDir = join(sandbox, "monochange__cli-win32-x64-msvc");
		mkdirSync(join(pkgDir, "bin"), { recursive: true });
		writeFileSync(
			join(pkgDir, "package.json"),
			JSON.stringify({
				name: "@monochange/cli-win32-x64-msvc",
				version: "0.0.0",
			}),
		);

		const assetsDir = join(sandbox, "assets");
		mkdirSync(assetsDir);
		const srcBinDir = join(assetsDir, "src");
		mkdirSync(srcBinDir);
		writeFileSync(join(srcBinDir, "monochange.exe"), "binary");
		execSync(
			`cd ${assetsDir} && zip -q monochange-x86_64-pc-windows-msvc-v2.0.0.zip src/monochange.exe`,
		);

		populatePlatformPackage({
			packagesDir: sandbox,
			spec: {
				archiveExt: "zip",
				binaryName: "monochange.exe",
				packageName: "@monochange/cli-win32-x64-msvc",
				target: "x86_64-pc-windows-msvc",
			},
			releaseTag: "v2.0.0",
			assetsDir,
			tmpDir: join(sandbox, ".tmp"),
		});

		assert.ok(existsSync(join(pkgDir, "bin", "monochange.exe")));
	});
});

describe("main", () => {
	test("throws when required arguments are missing", () => {
		assert.throws(
			() => buildMain([]),
			{ message: /usage:/ },
		);
	});

	test("throws when --assets-dir is missing", () => {
		assert.throws(
			() => buildMain(["--release-tag", "v1.0.0"]),
			{ message: /usage:/ },
		);
	});

	test("throws when --release-tag is missing", () => {
		assert.throws(
			() => buildMain(["--assets-dir", "/tmp"]),
			{ message: /usage:/ },
		);
	});
});
