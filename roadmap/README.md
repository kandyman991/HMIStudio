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
