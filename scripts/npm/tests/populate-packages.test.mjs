import assert from "node:assert/strict";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test, { afterEach, describe } from "node:test";
import {
	_resetSpawnSync,
	_setSpawnSync,
	CLI_PACKAGE_DIR,
	hasBinary,
	main as populateMain,
	packageMetadata,
	parseArgs,
	PLATFORM_PACKAGE_DIRS,
	run,
	assertTrustedPublishingContext,
} from "../populate-packages.mjs";

function makeSandbox() {
	const base = join(process.cwd(), ".tmp-test-populate-packages");
	const sandbox = join(base, `test-${Date.now()}-${Math.random().toString(36).slice(2)}`);
	mkdirSync(sandbox, { recursive: true });
	return sandbox;
}

function trustedPublishingEnv(overrides = {}) {
	return {
		GITHUB_ACTIONS: "true",
		GITHUB_REPOSITORY: "monochange/monochange",
		GITHUB_WORKFLOW_REF: "monochange/monochange/.github/workflows/publish.yml@refs/tags/v1.0.0",
		ACTIONS_ID_TOKEN_REQUEST_URL: "https://token.actions.example/request",
		ACTIONS_ID_TOKEN_REQUEST_TOKEN: "oidc-request-token",
		...overrides,
	};
}

afterEach(() => {
	_resetSpawnSync();
});

describe("parseArgs", () => {
	test("parses --packages-dir", () => {
		const result = parseArgs(["--packages-dir", "/tmp/pkg"]);
		assert.deepEqual(result, { "packages-dir": "/tmp/pkg" });
	});

	test("skips non-flag arguments", () => {
		const result = parseArgs(["positional", "--packages-dir", "/tmp/pkg"]);
		assert.deepEqual(result, { "packages-dir": "/tmp/pkg" });
	});

	test("handles empty argv", () => {
		const result = parseArgs([]);
		assert.deepEqual(result, {});
	});

	test("skips flag without value", () => {
		const result = parseArgs(["--packages-dir"]);
		assert.deepEqual(result, {});
	});

	test("takes next flag as value for previous flag if non-flag", () => {
		const result = parseArgs(["--packages-dir", "--other", "--key", "val"]);
		assert.deepEqual(result, { "packages-dir": "--other", key: "val" });
	});
});

describe("run", () => {
	test("returns result on success", () => {
		const result = run("echo", ["hello"]);
		assert.equal(result.status, 0);
		assert.match(result.stdout, /hello/);
	});

	test("throws on non-zero exit with stderr", () => {
		assert.throws(() => run("sh", ["-c", "echo err >&2; exit 1"]), { message: /failed/ });
	});

	test("throws on non-zero exit with stdout when stderr is empty", () => {
		assert.throws(() => run("sh", ["-c", "echo out; exit 1"]), { message: /failed/ });
	});

	test("handles null status", () => {
		_setSpawnSync(() => ({ status: null, stderr: "", stdout: "" }));
		assert.throws(() => run("noop", []), { message: /exit code unknown/ });
	});

	test("respects cwd option", () => {
		const sandbox = makeSandbox();
		const result = run("sh", ["-c", "pwd"], { cwd: sandbox });
		assert.match(result.stdout.trim(), new RegExp(sandbox.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
	});

	test("respects stdio option", () => {
		const result = run("echo", ["test"], { stdio: "pipe" });
		assert.equal(result.status, 0);
	});
});

describe("packageMetadata", () => {
	test("reads and parses package.json", () => {
		const sandbox = makeSandbox();
		const pkg = { name: "@monochange/test", version: "1.0.0" };
		writeFileSync(join(sandbox, "package.json"), JSON.stringify(pkg));
		const result = packageMetadata(sandbox);
		assert.deepEqual(result, pkg);
	});
});

describe("hasBinary", () => {
	test("returns false when bin directory does not exist", () => {
		const sandbox = makeSandbox();
		assert.equal(hasBinary(sandbox), false);
	});

	test("returns false when bin directory is empty", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		assert.equal(hasBinary(sandbox), false);
	});

	test("returns true when bin contains only monochange.js (launcher)", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		writeFileSync(join(sandbox, "bin", "monochange.js"), "");
		assert.equal(hasBinary(sandbox), true);
	});

	test("returns true when bin contains a native binary", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		writeFileSync(join(sandbox, "bin", "monochange"), "");
		assert.equal(hasBinary(sandbox), true);
	});

	test("returns true when bin contains a .exe binary", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		writeFileSync(join(sandbox, "bin", "monochange.exe"), "");
		assert.equal(hasBinary(sandbox), true);
	});

	test("returns true when bin contains both launcher and native binary", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		writeFileSync(join(sandbox, "bin", "monochange.js"), "");
		writeFileSync(join(sandbox, "bin", "monochange"), "");
		assert.equal(hasBinary(sandbox), true);
	});

	test("returns false when bin contains non-monochange files", () => {
		const sandbox = makeSandbox();
		mkdirSync(join(sandbox, "bin"));
		writeFileSync(join(sandbox, "bin", "other.txt"), "");
		assert.equal(hasBinary(sandbox), false);
	});
});

