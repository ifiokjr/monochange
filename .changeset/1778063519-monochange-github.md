---
monochange_github:
  type: fix
---

# Support verified release commits on Windows

Verified release pull-request commits now build on Windows by avoiding Unix-only file permission APIs outside Unix targets. On Unix platforms, executable files still retain executable Git blob modes; on Windows, regular file blobs use the portable 100644 mode so the GitHub release workflow can compile across macOS, Linux, and Windows.
