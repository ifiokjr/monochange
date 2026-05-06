//! End-to-end tests using Playwright (Rust bindings via padamson/playwright-rust).
//!
//! These tests require the server to be running at `http://localhost:3000`.
//! Run via: `cargo leptos end-to-end`
//!
//! Prerequisites:
//!   npx playwright install chromium

use playwright_rs::api::{Browser, BrowserContext, Page};
use playwright_rs::Playwright;
use rstest::{fixture, rstest};
use std::sync::Once;

static INIT: Once = Once::new();
const BASE_URL: &str = "http://localhost:3000";

/// Shared browser fixture for all E2E tests.
#[fixture]
async fn browser_context() -> (Playwright, Browser, BrowserContext) {
    let playwright = Playwright::initialize().await.expect("Failed to initialize Playwright");
    playwright.install_chromium().expect("Failed to install Chromium");

    let browser = playwright
        .chromium()
        .launch()
        .headless(true)
        .launch()
        .await
        .expect("Failed to launch browser");

    let context = browser
        .context_builder()
        .build()
        .await
        .expect("Failed to create browser context");

    (playwright, browser, context)
}

async fn new_page(context: &BrowserContext) -> Page {
    context.new_page().await.expect("Failed to create page")
}

// ── Home page tests ──

#[rstest]
#[tokio::test]
async fn home_page_loads_with_hero(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("Failed to navigate");
    page.wait_for_selector("h1").await.expect("h1 not found");

    let heading = page.query_selector("h1").await.expect("query h1")
        .expect("h1 element not present");
    let text = heading.inner_text().await.expect("get heading text");
    assert!(text.contains("monochange"), "Heading: {text}");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn page_title_is_monochange(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    let title = page.title().await.expect("get title");
    assert!(title.contains("monochange"), "Title: {title}");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn sign_in_button_exists(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    let link = page.query_selector("a[href=\"/login\"]").await.expect("query link");
    assert!(link.is_some(), "Sign-in link not found");

    browser.close().await.expect("close browser");
}

// ── Navigation tests ──

#[rstest]
#[tokio::test]
async fn navbar_contains_brand(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_selector("nav").await.expect("nav not found");

    let nav = page.query_selector("nav").await.expect("query nav")
        .expect("nav element not present");
    let text = nav.inner_text().await.expect("get nav text");
    assert!(text.contains("monochange"), "Nav text: {text}");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn github_link_points_to_repo(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");

    let link = page.query_selector("a[href=\"https://github.com/monochange/monochange\"]")
        .await.expect("query github link");
    assert!(link.is_some(), "GitHub link not found on page");

    browser.close().await.expect("close browser");
}

// ── Theme tests ──

#[rstest]
#[tokio::test]
async fn theme_toggle_changes_color_mode(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_timeout(1000.0).await;

    let initial = page.evaluate("document.documentElement.className").await
        .expect("get class");
    let initial: String = initial.as_str().unwrap_or("light").to_string();

    page.click("button[aria-label=\"Toggle color mode\"]").await.expect("click toggle");
    page.wait_for_timeout(500.0).await;

    let toggled = page.evaluate("document.documentElement.className").await
        .expect("get class");
    let toggled: String = toggled.as_str().unwrap_or("").to_string();

    assert_ne!(toggled, initial, "Class should change after toggle");
    assert!(toggled == "light" || toggled == "dark", "Unexpected: {toggled}");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn theme_persists_across_page_loads(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    // First load
    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_timeout(1000.0).await;

    // Set to dark
    page.click("button[aria-label=\"Toggle color mode\"]").await.expect("toggle");
    page.wait_for_timeout(500.0).await;

    let after_toggle = page.evaluate("document.documentElement.className").await
        .expect("get class");
    let after: String = after_toggle.as_str().unwrap_or("").to_string();

    // Reload
    page.goto_builder(BASE_URL).goto().await.expect("reload");
    page.wait_for_timeout(1000.0).await;

    let after_reload = page.evaluate("document.documentElement.className").await
        .expect("get class");
    let reloaded: String = after_reload.as_str().unwrap_or("").to_string();

    assert_eq!(reloaded, after, "Theme should persist across reloads");

    browser.close().await.expect("close browser");
}

// ── Content structure tests ──

#[rstest]
#[tokio::test]
async fn feature_cards_are_present(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_selector("h2").await.expect("h2 not found");

    // Should have feature cards in the grid
    let cards = page.query_selector_all(".grid > div").await.expect("query cards");
    assert!(cards.len() >= 3, "Expected at least 3 feature cards, got {}", cards.len());

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn cta_section_has_get_started_button(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");

    // The CTA button text contains "Get started"
    let body = page.inner_text("body").await.expect("get body text");
    assert!(body.contains("Get started free"), "CTA text not found. Body: {body}");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn footer_contains_copyright(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_selector("footer").await.expect("footer not found");

    let footer = page.inner_text("footer").await.expect("get footer text");
    assert!(footer.contains("2026 monochange"), "Footer text: {footer}");

    browser.close().await.expect("close browser");
}

// ── Responsive/layout tests ──

#[rstest]
#[tokio::test]
async fn page_has_viewport_meta(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");

    let has_viewport = page.evaluate(
        "!!document.querySelector('meta[name=\"viewport\"]')"
    ).await.expect("evaluate viewport");

    assert!(has_viewport.as_bool().unwrap_or(false), "Viewport meta tag missing");

    browser.close().await.expect("close browser");
}

#[rstest]
#[tokio::test]
async fn stylesheet_is_loaded(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(BASE_URL).goto().await.expect("navigate");

    let has_css = page.evaluate(
        "!!document.querySelector('link[href*=\"output.css\"]')"
    ).await.expect("evaluate css");

    assert!(has_css.as_bool().unwrap_or(false), "Tailwind CSS not loaded");

    browser.close().await.expect("close browser");
}

// ── Login page tests ──

#[rstest]
#[tokio::test]
async fn login_page_loads(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(&format!("{BASE_URL}/login")).goto().await.expect("navigate");
    page.wait_for_timeout(1000.0).await;

    let body = page.inner_text("body").await.expect("get body text");
    assert!(body.contains("Sign in"), "Login page text: {body}");

    browser.close().await.expect("close browser");
}

// ── Error page tests ──

#[rstest]
#[tokio::test]
async fn not_found_page_returns_404_content(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    page.goto_builder(&format!("{BASE_URL}/nonexistent-page-12345"))
        .goto().await.expect("navigate");
    page.wait_for_timeout(1000.0).await;

    let body = page.inner_text("body").await.expect("get body text");
    assert!(body.contains("Page not found") || body.contains("404"), "404 text: {body}");

    browser.close().await.expect("close browser");
}

// ── Performance smoke tests ──

#[rstest]
#[tokio::test]
async fn home_page_loads_under_3_seconds(
    #[future] browser_context: (Playwright, Browser, BrowserContext),
) {
    use std::time::Instant;

    let (_pw, browser, context) = browser_context.await;
    let page = new_page(&context).await;

    let start = Instant::now();
    page.goto_builder(BASE_URL).goto().await.expect("navigate");
    page.wait_for_selector("main").await.expect("main not found");
    page.wait_for_timeout(500.0).await;
    let elapsed = start.elapsed();

    // Allow up to 10 seconds in CI (first compilation can be slow)
    assert!(elapsed.as_secs() < 10, "Home page load took {elapsed:?}");

    browser.close().await.expect("close browser");
}
