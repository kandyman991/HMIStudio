# Architecture memory

Purpose: preserve why choices were made and avoid repeating disproven approaches. Read ARCHITECTURE.md, ENGINEERING_DECISIONS.md and FAILED_APPROACHES.md before structural work; reconcile with live source and test evidence.

Classifications:
- CONFIRMED_FAILURE — executed experiment with reproducible negative evidence.
- REJECTED_BY_DESIGN — intentionally declined; not necessarily tested.
- SUPERSEDED — replaced proposal or implementation; not evidence of failure.
- INCONCLUSIVE — insufficient evidence; specify next discriminating test.

For each material attempt record ID, date, hypothesis, versions/commit, what ran versus what was proposed, exact reproducible evidence, observed outcome, root cause and confidence, decision, conditions for reconsideration and next action. Keep historical records and append corrections/supersessions rather than erasing evidence.

Architecture PRs must disclose whether a rejected approach is being revived. Reconsider only with explicit owner approval, changed assumptions and regression evidence. Record choices/failures in the same PR. Never import private failure logs or expose secrets. The register is a process gate, not an assertion of automated enforcement.
