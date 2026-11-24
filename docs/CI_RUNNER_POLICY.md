# CI runner and build policy

Public successor policy, not the legacy private repository's runner contract.

- Standard GitHub-hosted Windows/Linux runners for desktop CI. Never execute untrusted public pull requests on the Legion or other private self-hosted hosts. Any future trusted hardware workflow requires separate owner approval and isolation.
- Use pull_request (not pull_request_target for untrusted builds), least privilege, no secrets in PR jobs, bounded timeouts and short artifact retention. Artifacts are build outputs, not engineering memory.
- Batch corrections and run targeted preflight before pushing. Keep cancel-in-progress for superseded runs. Do not repeatedly rerun unchanged failing jobs without evidence.
- Both Windows and Linux builds are currently routine on this public repository; do not transplant private-repo manual Windows rules without a new decision.
- Commit dependency lockfiles and require npm ci and Cargo locked resolution before milestone acceptance. PR #1 now configures locked installs, tests and packaging and rejects lockfile changes during CI. Predecessor 0f2dddb passed both locked desktop jobs; verify every subsequent final head.
- Verify exact final PR head, independent review and applicable dependency checks. Build/package success is not install/launch/close or hardware acceptance. D-008 defers routine manual acceptance to designated milestones; installer generation is required when packaging changes. Do not skip required automated checks.
- Avoid needless artifact uploads and expensive builds for docs-only housekeeping when safe; never use skip-ci to circumvent required exact-head checks. Retention currently seven days for installers.
- Snapshot workflow is read-only, never uploads artifacts or writes refs. Metadata is evidence, not authority.