describe("trusted publishing context", () => {
	test("accepts the monochange publish workflow OIDC context", () => {
		assert.doesNotThrow(() => assertTrustedPublishingContext(trustedPublishingEnv()));
	});

	test("rejects long-lived npm token environment variables", () => {
		assert.throws(
			() => assertTrustedPublishingContext(trustedPublishingEnv({ NODE_AUTH_TOKEN: "secret" })),
			{
				message: /long-lived npm token environment variables: NODE_AUTH_TOKEN/,
			},
		);
	});

	test("rejects missing GitHub OIDC context", () => {
		assert.throws(() => assertTrustedPublishingContext({}), {
			message: /Cannot publish npm packages without the trusted-publishing GitHub Actions context/,
		});
	});
});

describe("main", () => {
	test("throws when --packages-dir is missing", () => {
		assert.throws(() => populateMain([]), { message: /usage:/ });
	});

	test("populates all platform packages then the cli package", () => {
		const sandbox = makeSandbox();

		for (const dirName of [...PLATFORM_PACKAGE_DIRS, CLI_PACKAGE_DIR]) {
			const pkgDir = join(sandbox, dirName);
			mkdirSync(join(pkgDir, "bin"), { recursive: true });
			const binaryName = dirName === CLI_PACKAGE_DIR ? "monochange.js" : "monochange";
			writeFileSync(join(pkgDir, "bin", binaryName), "");
			writeFileSync(
				join(pkgDir, "package.json"),
				JSON.stringify({
					name: `@monochange/${dirName.replace("monochange__", "").replace("__", "/")}`,
					version: "1.0.0",
				}),
			);
		}

		populateMain(["--packages-dir", sandbox]);
	});
});

describe("PLATFORM_PACKAGE_DIRS", () => {
	test("contains all 8 platform directories", () => {
		assert.equal(PLATFORM_PACKAGE_DIRS.length, 8);
	});

	test("includes darwin arm64", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-darwin-arm64"));
	});

	test("includes darwin x64", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-darwin-x64"));
	});

	test("includes linux arm64 gnu", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-linux-arm64-gnu"));
	});

	test("includes linux arm64 musl", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-linux-arm64-musl"));
	});

	test("includes linux x64 gnu", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-linux-x64-gnu"));
	});

	test("includes linux x64 musl", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-linux-x64-musl"));
	});

	test("includes win32 x64 msvc", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-win32-x64-msvc"));
	});

	test("includes win32 arm64 msvc", () => {
		assert.ok(PLATFORM_PACKAGE_DIRS.includes("monochange__cli-win32-arm64-msvc"));
	});
});

describe("CLI_PACKAGE_DIR", () => {
	test("is monochange__cli", () => {
		assert.equal(CLI_PACKAGE_DIR, "monochange__cli");
	});
});
