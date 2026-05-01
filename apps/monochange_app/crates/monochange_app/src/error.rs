//! Error types and error template component.

use leptos::prelude::*;
use thiserror::Error;

/// Application-level error type.
#[derive(Debug, Error)]
#[must_use]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Authentication required")]
    Unauthorized,

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("GitHub API error: {0}")]
    GitHub(String),

    #[error("Database error: {0}")]
    Database(String),
}

impl From<AppError> for u16 {
    fn from(err: AppError) -> Self {
        match err {
            AppError::NotFound(_) => 404,
            AppError::Unauthorized => 401,
            AppError::Internal(_) => 500,
            AppError::GitHub(_) => 502,
            AppError::Database(_) => 500,
        }
    }
}

/// Error page component.
#[component]
pub fn ErrorTemplate(
    #[prop(default = 500)] status: u16,
    #[prop(default = "Something went wrong.")] message: &'static str,
) -> impl IntoView {
    let title = move || match status {
        404 => "Page not found",
        401 => "Not authorized",
        _ => "Server error",
    };

    let emoji = move || match status {
        404 => "🔍",
        401 => "🔒",
        _ => "⚠️",
    };

    view! {
        <div class="flex min-h-screen items-center justify-center bg-white dark:bg-gray-950">
            <div class="text-center">
                <div class="text-7xl">{emoji}</div>
                <h1 class="mt-8 text-6xl font-bold text-gray-200 dark:text-gray-800">
                    {status}
                </h1>
                <h2 class="mt-4 text-2xl font-semibold text-gray-900 dark:text-white">
                    {title}
                </h2>
                <p class="mt-3 text-gray-600 dark:text-gray-400">{message}</p>
                <div class="mt-10">
                    <a
                        href="/"
                        class="inline-flex items-center gap-x-2 rounded-xl bg-gray-900 px-6 py-3 text-sm font-semibold text-white shadow-sm hover:bg-gray-800 dark:bg-white dark:text-gray-900 dark:hover:bg-gray-100"
                    >
                        <svg class="size-4" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M10.5 19.5L3 12m0 0l7.5-7.5M3 12h18" />
                        </svg>
                        Go home
                    </a>
                </div>
            </div>
        </div>
    }
}
