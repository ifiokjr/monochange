---
monochange: test
"@monochange/skill": test
---

# Validate generated release commits in PR CI

Pull requests now run release-state test and lint preflights after creating a local release commit, while generated release PRs skip those extra preflights.
