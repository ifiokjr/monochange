#!/usr/bin/env python3
"""Fix remaining async issues in monochange_github"""

with open('crates/monochange_github/src/lib.rs', 'r') as f:
    content = f.read()

# 1. Convert remaining sync functions to async
replacements = [
    # publish_release_pull_request
    ('pub fn publish_release_pull_request(', 'pub async fn publish_release_pull_request('),
    
    # sync_retargeted_releases
    ('pub fn sync_retargeted_releases(', 'pub async fn sync_retargeted_releases('),
    
    # lookup_existing_pull_request
    ('fn lookup_existing_pull_request(', 'async fn lookup_existing_pull_request('),
    
    # git_checkout_branch
    ('fn git_checkout_branch(root: &Path, branch: &str)', 'async fn git_checkout_branch(root: &Path, branch: &str)'),
    
    # git_stage_paths
    ('fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf])', 'async fn git_stage_paths(root: &Path, tracked_paths: &[PathBuf])'),
    
    # release_path_requires_staging
    ('fn release_path_requires_staging(root: &Path, path: &Path)', 'async fn release_path_requires_staging(root: &Path, path: &Path)'),
    
    # git_path_is_tracked
    ('fn git_path_is_tracked(root: &Path, path: &Path)', 'async fn git_path_is_tracked(root: &Path, path: &Path)'),
    
    # git_path_is_ignored
    ('fn git_path_is_ignored(root: &Path, path: &Path)', 'async fn git_path_is_ignored(root: &Path, path: &Path)'),
    
    # maybe_replace_release_pull_request_commit_with_verified_github_commit
    ('fn maybe_replace_release_pull_request_commit_with_verified_github_commit(', 'async fn maybe_replace_release_pull_request_commit_with_verified_github_commit('),
    
    # join_existing_pull_request_lookup
    ('fn join_existing_pull_request_lookup(', 'async fn join_existing_pull_request_lookup('),
]

for old, new in replacements:
    content = content.replace(old, new)

# 2. Add .await to async function calls that don't already have it

# git_current_branch calls without .await
content = content.replace('git_current_branch(root).as_deref()', 'git_current_branch(root).await.as_deref()')

# run_command calls without .await (in functions that were just made async)
# These are tricky because run_command is called with git_..._command which returns Command
# So the pattern is: run_command(git_..._command(...), ...) without .await
# We need to look for run_command( that doesn't already have .await

# git_command_output without .await  
# Line 1925: git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative]).map_err
# We need to add .await after git_command_output(...)
content = content.replace(
    'git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative])\n\t\t.map_err',
    'git_command_output(root, &["ls-files", "--error-unmatch", "--", &relative])\n\t\t.await\n\t\t.map_err'
)

# git_command_output for check-ignore
content = content.replace(
    'git_command_output(root, &["check-ignore", "-q", "--", &relative]).map_err',
    'git_command_output(root, &["check-ignore", "-q", "--", &relative])\n\t\t.await\n\t\t.map_err'
)

# run_command calls in git_checkout_branch, git_stage_paths
content = content.replace(
    'run_command(\n\t\tgit_checkout_branch_command(root, branch),\n\t\t"prepare release pull request branch",\n\t)',
    'run_command(\n\t\tgit_checkout_branch_command(root, branch),\n\t\t"prepare release pull request branch",\n\t).await'
)

content = content.replace(
    'run_command(\n\t\tgit_stage_paths_command(root, &stageable_paths),\n\t\t"stage release pull request files",\n\t)',
    'run_command(\n\t\tgit_stage_paths_command(root, &stageable_paths),\n\t\t"stage release pull request files",\n\t).await'
)

# In publish_release_pull_request, add .await to git_* calls
content = content.replace('git_checkout_branch(root, &request.head_branch)?', 'git_checkout_branch(root, &request.head_branch).await?')
content = content.replace('git_stage_paths(root, tracked_paths)?', 'git_stage_paths(root, tracked_paths).await?')
content = content.replace('git_commit_paths(root, &request.commit_message, no_verify)?', 'git_commit_paths(root, &request.commit_message, no_verify).await?')
content = content.replace('git_head_commit(root)?', 'git_head_commit(root).await?')
content = content.replace('git_push_branch(root, &request.head_branch, no_verify)?', 'git_push_branch(root, &request.head_branch, no_verify).await?')

