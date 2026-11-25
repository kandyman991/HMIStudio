# Living development roadmap

Open [dashboard.html](dashboard.html) as a local file from your Git checkout. The dashboard works offline with JavaScript enabled. [Detailed milestone plan](../docs/MILESTONE_PLAN.md) is the Markdown view. Both files are generated and checked in.

Use Node.js 22 or later and full Git history.

```text
node roadmap/generate.mjs
node roadmap/generate.mjs --check --verify-migration
node --test roadmap/roadmap.test.mjs
```

## Update the roadmap

- Use one file per milestone under `milestones/`. Read [AGENTS.md](../AGENTS.md), verify main and open PR heads, and coordinate with whoever owns overlapping work. Update the assignment, branch, blockers and dated history at task start, handoff and completion proposal. Record a handoff even when the phase stays the same.
- Use `proposed` for work submitted for review. A draft or open PR and its CI are evidence about that proposal. Record `merged` after verifying the merge commit on the default branch, and `accepted` after the designated acceptance gates have evidence. Clear the active assignment after integration. A milestone can need a follow-up PR to record its merge. D-008 defines manual acceptance boundaries.
- If a PR changes head, record the new SHA and clear workflow results from older heads. Set each PR or issue's `observedAt` from the actual UTC observation. Record workflow run ID, name, state, conclusion and exact source SHA. Empty runs mean unrecorded evidence. GitHub can provide newer results. If a state cannot be verified, retain the prior observation with its original timestamp.
- Use `merged` or `accepted` dependency gates according to the actual prerequisite. Put unresolved owner decisions and scope questions in blockers. `counts: false` marks summary records that display alongside counted tasks and are excluded from progress and eligibility.
- Regenerate and stage both views with the milestone source. Review the input diff first, then verify both views. In a Git conflict, merge the per-milestone records and regenerate from the combined inputs. Existing PRs that edited the Markdown plan should port their changes to the corresponding source records when they rebase.
- Use `context.json` for roadmap-wide material. Keep the pinned migration snapshots as historical evidence. Engineering decisions and curated handoff are linked records. The roadmap is authoritative for current milestone state.

For live GitHub status, open the observed PR or issue link and verify the PR head in GitHub. In GitHub CLI, use `gh pr view` for the PR and head, `gh run list --commit SHA --event pull_request` for exact-head workflow status, and `gh issue view` for issue state. The browser uses the observations stored in the checkout.

## CI and privacy

Desktop CI's existing `changes` job runs the roadmap tests and compares both generated files byte-for-byte after normalizing checkout line endings. It checks the original source snapshots against Git history and validates integration evidence against the default branch. Required CI depends on that job and the other selected checks. Roadmap-only changes skip application jobs and installer generation. Build inputs, workflows, releases and manual packaging keep their existing routing.

GitHub identifies this repository as public. All content here, including assignments and history, must be suitable for public Git. Keep private source, chats, operational details, credentials and private evidence in their approved location. The dashboard operates offline with a restrictive content security policy. GitHub links navigate only on activation.

Private GitHub Pages requires an eligible organization-owned private or internal project repository on GitHub Enterprise Cloud. It is unavailable for this public personal repository. Use the local file. [GitHub Pages access control](https://docs.github.com/en/enterprise-cloud@latest/pages/getting-started-with-github-pages/about-privately-published-github-pages-sites)

The previous graphical roadmap was unavailable in the inspected Git history, workspace and referenced conversation. The original HMI editor visual prototype referenced by PR #13 is a separate artifact. This dashboard follows the documented roadmap requirements.

## Shared reservations and merge reconciliation

Before implementation, run the following from the checkout with GitHub access:

    python3 roadmap/claim.py list
    python3 roadmap/claim.py claim RUST-09 --owner Claude --branch feat/example --scope src/ docs/example.md
    python3 roadmap/claim.py release RUST-09 --owner Claude --branch feat/example

The example is illustrative, not an assignment. Scope entries are exact paths or directory prefixes ending in a slash. Copy the sorted scope from the claim into the assignment. A group ref is created atomically; if different groups race for overlapping paths, both must stop and coordinate before proceeding. Required CI also rejects overlapping reservation scopes. The live refs are visible in GitHub branches before either agent merges. Only release your own exact reservation after verifying merge or abandonment. This is coordination, not authorization against malicious repository writers.

An implementation PR must append real dated milestone history, record the matching branch and scope, and regenerate both outputs. A proposal that completes the milestone must name its own PR as completionPr; intermediate PRs stay in progress. If a required dedicated workflow applies, list its name in requiredWorkflows. The migration is exempt because its base lacks the roadmap. Documentation and canonical-data-only maintenance changes are exempt from reservation requirements; roadmap executable or CI changes are not. Existing branches must reconcile with the new default branch before integration.

Run python3 roadmap/sync.py to read GitHub's API and update changed PR/head/CI/issue observations. It verifies the actual GitHub merge commit against main, discards CI from obsolete heads, and only promotes an explicitly designated completion PR with successful required and observed workflows. It never records acceptance. Re-run the generator and submit a reviewed PR (lightweight independent consistency review for evidence-only changes under AGENTS.md; full technical review for executable/control changes). The workflow runs on trusted main pushes, six-hour schedules and manual dispatch. To automatically open reconciliation PRs, configure an approved dedicated repository-scoped ROADMAP_SYNC_TOKEN with contents and pull-request write permissions; it must trigger normal pull-request CI. Without it the job reports that publishing is blocked, and agents can reconcile locally with their existing authorized CLI. No credential is sent to the browser, no PR branch is executed with the token, no direct write to main is attempted. An existing reconciliation PR is not overwritten. Timestamps denote the last changed observation; unchanged snapshots do not generate endless PRs.

Parent and related-scope links separate duplicate RUST/REUSE scope from independently counted implementation slices. The parent remains the acceptance gate even when it is excluded from engineering progress. Open the [shared claim refs](https://github.com/kandyman991/HMIStudio/branches) for the current reservation state. The offline dashboard is a versioned snapshot, not a live hosted website.
