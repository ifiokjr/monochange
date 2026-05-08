---
monochange: patch
monochange_publish: minor
---

# Move resume and dependency ordering to monochange_publish

Move resume/artifact logic (`read_publish_report_artifact`, `write_publish_report_artifact`, `ensure_publish_report_succeeded`, `resume_publish_requests`, `merge_publish_resume_report`) and dependency ordering (`order_release_requests_by_publish_dependencies`, `render_publish_dependency_cycle`) from `monochange` into `monochange_publish`.

This continues the Phase 2 crate boundary audit by removing more publish-orchestration helpers from the top-level `monochange` crate into the dedicated `monochange_publish` crate where they belong.
