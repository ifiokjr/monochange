//! Home page — landing page with animated content and hero logo.

use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use leptos_use::use_intersection_observer;

/// Home page component.
#[component]
pub fn HomePage() -> impl IntoView {
    view! {
        // ── Hero section ──
        <section class="relative overflow-hidden bg-white pt-16 dark:bg-gray-950">
            // Decorative gradient blobs
            <div class="pointer-events-none absolute inset-0 overflow-hidden">
                <div class="absolute -top-40 left-1/2 h-[600px] w-[800px] -translate-x-1/2 rounded-full bg-gradient-to-br from-brand-200/40 via-brand-100/20 to-transparent blur-3xl dark:from-brand-600/20 dark:via-brand-800/10" />
                <div class="absolute -right-40 top-0 h-[400px] w-[500px] rounded-full bg-gradient-to-bl from-purple-200/30 to-transparent blur-3xl dark:from-purple-800/20" />
            </div>

            <div class="relative mx-auto max-w-7xl px-4 pb-24 pt-16 sm:px-6 lg:px-8 lg:pb-32 lg:pt-24">
                <div class="text-center">
                    // Logo mark — large centered
                    <div class="mx-auto mb-10 flex size-24 items-center justify-center rounded-3xl bg-gradient-to-br from-brand-50 to-brand-100 p-5 shadow-lg shadow-brand-100/50 dark:from-brand-950 dark:to-brand-900 dark:shadow-brand-900/30">
                        <img
                            src="/branding/logos/08-delta-with-m.svg"
                            alt="monochange"
                            class="size-14 transition-transform duration-500 hover:scale-110"
                        />
                    </div>

                    // Alpha badge
                    <AnimatedEntry>
                        <span class="inline-flex items-center gap-x-1.5 rounded-full bg-brand-50 px-3 py-1.5 text-xs font-medium text-brand-700 ring-1 ring-inset ring-brand-600/20 dark:bg-brand-950 dark:text-brand-300 dark:ring-brand-400/20">
                            <span class="relative flex size-1.5">
                                <span class="absolute inline-flex size-full animate-ping rounded-full bg-brand-400 opacity-75" />
                                <span class="relative inline-flex size-1.5 rounded-full bg-brand-500" />
                            </span>
                            Early Access
                        </span>
                    </AnimatedEntry>

                    // Main heading
                    <AnimatedEntry>
                        <h1 class="mt-10 text-5xl font-extrabold tracking-tight text-gray-900 sm:text-7xl dark:text-white">
                            <span class="bg-gradient-to-r from-brand-600 via-purple-600 to-brand-600 bg-clip-text text-transparent dark:from-brand-400 dark:via-purple-400 dark:to-brand-400">
                                Release planning
                            </span>
                            <br />
                            <span>for monorepos</span>
                        </h1>
                    </AnimatedEntry>

                    // Subtitle
                    <AnimatedEntry>
                        <p class="mx-auto mt-8 max-w-2xl text-lg leading-8 text-gray-600 sm:text-xl dark:text-gray-400">
                            Automated changesets, AI-powered roadmaps, and beautiful changelogs.
                            <br />
                            Install the GitHub App and let monochange manage how your
                            <br class="hidden sm:block" />
                            application evolves over time.
                        </p>
                    </AnimatedEntry>

                    // CTA buttons
                    <AnimatedEntry>
                        <div class="mt-12 flex items-center justify-center gap-x-4">
                            <a
                                href="/login"
                                class="group relative inline-flex items-center gap-x-2 rounded-xl bg-gray-900 px-8 py-4 text-sm font-semibold text-white shadow-lg shadow-gray-900/10 transition-all hover:bg-gray-800 hover:shadow-gray-900/20 hover:-translate-y-0.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gray-900 dark:bg-white dark:text-gray-900 dark:shadow-white/10 dark:hover:bg-gray-100"
                            >
                                <svg class="size-5 fill-white dark:fill-gray-900" viewBox="0 0 16 16">
                                    <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0016 8c0-4.42-3.58-8-8-8z" />
                                </svg>
                                Sign in with GitHub
                                <svg class="size-4 transition-transform group-hover:translate-x-0.5" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
                                </svg>
                            </a>
                            <a
                                href="https://github.com/monochange/monochange"
                                target="_blank"
                                rel="noopener noreferrer"
                                class="inline-flex items-center gap-x-2 rounded-xl bg-gray-100 px-8 py-4 text-sm font-semibold text-gray-900 transition-all hover:bg-gray-200 hover:-translate-y-0.5 dark:bg-gray-800 dark:text-gray-100 dark:hover:bg-gray-700"
                            >
                                <svg class="size-5" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M17.25 6.75L22.5 12l-5.25 5.25m-10.5 0L1.5 12l5.25-5.25m7.5-3l-4.5 16.5" />
                                </svg>
                                Open source
                            </a>
                        </div>
                    </AnimatedEntry>
                </div>
            </div>

            // Bottom gradient fade
            <div class="pointer-events-none absolute inset-x-0 bottom-0 h-32 bg-gradient-to-t from-gray-50 to-transparent dark:from-gray-900/50" />
        </section>

        // ── Feature grid ──
        <section class="relative bg-gray-50 py-24 dark:bg-gray-900/50 sm:py-32">
            <div class="mx-auto max-w-7xl px-4 sm:px-6 lg:px-8">
                <div class="mx-auto max-w-2xl text-center">
                    <h2 class="text-3xl font-bold tracking-tight text-gray-900 sm:text-4xl dark:text-white">
                        Everything you need
                    </h2>
                    <p class="mt-4 text-lg leading-8 text-gray-600 dark:text-gray-400">
                        From automated changesets to public roadmaps, monochange handles the entire release workflow.
                    </p>
                </div>

                <div class="mx-auto mt-16 grid max-w-5xl gap-8 sm:grid-cols-2 lg:grid-cols-3">
                    {animated_feature("🤖", "AI Changesets", "Open an issue and monochange scopes it into a properly formatted changeset with the right bump level and package target.")}
                    {animated_feature("📋", "Public Roadmap", "Share a beautiful public roadmap. AI keeps it updated as features ship and PRs merge.")}
                    {animated_feature("✨", "Beautiful Changelogs", "Auto-generated, user-facing changelogs your users will actually want to read. Customizable templates and RSS feeds.")}
                    {animated_feature("🔗", "GitHub App", "Install the GitHub App and let monochange manage releases, PRs, and changesets as @monochange[bot].")}
                    {animated_feature("💬", "Feedback Forms", "Embed feedback widgets in your app or terminal. AI triages submissions and merges duplicates.")}
                    {animated_feature("📦", "Cross-Ecosystem", "Cargo, npm, Dart, Python, Go, Deno. monochange speaks every package manager in your monorepo.")}
                </div>
            </div>
        </section>

        // ── CTA section ──
        <section class="relative bg-white py-24 dark:bg-gray-950 sm:py-32">
            <div class="pointer-events-none absolute inset-0 overflow-hidden">
                <div class="absolute left-1/2 top-0 h-[500px] w-[600px] -translate-x-1/2 rounded-full bg-gradient-to-b from-brand-100/40 to-transparent blur-3xl dark:from-brand-800/10" />
            </div>
            <div class="relative mx-auto max-w-2xl text-center">
                <h2 class="text-3xl font-bold tracking-tight text-gray-900 sm:text-4xl dark:text-white">
                    Ready to streamline your releases?
                </h2>
                <p class="mt-4 text-lg leading-8 text-gray-600 dark:text-gray-400">
                    Free for open-source developer tools. Upgrade when you need more.
                </p>
                <div class="mt-10">
                    <a
                        href="/login"
                        class="inline-flex items-center gap-x-2 rounded-xl bg-brand-600 px-8 py-4 text-sm font-semibold text-white shadow-lg shadow-brand-600/25 transition-all hover:bg-brand-700 hover:shadow-brand-600/40 hover:-translate-y-0.5 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-brand-600"
                    >
                        Get started free
                        <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
                        </svg>
                    </a>
                </div>
            </div>
        </section>
    }
}

