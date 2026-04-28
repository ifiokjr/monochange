#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const monitoredRoots = ["crates/monochange/src", "crates/monochange_config/src"];
const allowlist = new Set([
	"crates/monochange/src/hosted_sources.rs",
	"crates/monochange/src/release_artifacts.rs",
	"crates/monochange/src/release_branch_policy.rs",
	"crates/monochange/src/release_record.rs",
	"crates/monochange/src/versioned_files.rs",
	"crates/monochange/src/workspace_ops.rs",
	"crates/monochange/src/package_publish.rs",
	"crates/monochange_config/src/lib.rs",
]);
const forbiddenTokens = ["SourceProvider::", "EcosystemType::"];

function walk(relativeDir) {
	const fullDir = path.join(repoRoot, relativeDir);
	const entries = fs.readdirSync(fullDir, { withFileTypes: true });
	const files = [];

	for (const entry of entries) {
		const relativePath = path.join(relativeDir, entry.name);

		if (entry.isDirectory()) {
			files.push(...walk(relativePath));
			continue;
		}

		if (!entry.isFile() || !relativePath.endsWith(".rs")) {
			continue;
		}

		if (relativePath.includes("__tests") || relativePath.includes("/tests/")) {
			continue;
		}

		files.push(relativePath);
	}

	return files;
}

const violations = [];

for (const root of monitoredRoots) {
	for (const relativePath of walk(root)) {
		const contents = fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

		if (!forbiddenTokens.some((token) => contents.includes(token))) {
			continue;
		}

		if (allowlist.has(relativePath)) {
			continue;
		}

		violations.push(relativePath);
	}
}

if (violations.length > 0) {
	throw new Error(
		[
			"new provider/ecosystem dispatch points must be documented before landing:",
			...violations.map((value) => `- ${value}`),
			"update ARCHITECTURE.md and this allowlist if the exception is intentional.",
		].join("\n"),
	);
}

console.log("architecture boundary checks passed");
