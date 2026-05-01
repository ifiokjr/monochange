//! Polished navigation bar with:
//! - Scroll-aware background (transparent → blurred on scroll)
//! - Logo mark integration
//! - Smooth theme toggle with rotation animation
//! - Mobile hamburger menu
//! - Active route highlighting

use leptos::prelude::*;
use leptos_router::hooks::use_location;
#[cfg(target_arch = "wasm32")]
use leptos_use::use_window_scroll;
use crate::color_mode::{ColorMode, provide_color_mode, use_color_mode};

/// Navigation bar component.
#[component]
pub fn NavBar() -> impl IntoView {
    let _color_mode = provide_color_mode();

    view! {
        <nav class="fixed inset-x-0 top-0 z-50 transition-all duration-300" id="main-nav">
            <NavContent />
        </nav>
        // Spacer to prevent content from hiding behind fixed nav
        <div class="h-16" />
    }
}

/// Inner nav content that responds to scroll.
#[component]
fn NavContent() -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    let (scroll_y, _) = use_window_scroll();
    #[cfg(not(target_arch = "wasm32"))]
    let (scroll_y, _) = (RwSignal::new(0.0).read_only(), RwSignal::new(0.0).read_only());
    let color_mode = use_color_mode();
    let (mobile_open, set_mobile_open) = signal(false);
    let location = use_location();

    // Track scroll position for background effects
    let is_scrolled = Signal::derive(move || scroll_y.get() > 10.0);
    let is_home = Signal::derive(move || location.pathname.get() == "/");

    // Close mobile menu on route change
    let _ = Effect::new(move || {
        location.pathname.get();
        set_mobile_open.set(false);
    });

    view! {
        <div class=move || {
            let scrolled = is_scrolled.get();
            let home = is_home.get();
            if home && !scrolled {
                "border-b border-transparent bg-transparent transition-all duration-300"
            } else {
                "border-b border-gray-200 bg-white/80 backdrop-blur-xl shadow-sm dark:border-gray-800 dark:bg-gray-950/80 transition-all duration-300"
            }
        }>
            <div class="mx-auto flex max-w-7xl items-center justify-between px-4 py-3 sm:px-6 lg:px-8">
                // Logo + brand
                <a href="/" class="flex items-center gap-2.5 group">
                    <img
                        src="/branding/logos/08-delta-with-m.svg"
                        alt="monochange"
                        class="size-8 transition-transform duration-300 group-hover:scale-110"
                    />
                    <span class="text-lg font-bold text-gray-900 dark:text-white">
                        monochange
                    </span>
                </a>

                // Desktop nav links
                <div class="hidden items-center gap-1 md:flex">
                    <NavLink path="/" label="Home" active=Signal::derive(move || location.pathname.get() == "/") />
                    <NavLink path="/docs" label="Docs" active=Signal::derive(move || location.pathname.get().starts_with("/docs")) />
                    <NavLink path="/pricing" label="Pricing" active=Signal::derive(move || location.pathname.get() == "/pricing") />
                </div>

                // Right side actions
                <div class="flex items-center gap-2">
                    // Theme toggle
                    <button
                        on:click=move |_| color_mode.toggle.run(())
                        class="rounded-lg p-2 text-gray-500 transition-all duration-300 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800"
                        aria-label="Toggle color mode"
                    >
                        <span class="block transition-transform duration-500" class:rotate-180=move || color_mode.mode.get() == ColorMode::Dark>
                            {move || match color_mode.mode.get() {
                                ColorMode::Dark => view! {
                                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-5">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z" />
                                    </svg>
                                }.into_any(),
                                ColorMode::Light => view! {
                                    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-5">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z" />
                                    </svg>
                                }.into_any(),
                            }}
                        </span>
                    </button>

                    // Sign in button (desktop)
                    <a
                        href="/login"
                        class="hidden rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white transition-all hover:bg-brand-700 hover:shadow-md sm:inline-flex"
                    >
                        Sign in
                    </a>

                    // Mobile menu toggle
                    <button
                        on:click=move |_| set_mobile_open.update(|v| *v = !*v)
                        class="rounded-lg p-2 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800 md:hidden"
                        aria-label="Toggle menu"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" class="size-5">
                            {move || if mobile_open.get() {
                                view! { <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" /> }.into_any()
                            } else {
                                view! { <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" /> }.into_any()
                            }}
                        </svg>
                    </button>
                </div>
            </div>

            // Mobile menu dropdown
            {move || mobile_open.get().then(|| view! {
                <div class="border-t border-gray-200 bg-white px-4 py-3 dark:border-gray-800 dark:bg-gray-950 md:hidden animate-in slide-in-from-top-2 duration-200">
                    <div class="space-y-1">
                        <MobileNavLink path="/" label="Home" />
                        <MobileNavLink path="/docs" label="Docs" />
                        <MobileNavLink path="/pricing" label="Pricing" />
                        <div class="pt-2">
                            <a href="/login" class="block w-full rounded-lg bg-brand-600 px-4 py-2.5 text-center text-sm font-medium text-white hover:bg-brand-700">
                                Sign in
                            </a>
                        </div>
                    </div>
                </div>
            })}
        </div>
    }
}

/// Desktop navigation link with active state.
#[component]
fn NavLink(
    path: &'static str,
    label: &'static str,
    active: Signal<bool>,
) -> impl IntoView {
    view! {
        <a
            href=path
            class=move || {
                if active.get() {
                    "rounded-lg bg-gray-100 px-3 py-2 text-sm font-medium text-gray-900 dark:bg-gray-800 dark:text-white"
                } else {
                    "rounded-lg px-3 py-2 text-sm font-medium text-gray-600 hover:bg-gray-50 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-white transition-colors"
                }
            }
        >
            {label}
        </a>
    }
}

/// Mobile navigation link.
#[component]
fn MobileNavLink(
    path: &'static str,
    label: &'static str,
) -> impl IntoView {
    view! {
        <a
            href=path
            class="block rounded-lg px-3 py-2.5 text-base font-medium text-gray-900 hover:bg-gray-100 dark:text-white dark:hover:bg-gray-800 transition-colors"
        >
            {label}
        </a>
    }
}
