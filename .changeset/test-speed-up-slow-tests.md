---
"monochange": patch
---

Improve the CLI progress integration test harness to reduce unnecessary waiting during TTY transcript collection.

Before:

- contributor test runs spent extra time waiting after streamed progress output finished

After:

- the TTY progress integration harness drains output as soon as the subprocess exits
- the streamed progress fixture uses a shorter delay while still exercising live output rendering
