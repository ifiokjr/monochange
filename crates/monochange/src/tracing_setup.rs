use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::fmt::format::FmtSpan;

/// Initialize the tracing subscriber for CLI diagnostics.
///
/// Priority:
/// 1. `RUST_LOG` environment variable (full precedence when set)
/// 2. `log_level` parameter from `--log-level` CLI flag
/// 3. No subscriber installed (silent, near-zero overhead)
pub(crate) fn init_tracing(log_level: Option<&str>) {
	let filter = match EnvFilter::try_from_default_env() {
		Ok(env_filter) => env_filter,
		Err(_) => {
			match log_level {
				Some(level) => EnvFilter::new(level),
				None => return,
			}
		}
	};

	let subscriber = fmt::Subscriber::builder()
		.with_env_filter(filter)
		.with_span_events(FmtSpan::CLOSE)
		.with_target(true)
		.with_writer(std::io::stderr)
		.finish();

	let _ = tracing::subscriber::set_global_default(subscriber);
}
