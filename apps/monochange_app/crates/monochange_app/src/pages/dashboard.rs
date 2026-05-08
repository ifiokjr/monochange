//! Dashboard page for logged-in users.

use leptos::prelude::*;

use crate::server_fns::auth::get_session;
use crate::server_fns::repos::list_repos;

/// Dashboard page shown after login.
#[component]
pub fn DashboardPage() -> impl IntoView {
	let user = Resource::new(|| (), |_| async move { get_session().await.ok().flatten() });

	let repos = Resource::new(
		|| (),
		|_| async move { list_repos().await.unwrap_or_default() },
	);

	view! {
		<div class="mx-auto max-w-7xl px-4 py-12 sm:px-6 lg:px-8">
			<Suspense fallback={move || {
				view! {
					<div class="space-y-8">
						<div class="h-32 animate-pulse rounded-2xl bg-gray-100 dark:bg-gray-800" />
						<div class="h-64 animate-pulse rounded-2xl bg-gray-100 dark:bg-gray-800" />
					</div>
				}
			}}>
				{move || {
					user.get().map(|u| {
						match u {
							Some(session) => view! {
								<DashboardContent
									user={session}
									repos={repos.get().unwrap_or_default()}
								/>
							}.into_any(),
							None => view! {
								<div class="text-center">
									<p class="text-lg text-gray-600 dark:text-gray-400">
										"Please " <a href="/login" class="text-brand-600 hover:underline dark:text-brand-400">"sign in"</a> " to view your dashboard."
									</p>
								</div>
							}.into_any(),
						}
					})
				}}
			</Suspense>
		</div>
	}
}

