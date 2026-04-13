use monochange_core::HostedSourceAdapter;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;

pub(crate) fn hosted_source_adapter(provider: SourceProvider) -> &'static dyn HostedSourceAdapter {
	match provider {
		SourceProvider::GitHub => &monochange_github::HOSTED_SOURCE_ADAPTER,
		SourceProvider::GitLab => &monochange_gitlab::HOSTED_SOURCE_ADAPTER,
		SourceProvider::Gitea => &monochange_gitea::HOSTED_SOURCE_ADAPTER,
	}
}

pub(crate) fn configured_hosted_source_adapter(
	source: &SourceConfiguration,
) -> &'static dyn HostedSourceAdapter {
	hosted_source_adapter(source.provider)
}
