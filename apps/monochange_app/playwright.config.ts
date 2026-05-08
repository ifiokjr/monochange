import { defineConfig, devices } from "@playwright/test";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
const artifactsDir =
	process.env.MONOCHANGE_APP_ARTIFACTS_DIR ?? join(tmpdir(), "monochange-app-playwright");
const baseURL = process.env.MONOCHANGE_APP_BASE_URL ?? "http://127.0.0.1:3000";
const browserChannel =
	process.env.PLAYWRIGHT_BROWSER_CHANNEL ?? (process.env.CI ? undefined : "chrome");
const startLocalServer = process.env.MONOCHANGE_APP_SKIP_WEB_SERVER !== "1";

export default defineConfig({
	testDir: "./tests/e2e",
	outputDir: join(artifactsDir, "test-results"),
	fullyParallel: true,
	forbidOnly: Boolean(process.env.CI),
	retries: process.env.CI ? 2 : 0,
	workers: process.env.CI ? 1 : undefined,
	reporter: [
		["list"],
		["html", { open: "never", outputFolder: join(artifactsDir, "playwright-report") }],
	],
	use: {
		baseURL,
		trace: "retain-on-failure",
	},
	projects: [
		{
			name: "chromium",
			use: { ...devices["Desktop Chrome"], channel: browserChannel },
		},
	],
	webServer: startLocalServer
		? {
				command: process.env.CI
					? "$HOME/.cargo/bin/cargo-leptos serve"
					: "bash -lc 'set -e; devenv processes list 2>/dev/null | grep -q \"^postgres[[:space:]]\" || devenv up -d postgres; exec devenv shell cargo leptos --manifest-path apps/monochange_app/crates/monochange_app/Cargo.toml serve'",
				cwd: process.env.CI ? join(repoRoot, "apps/monochange_app") : repoRoot,
				reuseExistingServer: !process.env.CI,
				timeout: 300_000,
				url: baseURL,
			}
		: undefined,
});