#[component]
fn DashboardContent(
	user: crate::server_fns::auth::SessionUser,
	repos: Vec<crate::server_fns::repos::RepoInfo>,
) -> impl IntoView {
	view! {
		<div class="space-y-10">
			{/* Header */}
			<div class="flex items-center gap-4">
				{user.github_avatar_url.map(|url| {
					view! {
						<img
							src={url}
							alt={user.github_login.clone()}
							class="size-14 rounded-full ring-2 ring-gray-100 dark:ring-gray-800"
						/>
					}
					.into_any()
				}).unwrap_or_else(|| {
					view! {
						<div class="flex size-14 items-center justify-center rounded-full bg-gray-100 text-xl font-bold text-gray-600 dark:bg-gray-800 dark:text-gray-300">
							{user.github_login.chars().next().unwrap_or('?').to_uppercase().to_string()}
						</div>
					}
					.into_any()
				})}
				<div>
					<h1 class="text-2xl font-bold text-gray-900 dark:text-white">
						"Welcome, " {user.github_login}
					</h1>
					<p class="text-sm text-gray-500 dark:text-gray-400">
						{match user.plan_tier.as_str() {
							"free" => "Free plan".to_string(),
							_ => user.plan_tier.clone(),
						}}
					</p>
				</div>
			</div>

			{/* Repositories section */}
			<section class="rounded-2xl border border-gray-200 bg-white p-6 dark:border-gray-800 dark:bg-gray-900/50">
				<div class="mb-4 flex items-center justify-between">
					<h2 class="text-lg font-semibold text-gray-900 dark:text-white">
						"Repositories"
					</h2>
					<span class="text-xs text-gray-400 dark:text-gray-500">
						{repos.len()} " connected"
					</span>
				</div>

				{if repos.is_empty() {
					view! {
						<div class="rounded-xl border border-dashed border-gray-300 bg-gray-50 p-8 text-center dark:border-gray-700 dark:bg-gray-800/50">
							<div class="mx-auto mb-3 flex size-12 items-center justify-center rounded-full bg-gray-100 text-xl dark:bg-gray-800">
								"🔗"
							</div>
							<p class="text-sm font-medium text-gray-900 dark:text-white">
								"No repositories connected yet"
							</p>
							<p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
								"Install the monochange GitHub App to connect repositories."
							</p>
							<a
								href="https://github.com/apps/monochange"
								target="_blank"
								rel="noopener noreferrer"
								class="mt-4 inline-flex items-center gap-x-2 rounded-xl bg-brand-600 px-4 py-2 text-sm font-semibold text-white hover:bg-brand-500"
							>
								"Install GitHub App"
								<svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
									<path stroke-linecap="round" stroke-linejoin="round" d="M13.5 4.5L21 12m0 0l-7.5 7.5M21 12H3" />
								</svg>
							</a>
						</div>
					}.into_any()
				} else {
					view! {
						<div class="space-y-2">
							{repos.into_iter().map(|r| {
								view! {
									<div class="flex items-center justify-between rounded-xl border border-gray-200 bg-gray-50 px-4 py-3 dark:border-gray-700 dark:bg-gray-800">
										<div class="flex items-center gap-3">
											<svg class="size-5 text-gray-500" viewBox="0 0 16 16" fill="currentColor">
												<path d="M2 2.5A2.5 2.5 0 014.5 0h8.75a.75.75 0 01.75.75v12.5a.75.75 0 01-.75.75h-2.5a.75.75 0 110-1.5h1.75v-2h-8a1 1 0 00-.714 1.7.75.75 0 01-1.072 1.05A2.495 2.495 0 012 11.5v-9zm10.5-1V9h-8c-.356 0-.694.074-1 .208V2.5a1 1 0 011-1h8zM5 12.25v3.25a.25.25 0 00.4.2l1.45-1.087a.25.25 0 01.3 0L8.6 15.7a.25.25 0 00.4-.2v-3.25a.25.25 0 00-.25-.25h-3.5a.25.25 0 00-.25.25z" />
											</svg>
											<span class="text-sm font-medium text-gray-900 dark:text-white">{r.github_full_name}</span>
											{move || if r.github_private {
												view! { <span class="rounded bg-gray-200 px-1.5 py-0.5 text-xs text-gray-600 dark:bg-gray-700 dark:text-gray-300">"Private"</span> }.into_any()
											} else {
												view! { <span class="rounded bg-brand-50 px-1.5 py-0.5 text-xs text-brand-700 dark:bg-brand-950 dark:text-brand-300">"Public"</span> }.into_any()
											}}
										</div>
										<span class="text-xs text-gray-400 dark:text-gray-500">{r.plan_tier}</span>
									</div>
								}
							}).collect::<Vec<_>>()}
						</div>
					}.into_any()
				}}
			</section>

			{/* Release schedules section */}
			<section class="rounded-2xl border border-gray-200 bg-white p-6 dark:border-gray-800 dark:bg-gray-900/50">
				<div class="mb-4 flex items-center justify-between">
					<h2 class="text-lg font-semibold text-gray-900 dark:text-white">
						"Release Schedules"
					</h2>
					<span class="text-xs text-gray-400 dark:text-gray-500">
						"0 configured"
					</span>
				</div>

				<div class="rounded-xl border border-dashed border-gray-300 bg-gray-50 p-8 text-center dark:border-gray-700 dark:bg-gray-800/50">
					<div class="mx-auto mb-3 flex size-12 items-center justify-center rounded-full bg-gray-100 text-xl dark:bg-gray-800">
						"📅"
					</div>
					<p class="text-sm font-medium text-gray-900 dark:text-white">
						"No release schedules yet"
					</p>
					<p class="mt-1 text-sm text-gray-500 dark:text-gray-400">
						"Connect a repository first, then configure automated release schedules."
					</p>
				</div>
			</section>

			{/* Automation status */}
			<section class="rounded-2xl border border-gray-200 bg-white p-6 dark:border-gray-800 dark:bg-gray-900/50">
				<h2 class="mb-4 text-lg font-semibold text-gray-900 dark:text-white">
					"Automation Status"
				</h2>
				<div class="flex items-center gap-3">
					<span class="relative flex size-3">
						<span class="absolute inline-flex size-full animate-ping rounded-full bg-brand-400 opacity-75" />
						<span class="relative inline-flex size-3 rounded-full bg-brand-500" />
					</span>
					<p class="text-sm text-gray-600 dark:text-gray-400">
						"Dry-run automation worker is active. No repository actions are being dispatched yet."
					</p>
				</div>
			</section>
		</div>
	}
}
