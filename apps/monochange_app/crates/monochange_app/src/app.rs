//! Root application component with polished design.
//!
//! Features WASM code splitting, scroll-aware navbar, animated content,
//! and the monochange brand logo.

use leptos::prelude::*;
use leptos_router::{path, components::*};

use crate::components::navbar::NavBar;
use crate::error::ErrorTemplate;
use crate::pages::home::HomePage;

/// Root `<App/>` component.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <NavBar />
            <main class="min-h-screen">
                <Routes fallback=|| {
                    view! { <ErrorTemplate status=404 message="Page not found" /> }
                }>
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/auth/callback") view=AuthCallbackPage />
                </Routes>
            </main>
            <Footer />
        </Router>
    }
}

// ── Login Page ──

#[component]
fn LoginPage() -> impl IntoView {
    let login_url = Resource::new(|| (), |_| async {
        crate::server_fns::auth::get_login_url().await.unwrap_or_default()
    });

    view! {
        <div class="flex min-h-[80vh] items-center justify-center px-4">
            <div class="w-full max-w-md text-center">
                // Logo mark
                <div class="mx-auto mb-8 flex size-20 items-center justify-center rounded-2xl bg-brand-50 p-4 dark:bg-brand-950">
                    <img src="/branding/logos/08-delta-with-m.svg" alt="monochange" class="size-12" />
                </div>

                <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                    Sign in to monochange
                </h2>
                <p class="mt-2 text-gray-600 dark:text-gray-400">
                    Connect your GitHub account to manage releases, roadmaps, and changelogs.
                </p>

                <div class="mt-10">
                    <Suspense fallback=|| {
                        view! {
                            <div class="mx-auto h-12 w-64 animate-pulse rounded-xl bg-gray-100 dark:bg-gray-800" />
                        }
                    }>
                        {move || login_url.get().map(|url| {
                            if url.is_empty() {
                                view! {
                                    <div class="rounded-xl border border-red-200 bg-red-50 p-4 dark:border-red-800 dark:bg-red-950">
                                        <p class="text-sm text-red-600 dark:text-red-400">
                                            OAuth is not configured. Set GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET.
                                        </p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <a
                                        href=url
                                        class="group inline-flex items-center gap-x-2 rounded-xl bg-gray-900 px-8 py-4 text-sm font-semibold text-white shadow-lg shadow-gray-900/10 transition-all hover:bg-gray-800 hover:shadow-gray-900/20 hover:-translate-y-0.5 dark:bg-white dark:text-gray-900 dark:shadow-white/10 dark:hover:bg-gray-100"
                                    >
                                        <svg class="size-5 fill-white dark:fill-gray-900" viewBox="0 0 16 16">
                                            <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
                                        </svg>
                                        Continue with GitHub
                                        <svg class="size-4 transition-transform group-hover:translate-x-0.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
                                        </svg>
                                    </a>
                                }.into_any()
                            }
                        })}
                    </Suspense>
                </div>

                <p class="mt-6 text-xs text-gray-400 dark:text-gray-500">
                    By signing in, you agree to monochange&apos;s Terms of Service and Privacy Policy.
                </p>
            </div>
        </div>
    }
}

// ── Auth Callback Page ──

