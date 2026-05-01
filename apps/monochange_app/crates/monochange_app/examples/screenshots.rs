//! Screenshot capture script using playwright-rs.
//! Run with: cargo run --example screenshots

use playwright_rs::{Playwright, BrowserType};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let playwright = Playwright::initialize().await?;
    playwright.prepare()?;

    let browser = playwright.chromium().launcher().headless(true).launch().await?;
    let context = browser.context_builder().viewport(1280, 800).build().await?;
    let page = context.new_page().await?;

    let screenshots_dir = Path::new("target/screenshots");
    std::fs::create_dir_all(screenshots_dir)?;

    let pages = vec![
        ("home", "http://localhost:3000/"),
        ("login", "http://localhost:3000/login"),
    ];

    for (name, url) in pages {
        println!("Screenshotting: {name} ({url})");
        page.goto(url, None).await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        page.screenshot(
            playwright_rs::ScreenshotOptions::default()
                .path(screenshots_dir.join(format!("{name}.png")))
                .full_page(true),
        )
        .await?;
    }

    println!("Screenshots saved to target/screenshots/");
    Ok(())
}
