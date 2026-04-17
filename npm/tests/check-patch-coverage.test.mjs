import assert from "node:assert/strict";
import test, { describe } from "node:test";
import {
	computePatchCoverage,
	parseArgs,
	parseChangedLines,
	parseLcov,
	verifyPatchCoverage,
} from "../../scripts/check-patch-coverage.mjs";

const repoRoot = "/repo";

describe("parseArgs", () => {
	test("parses supported flags", () => {
		assert.deepEqual(
			parseArgs([
				"--base",
				"origin/main",
				"--head",
				"HEAD",
				"--lcov",
				"target/coverage/lcov.info",
			]),
			{
				base: "origin/main",
				head: "HEAD",
				lcov: "target/coverage/lcov.info",
			},
		);
	});

	test("skips tokens without values", () => {
		assert.deepEqual(parseArgs(["--base", "--head", "HEAD"]), {
			head: "HEAD",
		});
	});
});

describe("parseLcov", () => {
	test("collects per-line hit counts by source file", () => {
		const coverageByFile = parseLcov(
			[
				"TN:",
				"SF:crates/monochange/src/lib.rs",
				"DA:10,3",
				"DA:11,0",
				"end_of_record",
			].join("\n"),
			repoRoot,
		);

		assert.equal(
			coverageByFile.get("/repo/crates/monochange/src/lib.rs").get(10),
			3,
		);
		assert.equal(
			coverageByFile.get("/repo/crates/monochange/src/lib.rs").get(11),
			0,
		);
	});
});

describe("parseChangedLines", () => {
	test("collects added line numbers from zero-context unified diffs", () => {
		const changedLinesByFile = parseChangedLines(
			[
				"diff --git a/crates/monochange/src/lib.rs b/crates/monochange/src/lib.rs",
				"--- a/crates/monochange/src/lib.rs",
				"+++ b/crates/monochange/src/lib.rs",
				"@@ -10,0 +11,2 @@",
				"+first",
				"+second",
				"@@ -20 +25 @@",
				"+replacement",
			].join("\n"),
			repoRoot,
		);

		assert.deepEqual(
			[...changedLinesByFile.get("/repo/crates/monochange/src/lib.rs")],
			[11, 12, 25],
		);
	});

	test("ignores deletion-only hunks", () => {
		const changedLinesByFile = parseChangedLines(
			[
				"diff --git a/crates/monochange/src/lib.rs b/crates/monochange/src/lib.rs",
				"--- a/crates/monochange/src/lib.rs",
				"+++ b/crates/monochange/src/lib.rs",
				"@@ -10,2 +10,0 @@",
				"-first",
				"-second",
			].join("\n"),
			repoRoot,
		);

		assert.equal(changedLinesByFile.size, 0);
	});
});

describe("computePatchCoverage", () => {
	test("counts only executable changed lines from the coverage report", () => {
		const coverageByFile = parseLcov(
			[
				"SF:crates/monochange/src/lib.rs",
				"DA:10,1",
				"DA:11,0",
				"DA:12,4",
				"end_of_record",
			].join("\n"),
			repoRoot,
		);
		const changedLinesByFile = new Map([
			[
				"/repo/crates/monochange/src/lib.rs",
				new Set([9, 10, 11, 12, 13]),
			],
		]);

		const result = computePatchCoverage(coverageByFile, changedLinesByFile);
		assert.equal(result.coveredLines, 2);
		assert.equal(result.executableChangedLines, 3);
		assert.equal(result.uncoveredLines.length, 1);
		assert.deepEqual(result.uncoveredLines[0], {
			filePath: "/repo/crates/monochange/src/lib.rs",
			lineNumber: 11,
		});
	});
});

describe("verifyPatchCoverage", () => {
	test("passes when every executable changed line is covered", () => {
		const result = verifyPatchCoverage({
			lcovText: [
				"SF:crates/monochange/src/lib.rs",
				"DA:10,1",
				"DA:11,7",
				"end_of_record",
			].join("\n"),
			diffText: [
				"diff --git a/crates/monochange/src/lib.rs b/crates/monochange/src/lib.rs",
				"--- a/crates/monochange/src/lib.rs",
				"+++ b/crates/monochange/src/lib.rs",
				"@@ -9,0 +10,2 @@",
				"+covered",
				"+also covered",
			].join("\n"),
			repoRoot,
			target: 100,
		});

		assert.equal(result.passed, true);
		assert.equal(result.coveredLines, 2);
		assert.equal(result.executableChangedLines, 2);
		assert.match(result.summary, /PATCH_COVERAGE 2\/2 \(100\.00%\)/);
	});

	test("fails when patch coverage drops below 100%", () => {
		const result = verifyPatchCoverage({
			lcovText: [
				"SF:crates/monochange/src/lib.rs",
				"DA:10,1",
				"DA:11,0",
				"end_of_record",
			].join("\n"),
			diffText: [
				"diff --git a/crates/monochange/src/lib.rs b/crates/monochange/src/lib.rs",
				"--- a/crates/monochange/src/lib.rs",
				"+++ b/crates/monochange/src/lib.rs",
				"@@ -9,0 +10,2 @@",
				"+covered",
				"+missed",
			].join("\n"),
			repoRoot,
			target: 100,
		});

		assert.equal(result.passed, false);
		assert.match(result.summary, /Required patch coverage: 100\.00%/);
		assert.match(result.summary, /crates\/monochange\/src\/lib\.rs:11/);
	});

	test("treats diffs without executable changed lines as passing", () => {
		const result = verifyPatchCoverage({
			lcovText: [
				"SF:crates/monochange/src/lib.rs",
				"DA:10,1",
				"end_of_record",
			].join("\n"),
			diffText: [
				"diff --git a/docs/readme.md b/docs/readme.md",
				"--- a/docs/readme.md",
				"+++ b/docs/readme.md",
				"@@ -1,0 +1 @@",
				"+docs only",
			].join("\n"),
			repoRoot,
			target: 100,
		});

		assert.equal(result.passed, true);
		assert.equal(result.executableChangedLines, 0);
		assert.match(result.summary, /No executable changed lines were found/);
	});
});