#[component]
fn AuthCallbackPage() -> impl IntoView {
    let params = leptos_router::hooks::use_query_map();

    let result = Resource::new(
        move || params.get(),
        |params| async move {
            let code = params.get("code").map(|s| s.clone()).unwrap_or_default();
            let state = params.get("state").map(|s| s.clone()).unwrap_or_default();
            if code.is_empty() {
                return Err("No authorization code received".to_string());
            }
            crate::server_fns::auth::exchange_code(code, state).await.map_err(|e| e.to_string())
        },
    );

    view! {
        <div class="flex min-h-[80vh] items-center justify-center">
            <Suspense fallback=|| {
                view! {
                    <div class="text-center">
                        <div class="mx-auto size-12 animate-spin rounded-full border-4 border-brand-200 border-t-brand-600" />
                        <p class="mt-4 text-sm text-gray-500">Completing sign in...</p>
                    </div>
                }
            }>
                {move || result.get().map(|r| match r {
                    Ok(user) => view! {
                        <div class="text-center">
                            <div class="mx-auto mb-4 flex size-16 items-center justify-center rounded-full bg-green-100 dark:bg-green-900">
                                <svg class="size-8 text-green-600 dark:text-green-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 12.75l6 6 9-13.5" />
                                </svg>
                            </div>
                            <h2 class="text-2xl font-bold text-gray-900 dark:text-white">Signed in!</h2>
                            <p class="mt-2 text-gray-600 dark:text-gray-400">Welcome, {user.github_login}!</p>
                            <a href="/" class="mt-8 inline-flex items-center gap-x-2 rounded-xl bg-brand-600 px-6 py-3 text-sm font-semibold text-white shadow-sm hover:bg-brand-700 transition-colors">
                                Go to dashboard
                                <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
                                </svg>
                            </a>
                        </div>
                    }.into_any(),
                    Err(e) => view! {
                        <div class="text-center">
                            <div class="mx-auto mb-4 flex size-16 items-center justify-center rounded-full bg-red-100 dark:bg-red-900">
                                <svg class="size-8 text-red-600 dark:text-red-400" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </div>
                            <h2 class="text-2xl font-bold text-red-600 dark:text-red-400">Sign in failed</h2>
                            <p class="mt-2 text-gray-600 dark:text-gray-400">{e}</p>
                            <a href="/login" class="mt-8 inline-block rounded-xl bg-brand-600 px-6 py-3 text-sm font-semibold text-white hover:bg-brand-700 transition-colors">
                                Try again
                            </a>
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

// ── Footer ──

#[component]
fn Footer() -> impl IntoView {
    view! {
        <footer class="border-t border-gray-200 bg-white dark:border-gray-800 dark:bg-gray-950">
            <div class="mx-auto max-w-7xl px-4 py-12 sm:px-6 lg:px-8">
                <div class="grid gap-8 sm:grid-cols-3">
                    // Brand
                    <div>
                        <div class="flex items-center gap-2">
                            <img src="/branding/logos/08-delta-with-m.svg" alt="monochange" class="size-8" />
                            <span class="text-lg font-bold text-gray-900 dark:text-white">monochange</span>
                        </div>
                        <p class="mt-3 text-sm text-gray-500 dark:text-gray-400">
                            Release planning for monorepos. Built with Rust and Leptos.
                        </p>
                    </div>

                    // Links
                    <div class="flex justify-center gap-12">
                        <div>
                            <h4 class="text-sm font-semibold text-gray-900 dark:text-white">Product</h4>
                            <div class="mt-3 space-y-2">
                                <a href="/" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">Home</a>
                                <a href="/docs" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">Docs</a>
                                <a href="/pricing" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">Pricing</a>
                            </div>
                        </div>
                        <div>
                            <h4 class="text-sm font-semibold text-gray-900 dark:text-white">Company</h4>
                            <div class="mt-3 space-y-2">
                                <a href="https://github.com/monochange/monochange" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">GitHub</a>
                                <a href="/blog" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">Blog</a>
                                <a href="/privacy" class="block text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-300">Privacy</a>
                            </div>
                        </div>
                    </div>

                    // Status
                    <div class="text-right">
                        <div class="inline-flex items-center gap-x-1.5 rounded-full bg-green-50 px-3 py-1 text-xs font-medium text-green-700 ring-1 ring-inset ring-green-600/20 dark:bg-green-950 dark:text-green-300 dark:ring-green-400/20">
                            <span class="relative flex size-1.5">
                                <span class="absolute inline-flex size-full animate-ping rounded-full bg-green-400 opacity-75" />
                                <span class="relative inline-flex size-1.5 rounded-full bg-green-500" />
                            </span>
                            All systems operational
                        </div>
                    </div>
                </div>

                <div class="mt-8 border-t border-gray-100 pt-8 dark:border-gray-800">
                    <p class="text-center text-xs text-gray-400 dark:text-gray-500">
                        &copy; 2026 monochange. All rights reserved.
                    </p>
                </div>
            </div>
        </footer>
    }
}
