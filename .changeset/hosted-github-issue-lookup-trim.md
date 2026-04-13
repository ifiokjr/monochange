---
monochange_github: patch
---

Trim the hosted GitHub review-request payload by deriving closing and referenced issue numbers from pull request bodies instead of requesting `closingIssuesReferences` from GraphQL. This keeps provider batching in place while shaving work from the real non-dry-run `mc release` enrichment path, and also hardens the hosted benchmark fixture helper for machines with `commit.gpgsign=true`.
