#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();

function read(relativePath) {
	return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function extractBlock(contents, startMarker, endMarker) {
	const start = contents.indexOf(startMarker);
	const end = contents.indexOf(endMarker);

	if (start === -1 || end === -1 || end <= start) {
		throw new Error(`unable to find block ${startMarker} … ${endMarker}`);
	}

	return contents.slice(start + startMarker.length, end);
}

function extractToolNamesFromTemplate(contents) {
	const block = extractBlock(
		contents,
		"<!-- {@mcpToolsList} -->",
		"<!-- {/mcpToolsList} -->",
	);

	return new Set(
		[...block.matchAll(/`(monochange_[^`]+)`/g)].map((match) => match[1]),
	);
}

function extractToolNamesFromServer(contents) {
	return new Set(
		[...contents.matchAll(/name = "(monochange_[^"]+)"/g)].map((match) =>
			match[1]
		),
	);
}

function assertSetEquals(label, actual, expected) {
	const actualValues = [...actual].sort();
	const expectedValues = [...expected].sort();

	if (JSON.stringify(actualValues) === JSON.stringify(expectedValues)) {
		return;
	}

	const missing = expectedValues.filter((value) => !actual.has(value));
	const extra = actualValues.filter((value) => !expected.has(value));

	const details = [
		missing.length ? `missing: ${missing.join(", ")}` : null,
		extra.length ? `extra: ${extra.join(", ")}` : null,
	]
		.filter(Boolean)
		.join("; ");

	throw new Error(`${label} drift detected (${details})`);
}

function assertContains(relativePath, needle) {
	const contents = read(relativePath);

	if (!contents.includes(needle)) {
		throw new Error(`${relativePath} is missing required reference: ${needle}`);
	}
}

function assertExists(relativePath) {
	const fullPath = path.join(repoRoot, relativePath);

	if (!fs.existsSync(fullPath)) {
		throw new Error(`missing required file: ${relativePath}`);
	}
}

const templateTools = extractToolNamesFromTemplate(
	read(".templates/guides.t.md"),
);
const serverTools = extractToolNamesFromServer(
	read("crates/monochange/src/mcp.rs"),
);
assertSetEquals("MCP tool list", templateTools, serverTools);

assertExists("ARCHITECTURE.md");
assertExists("docs/plans/README.md");
assertExists("docs/plans/active/harness-engineering.md");

assertContains("AGENTS.md", "docs/agents/architecture.md");
assertContains("AGENTS.md", "docs/plans/README.md");
assertContains("ARCHITECTURE.md", "docs/plans/README.md");
assertContains("ARCHITECTURE.md", "crates/monochange_core");
assertContains("docs/plans/README.md", "docs/plans/active/");
assertContains(".templates/guides.t.md", "mc diagnostics --format json");
assertContains(".templates/guides.t.md", "mc release --dry-run --format json");

console.log("agent-facing documentation checks passed");
