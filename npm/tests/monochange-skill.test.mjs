import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const skillPath = join(process.cwd(), "npm/skill/bin/monochange-skill.js");

function run(args = []) {
	return spawnSync("node", [skillPath, ...args], {
		encoding: "utf8",
	});
}

test("monochange-skill prints install guidance", () => {
	const result = run(["--print-install"]);
	assert.equal(result.status, 0);
	assert.match(result.stdout, /@monochange\/cli/);
	assert.match(result.stdout, /@monochange\/skill/);
	assert.match(result.stdout, /monochange mcp/);
});

test("monochange-skill copies bundled files", () => {
	const targetDir = mkdtempSync(join(tmpdir(), "monochange-skill-"));
	const result = run(["--copy", targetDir]);
	assert.equal(result.status, 0);
	assert.ok(existsSync(join(targetDir, "SKILL.md")));
	assert.ok(existsSync(join(targetDir, "REFERENCE.md")));
	assert.ok(existsSync(join(targetDir, "README.md")));
});