# maybe_replace_release_pull_request_commit_with_verified_github_commit call
content = content.replace(
    'maybe_replace_release_pull_request_commit_with_verified_github_commit(\n\t\t\tsource,\n\t\t\trequest,',
    'maybe_replace_release_pull_request_commit_with_verified_github_commit(\n\t\t\tsource,\n\t\t\trequest,'
)
# Add .await to the specific call - need to find the exact pattern
# Let me search for this specific call

# lookup_existing_pull_request call in publish_release_pull_request
# line ~1147: thread::spawn(move || lookup_existing_pull_request(&lookup_source, &lookup_request));
# Replace with tokio::task::spawn
content = content.replace(
    'thread::spawn(move || lookup_existing_pull_request(&lookup_source, &lookup_request))',
    'tokio::task::spawn(async move { lookup_existing_pull_request(&lookup_source, &lookup_request).await })'
)

# Fix JoinHandle -> tokio::task::JoinHandle in join_existing_pull_request_lookup
content = content.replace(
    'thread::JoinHandle<MonochangeResult<Option<GitHubExistingPullRequest>>>',
    'tokio::task::JoinHandle<MonochangeResult<Option<GitHubExistingPullRequest>>>'
)

# Fix runtime.block_on(async { ... }) patterns in sync_retargeted_releases
# sync_retargeted_releases has: runtime.block_on(async { sync_retargeted_releases_with_client(...) })
content = content.replace(
    'sync_retargeted_releases_with_client(\n\t\t\t&client,\n\t\t\tsource,\n\t\t\ttag_results,\n\t\t\t\tdry_run,\n\t\t\t)',
    'sync_retargeted_releases_with_client(\n\t\t\t&client,\n\t\t\tsource,\n\t\t\ttag_results,\n\t\t\tdry_run,\n\t\t\t).await'
)

# Fix lookup_existing_pull_request_with_client call in lookup_existing_pull_request
content = content.replace(
    'lookup_existing_pull_request_with_client(\n\t\t&client,\n\t\trequest,\n\t)\n\t\t.await',
    'lookup_existing_pull_request_with_client(\n\t\t&client,\n\t\trequest,\n\t)\n\t.await'
)

# Also fix any runtime.block_on(async { }) wrapper around lookup_existing_pull_request
# This is in publish_release_pull_request around line 1147
content = content.replace(
    "let runtime = github_runtime()?;\n\truntime.block_on(async {\n\t\tlet client = github_client_from_env(source)?;\n\t\tlookup_existing_pull_request_with_client(\n\t\t\t&client,\n\t\t\trequest,\n\t\t)\n\t\t.await\n\t})",
    "let client = github_client_from_env(source)?;\n\tlookup_existing_pull_request_with_client(\n\t\t&client,\n\t\trequest,\n\t)\n\t.await"
)

# Fix release_path_requires_staging calls
content = content.replace('git_path_is_tracked(root, path)?', 'git_path_is_tracked(root, path).await?')
content = content.replace('git_path_is_ignored(root, path)?', 'git_path_is_ignored(root, path).await?')

# Fix join_existing_pull_request_lookup - handle.await instead of handle.join()
content = content.replace(
    'handle.join().map_err(|_| {\n\t\tMonochangeError::Config("failed to join GitHub pull request lookup thread".to_string())\n\t})?',
    'handle.await.map_err(|_| {\n\t\tMonochangeError::Config("failed to join GitHub pull request lookup task".to_string())\n\t})?'
)

# Fix sync_retargeted_releases - remove runtime wrapper
content = content.replace(
    '\tlet runtime = github_runtime()?;\n\truntime.block_on(async {\n\t\tlet client',
    '\tlet client'
)

# Remove closing brace of block_on wrapper for sync_retargeted_releases
content = content.replace(
    '\t\t)\n\t\t.await\n\t})',
    '\t\t).await'
)

with open('crates/monochange_github/src/lib.rs', 'w') as f:
    f.write(content)

print("Done")
