---
monochange: patch
---

#### Attest GitHub release archives

monochange's own GitHub release asset workflow now runs from tag or manual dispatch events instead of draft release creation events. This makes the workflow compatible with GitHub immutable releases, where assets should exist before the release is finalized and draft `release.created` events are not a reliable trigger.

**Before:**

```yaml
on:
  release:
    types: [created]
```

The workflow uploaded CLI archives and checksum files, but did not create first-class GitHub artifact attestations for the uploaded `.tar.gz` and `.zip` archives.

**After:**

```yaml
on:
  push:
    tags:
      - "v*"
  workflow_dispatch:
```

The release asset job now requests the minimum attestation permissions, downloads each uploaded archive back from the release, creates GitHub build-provenance attestations for those archive subjects, and verifies the attestations before triggering downstream package publishing.

Users can verify a published archive with:

```bash
gh attestation verify monochange-x86_64-unknown-linux-gnu-v1.2.3.tar.gz \
  --repo monochange/monochange
```
