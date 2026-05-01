//! Unit tests for Leptos components and app structure.

#[cfg(test)]
mod tests {
    use crate::color_mode::*;
    use crate::error::*;
    use leptos::prelude::*;
    use rstest::rstest;

    // ── AppError tests ──

    #[rstest]
    #[case(AppError::NotFound("test".into()), 404)]
    #[case(AppError::Unauthorized, 401)]
    #[case(AppError::Internal("err".into()), 500)]
    #[case(AppError::GitHub("api".into()), 502)]
    #[case(AppError::Database("db".into()), 500)]
    fn test_app_error_status_codes(#[case] error: AppError, #[case] expected: u16) {
        assert_eq!(u16::from(error), expected);
    }

    #[rstest]
    fn test_app_error_display() {
        assert_eq!(AppError::NotFound("page".into()).to_string(), "Not found: page");
        assert_eq!(AppError::Unauthorized.to_string(), "Authentication required");
        assert_eq!(AppError::Internal("boom".into()).to_string(), "Internal server error: boom");
        assert_eq!(AppError::GitHub("rate limit".into()).to_string(), "GitHub API error: rate limit");
        assert_eq!(AppError::Database("timeout".into()).to_string(), "Database error: timeout");
    }

    #[rstest]
    fn test_app_error_debug() {
        let err = AppError::NotFound("x".into());
        let debug = format!("{err:?}");
        assert!(debug.contains("NotFound"));
    }

    // ── ColorMode tests ──

    #[rstest]
    fn test_color_mode_as_str() {
        assert_eq!(ColorMode::Light.as_str(), "light");
        assert_eq!(ColorMode::Dark.as_str(), "dark");
    }

    #[rstest]
    fn test_color_mode_equality() {
        assert_eq!(ColorMode::Light, ColorMode::Light);
        assert_eq!(ColorMode::Dark, ColorMode::Dark);
        assert_ne!(ColorMode::Light, ColorMode::Dark);
    }

    #[rstest]
    fn test_color_mode_copy() {
        let mode = ColorMode::Light;
        let copied = mode;
        assert_eq!(mode, copied);
    }

    #[rstest]
    fn test_color_mode_clone() {
        let mode = ColorMode::Dark;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[rstest]
    fn test_color_mode_debug() {
        assert_eq!(format!("{:?}", ColorMode::Light), "Light");
        assert_eq!(format!("{:?}", ColorMode::Dark), "Dark");
    }

    // ── Color mode state tests (require WASM or browser env) ──

    #[cfg(target_arch = "wasm32")]
    mod color_mode_state_tests {
        use super::*;

        #[rstest]
        fn test_provide_color_mode_initial_state() {
            let owner = Owner::new();
            owner.set();
            let state = provide_color_mode();
            assert_eq!(state.mode.get(), ColorMode::Light);
        }

        #[rstest]
        fn test_color_mode_toggle() {
            let owner = Owner::new();
            owner.set();
            let state = provide_color_mode();
            assert_eq!(state.mode.get(), ColorMode::Light);
            state.toggle.run(());
            assert_eq!(state.mode.get(), ColorMode::Dark);
            state.toggle.run(());
            assert_eq!(state.mode.get(), ColorMode::Light);
        }

        #[rstest]
        fn test_color_mode_set_mode() {
            let owner = Owner::new();
            owner.set();
            let state = provide_color_mode();
            state.set_mode.run(ColorMode::Dark);
            assert_eq!(state.mode.get(), ColorMode::Dark);
            state.set_mode.run(ColorMode::Light);
            assert_eq!(state.mode.get(), ColorMode::Light);
        }

        #[rstest]
        fn test_use_color_mode_without_provide_panics() {
            let owner = Owner::new();
            owner.set();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                use_color_mode();
            }));
            assert!(result.is_err());
        }
    }
}
