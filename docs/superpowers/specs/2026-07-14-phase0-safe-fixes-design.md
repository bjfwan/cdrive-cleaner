# Phase 0 Safe Fixes Design

## Context

The deep audit identified several severe backend risks and a smaller set of isolated UI correctness issues. Another worktree is already refactoring onboarding, so this change must avoid its files except where a non-overlapping settings block is required to remove an unsafe updater control.

## Goal

Ship only changes that can be implemented and verified with low regression risk now: remove the unsafe in-app installer execution path and correct five misleading or unsafe UI states. Defer changes that require a Windows-only architectural migration or fault-injection coverage.

## Update safety design

The current renderer can choose an arbitrary mirror URL, ask Rust to download bytes, and then ask Rust to execute an arbitrary existing path. The project has no trusted digest, publisher pinning, or backend-issued install capability. HTTPS and an origin allowlist alone would not make mirror content trustworthy.

Phase 0 therefore fails closed:

- Keep the GitHub release metadata check.
- Replace “download and install inside the app” with “open the official project release page”.
- Only allow the backend `open_url` command to open `https://github.com/bjfwan/cdrive-cleaner/releases` and its child paths.
- Launch the validated URL without `cmd /C` so shell metacharacters are never interpreted.
- Remove `download_update` and `install_update` from Tauri's command registry. The renderer must not be able to invoke them.
- Remove mirror selection from Settings and remove unused renderer updater helpers.

The existing downloader implementation may remain temporarily as unreachable code if deleting it would create unnecessary churn. A later signed-updater project can replace it with a backend-owned manifest, SHA-256 verification, one-time install tickets, and Authenticode publisher verification.

## UI guardrail design

- AI suggestions show the spinner only while `state.analyzing` is true; an empty idle result shows the empty state.
- Only explicit migration suggestions can emit the migration accept action. Delete/review suggestions are disabled and direct users to the appropriate review surface.
- System reclaim actions requiring elevation are disabled when the app is not elevated, and the confirmation copy must not promise migration-history persistence that does not exist.
- Junk-rule feedback says it is stored locally rather than promising a future product change.
- The privacy dialog close control has an explicit button type and accessible name.

## Testing

- Add source-level Node tests for template guardrails and update-surface removal, run with Node's built-in test runner. These tests are intentionally dependency-free because the project has no frontend test framework and `package.json` is being edited in the onboarding worktree.
- Add Rust unit tests for the official release URL validator. The tests must cover the exact release root, child paths, lookalike hosts/paths, non-HTTPS URLs, and shell-metacharacter payloads.
- Run the Node guardrail tests and `npm run build` locally.
- Rust compilation/tests cannot be claimed locally unless a Rust toolchain is available. Windows CI remains the authoritative platform check.

## Deferred work

- Signed in-app updater and verified installer capability.
- Canonical backend authorization for deletion/migration/reclaim.
- Persisted migration and rollback sagas.
- CSP rollout and data-directory migration.
- Full AI information-architecture removal or repositioning.

