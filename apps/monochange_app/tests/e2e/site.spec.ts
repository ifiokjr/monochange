import { expect, test, type Page, type TestInfo } from "@playwright/test";

const colorModes = ["light", "dark"] as const;

for (const colorMode of colorModes) {
	test(`home page renders in ${colorMode} mode`, async ({ page }, testInfo) => {
		const browserErrors = await preparePage(page, colorMode);

		await page.goto("/");

		await expect(page.getByRole("heading", { name: /Release planning/i })).toBeVisible();
		await expect(page.getByRole("link", { name: /Sign in with GitHub/i })).toBeVisible();
		await expect(page.locator("html")).toHaveClass(new RegExp(`\\b${colorMode}\\b`));

		await saveScreenshot(page, testInfo, `home-${colorMode}`);
		expect(browserErrors).toEqual([]);
	});

	test(`login page renders in ${colorMode} mode`, async ({ page }, testInfo) => {
		const browserErrors = await preparePage(page, colorMode);

		await page.goto("/login");

		await expect(page.getByRole("heading", { name: /Sign in to monochange/i })).toBeVisible();
		await expect(page.getByText(/Connect your GitHub account/i)).toBeVisible();
		await expect(page.locator("html")).toHaveClass(new RegExp(`\\b${colorMode}\\b`));

		await saveScreenshot(page, testInfo, `login-${colorMode}`);
		expect(browserErrors).toEqual([]);
	});
}

async function preparePage(page: Page, colorMode: (typeof colorModes)[number]) {
	const browserErrors: string[] = [];

	page.on("console", (message) => {
		if (message.type() === "error") {
			browserErrors.push(message.text());
		}
	});
	page.on("pageerror", (error) => browserErrors.push(error.message));

	await page.route(/https:\/\/fonts\.(?:googleapis|gstatic)\.com\/.*/, (route) =>
		route.fulfill({ body: "", status: 204 }),
	);
	await page.setViewportSize({ height: 1200, width: 1440 });
	await page.emulateMedia({ colorScheme: colorMode });
	await page.addInitScript((mode) => {
		window.localStorage.setItem("monochange-color-mode", mode);
	}, colorMode);

	return browserErrors;
}

async function saveScreenshot(page: Page, testInfo: TestInfo, name: string) {
	const screenshotPath = testInfo.outputPath(`${name}.png`);

	await page.screenshot({
		animations: "disabled",
		fullPage: true,
		path: screenshotPath,
	});
	await testInfo.attach(`${name}.png`, {
		contentType: "image/png",
		path: screenshotPath,
	});
}
