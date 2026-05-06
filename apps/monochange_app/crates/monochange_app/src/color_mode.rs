//! Dark / light color mode context using Leptos signals.
//!
//! Persists preference in localStorage and respects OS-level preference.

use leptos::prelude::*;

/// Color mode variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Light,
    Dark,
}

impl ColorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorMode::Light => "light",
            ColorMode::Dark => "dark",
        }
    }
}

/// Reactive color mode state.
#[derive(Clone)]
pub struct ColorModeState {
    pub mode: Signal<ColorMode>,
    pub toggle: Callback<()>,
    pub set_mode: Callback<ColorMode>,
}

/// Provide color mode context and return the state.
///
/// On the server (SSR), defaults to Light since browser APIs are unavailable.
/// On the client (WASM), reads from localStorage and OS preference.
pub fn provide_color_mode() -> ColorModeState {
    // Read initial mode — on SSR, default to Light
    let initial = initial_mode();

    let (mode, set_mode) = signal(initial);

    // Apply mode changes to DOM and localStorage (client only)
    #[cfg(target_arch = "wasm32")]
    {
        let mode = mode;
        // Apply initial mode
        apply_mode(initial);
        // Watch for changes
        let _ = Effect::new(move || {
            apply_mode(mode.get());
        });
    }

    let toggle = Callback::new(move |()| {
        set_mode.update(|m| {
            *m = match *m {
                ColorMode::Light => ColorMode::Dark,
                ColorMode::Dark => ColorMode::Light,
            };
        });
    });

    let set_mode_cb = Callback::new(move |m: ColorMode| {
        set_mode.set(m);
    });

    let state = ColorModeState {
        mode: mode.into(),
        toggle,
        set_mode: set_mode_cb,
    };

    provide_context(state.clone());
    state
}

/// Determine initial color mode.
///
/// On the client (WASM): checks localStorage, then OS preference, defaults to Light.
/// On the server (SSR): always Light.
fn initial_mode() -> ColorMode {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            // Check localStorage first
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(value)) = storage.get_item("monochange-color-mode") {
                    return match value.as_str() {
                        "dark" => ColorMode::Dark,
                        _ => ColorMode::Light,
                    };
                }
            }
            // Fall back to OS preference
            if let Ok(Some(media)) = window.match_media("(prefers-color-scheme: dark)") {
                if media.matches() {
                    return ColorMode::Dark;
                }
            }
        }
    }
    ColorMode::Light
}

/// Apply color mode to the DOM (client only).
#[cfg(target_arch = "wasm32")]
fn apply_mode(mode: ColorMode) {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            let html = document.document_element().unwrap();
            let _ = html.set_attribute("class", mode.as_str());
            // Save preference
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("monochange-color-mode", mode.as_str());
            }
        }
    }
}

/// Hook to access the color mode state.
pub fn use_color_mode() -> ColorModeState {
    use_context::<ColorModeState>()
        .expect("ColorModeState not provided. Call provide_color_mode() first.")
}
