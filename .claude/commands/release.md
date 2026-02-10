---
description: Prepare a new release
---
Prepare a release using the @release-engineer agent.

Version: $ARGUMENTS

Steps:
1. Verify main branch CI is green
2. Run full audit: /audit all
3. Bump version numbers
4. Generate changelog
5. Create release PR
