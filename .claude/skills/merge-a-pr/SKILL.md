---
name: merge-a-pr
description: When work on an issue in a branch with a PR is completed and the human asks to merge the PR
---

When closing work done in a PR for the project:

## Checks

Double-check the points in the original issue were addressed.

Check that documentation was updated to reflect the changes.

Check that integration tests were expanded to cover the changes.

## Clean-up prior to Merge

Check there are no uncommitted changes or files present locally that are not in revision
control or not ignored. examples would be .profraw profiling files, .o object files from
working on an issue, other files created to debug issues.

That could represent things forgotten, or the user wants to carry over to other
work. Warn the user before causing any change that could lose it.

Check there is no remaining debugging code that was added while working on the issue. Often it's
marked with a comment containing "DEBUG:".

Check there is no dead-code.

Run:
- make fmt
- make clippy
- make test

Clean any files created in temporary directories such as "/tmp" or sub-folders not in version control.

Check for C++ build artifacts left around: `.profraw` files, `server/cpp/build/`, `clients/cpp/build/`.

## Merge the PR

Merge the PR using gh, if it returns an error check to see if the user has already merged it
via the GH user interface or some similar method.

If the user has already merged it, report that, but not as an error and consider the PR 
correctly merged and move on.

Ensure that the remote branch is also deleted along with the local branch.

## Post-Merge: Verify master CI

After merging, the PR is NOT considered successfully merged until the
corresponding CI run on master is also green.

1. Check out master and `git pull`.
2. Verify the merge commit from the PR is present in the log.
3. Monitor the CI run triggered by the merge on master (`gh run list --branch master --limit 1`).
4. Wait for the CI run to complete. If it fails:
   - Analyse the failure logs to determine if the merge caused it.
   - If the merge caused the failure, immediately fix it on a new branch,
     create a PR, and get it merged. Do NOT leave master broken.
   - If the failure is pre-existing (unrelated to the merge), note it
     but still investigate — a broken master blocks all other work.
5. Only after master CI is green, proceed to cleanup.

**A merged PR with a red master CI is not done.** Go back and fix it.

## Cleaning up

Check that all checks pass by running `make test`.
Delete the feature branch that was merged, locally and remote.
