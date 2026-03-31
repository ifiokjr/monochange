#!/usr/bin/env node
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const files = ["SKILL.md", "REFERENCE.md", "README.md"];

function usage() {
	console.log(`monochange-skill

Usage:
  monochange-skill --print-install
  monochange-skill --print-skill
  monochange-skill --print-reference
  monochange-skill --copy <target-dir>
`);
}

function printInstall() {
	console.log(
		`Install the Monochange CLI:\n  npm install -g @monochange/cli\n\nInstall the bundled skill package:\n  npm install -g @monochange/skill\n\nCopy the skill into an agent skill directory:\n  monochange-skill --copy ~/.pi/agent/skills/monochange\n\nStart the MCP server manually:\n  monochange mcp\n\nPlanned MCP config:\n{\n  \"mcpServers\": {\n    \"monochange\": {\n      \"command\": \"monochange\",\n      \"args\": [\"mcp\"]\n    }\n  }\n}`,
	);
}

function printFile(name) {
	process.stdout.write(fs.readFileSync(path.join(root, name), "utf8"));
}

function copyFiles(targetDir) {
	if (!targetDir) {
		throw new Error("--copy requires a target directory");
	}

	fs.mkdirSync(targetDir, { recursive: true });
	for (const file of files) {
		fs.copyFileSync(path.join(root, file), path.join(targetDir, file));
	}
	console.log(`Copied skill files to ${targetDir}`);
}

function main(argv) {
	const [flag, value] = argv;
	switch (flag) {
		case "--print-install":
			printInstall();
			return;
		case "--print-skill":
			printFile("SKILL.md");
			return;
		case "--print-reference":
			printFile("REFERENCE.md");
			return;
		case "--copy":
			copyFiles(value);
			return;
		case "--help":
		case "-h":
		case undefined:
			usage();
			return;
		default:
			throw new Error(`unknown argument: ${flag}`);
	}
}

try {
	main(process.argv.slice(2));
} catch (error) {
	console.error(error instanceof Error ? error.message : String(error));
	process.exit(1);
}
