use monochange_core::HostedSourceAdapter;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;

/// Returns the appropriate hosted source adapter for the given provider.
///
/// # Panics
///
/// Panics if the provider feature is not enabled at compile time.
pub(crate) fn hosted_source_adapter(provider: SourceProvider) -> &'static dyn HostedSourceAdapter {
	match provider {
		#[cfg(feature = "github")]
		SourceProvider::GitHub => &monochange_github::HOSTED_SOURCE_ADAPTER,
		#[cfg(not(feature = "github"))]
		SourceProvider::GitHub => {
			panic!("the `github` feature must be enabled to use GitHub as a source provider")
		}
		#[cfg(feature = "gitlab")]
		SourceProvider::GitLab => &monochange_gitlab::HOSTED_SOURCE_ADAPTER,
		#[cfg(not(feature = "gitlab"))]
		SourceProvider::GitLab => {
			panic!("the `gitlab` feature must be enabled to use GitLab as a source provider")
		}
		#[cfg(feature = "gitea")]
		SourceProvider::Gitea => &monochange_gitea::HOSTED_SOURCE_ADAPTER,
		#[cfg(not(feature = "gitea"))]
		SourceProvider::Gitea => {
			panic!("the `gitea` feature must be enabled to use Gitea as a source provider")
		}
	}
}

pub(crate) fn configured_hosted_source_adapter(
	source: &SourceConfiguration,
) -> &'static dyn HostedSourceAdapter {
	hosted_source_adapter(source.provider)
}