/// Animated entry wrapper — fades up on scroll into view.
#[component]
fn AnimatedEntry(children: Children) -> impl IntoView {
    let el = NodeRef::new();
    let (visible, set_visible) = signal(false);

    #[cfg(target_arch = "wasm32")]
    let _ = use_intersection_observer(el, move |entries, _| {
        for entry in entries {
            if entry.is_intersecting() {
                set_visible.set(true);
            }
        }
    });
    // On SSR, elements are always visible
    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_visible.set(true);

    view! {
        <div
            node_ref=el
            class=move || {
                if visible.get() {
                    "opacity-100 translate-y-0 transition-all duration-700 ease-out"
                } else {
                    "opacity-0 translate-y-8"
                }
            }
        >
            {children()}
        </div>
    }
}

/// Animated feature card — fades up with stagger based on index.
fn animated_feature(
    emoji: &'static str,
    title: &'static str,
    description: &'static str,
) -> impl IntoView {
    let el = NodeRef::new();
    let (visible, set_visible) = signal(false);

    #[cfg(target_arch = "wasm32")]
    let _ = use_intersection_observer(el, move |entries, _| {
        for entry in entries {
            if entry.is_intersecting() {
                set_visible.set(true);
            }
        }
    });
    // On SSR, elements are always visible
    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_visible.set(true);

    view! {
        <div
            node_ref=el
            class=move || {
                if visible.get() {
                    "opacity-100 translate-y-0 transition-all duration-500 ease-out"
                } else {
                    "opacity-0 translate-y-6"
                }
            }
        >
            <div class="group relative rounded-2xl border border-gray-200 bg-white p-8 transition-all duration-300 hover:border-brand-200 hover:shadow-lg hover:shadow-brand-100/50 hover:-translate-y-1 dark:border-gray-800 dark:bg-gray-900 dark:hover:border-brand-800 dark:hover:shadow-brand-900/30">
                <div class="mb-5 inline-flex rounded-xl bg-brand-50 p-3 text-2xl transition-transform duration-300 group-hover:scale-110 dark:bg-brand-950">
                    {emoji}
                </div>
                <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                    {title}
                </h3>
                <p class="mt-3 text-sm leading-6 text-gray-600 dark:text-gray-400">
                    {description}
                </p>
            </div>
        </div>
    }
}
