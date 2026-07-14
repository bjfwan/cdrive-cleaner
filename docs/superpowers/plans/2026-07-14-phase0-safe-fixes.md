# Phase 0 Safe Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Disable the untrusted in-app installer path and fix five isolated UI safety/correctness defects without colliding with the onboarding refactor.

**Architecture:** The updater becomes metadata-check plus an allowlisted external release-page handoff. UI guardrails are localized template/script changes, protected by dependency-free source assertions and the existing TypeScript/Vite build.

**Tech Stack:** Vue 3, TypeScript, Node built-in test runner, Tauri 2, Rust 2021.

## Global Constraints

- Do not modify onboarding behavior or its new help flow.
- Do not add an in-app installer until a trusted backend-owned digest and one-time verified install ticket exist.
- Do not introduce new npm dependencies.
- Preserve the existing version-check behavior.
- All user-facing updater links must resolve only to `https://github.com/bjfwan/cdrive-cleaner/releases` or a child path.

---

### Task 1: Fail-closed updater handoff

**Files:**
- Modify: `src/components/UpdateDialog.vue`
- Modify: `src/components/Settings.vue`
- Modify: `src/utils/updater.ts`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `tests/update-safety-source.test.mjs`

**Interfaces:**
- Consumes: `checkForUpdates(): Promise<UpdateInfo>` and the existing `UpdateInfo.download_url` returned by Rust.
- Produces: `open_url(url: String) -> Result<(), String>` restricted to the official release page.

- [ ] **Step 1: Write failing source and Rust unit tests**

Add Node assertions that the dialog no longer imports/calls `downloadUpdate`, `installUpdate`, or `resolveMirrorUrl`; the Tauri registry excludes `download_update` and `install_update`; Settings no longer renders mirror controls; and the dialog invokes `open_url` with the release URL. Add Rust tests for the release URL validator, including lookalike and metacharacter payloads.

- [ ] **Step 2: Verify the tests fail for the intended missing behavior**

Run `node --test tests/update-safety-source.test.mjs`. Expected: failures showing the unsafe updater imports/registrations and mirror UI still exist. Run Rust unit tests if a toolchain is available; otherwise record that platform verification is unavailable locally.

- [ ] **Step 3: Implement the minimal fail-closed behavior**

Simplify `UpdateDialog.vue` to open the official release page, remove renderer download/install helpers, remove the mirror settings block, unregister download/install commands, and replace `cmd /C start` with a non-shell launch after strict official-URL validation.

- [ ] **Step 4: Verify focused tests and build**

Run `node --test tests/update-safety-source.test.mjs` and `npm run build`. Expected: all Node tests pass and build exits 0.

### Task 2: Local UI safety guardrails

**Files:**
- Modify: `src/components/AiSuggestions.vue`
- Modify: `src/components/AiSuggestionCard.vue`
- Modify: `src/components/SystemReclaim.vue`
- Modify: `src/components/JunkCleanView.vue`
- Modify: `src/components/Privacy.vue`
- Test: `tests/ui-guardrails-source.test.mjs`

**Interfaces:**
- Consumes: existing AI suggestion action values, reclaim elevation state, and modal close event.
- Produces: no new public interfaces; only safer rendering and action availability.

- [ ] **Step 1: Write failing source tests**

Assert the AI spinner requires `state.analyzing`, non-migration AI actions cannot emit accept, reclaim buttons include the elevation guard, false migration-history copy is gone, junk feedback says it is local, and the privacy close button has `type="button"` plus an accessible name.

- [ ] **Step 2: Verify the tests fail for the intended defects**

Run `node --test tests/ui-guardrails-source.test.mjs`. Expected: each assertion fails against the current source for the described defect.

- [ ] **Step 3: Apply the minimal component changes**

Change only the relevant conditions, button state/copy, feedback copy, and accessibility attributes. Do not redesign the screens or route new destructive AI actions.

- [ ] **Step 4: Verify focused tests and build**

Run `node --test tests/ui-guardrails-source.test.mjs` and `npm run build`. Expected: all Node tests pass and build exits 0.

### Task 3: Integrated verification and review

**Files:**
- Review all files changed by Tasks 1-2.

**Interfaces:**
- Consumes: task test evidence and branch diff.
- Produces: a reviewed, merge-ready branch or an explicit list of unresolved blockers.

- [ ] **Step 1: Run all local verification**

Run `node --test tests/*.test.mjs`, `npm run build`, `git diff --check`, and `git status --short`.

- [ ] **Step 2: Review the complete branch diff**

Check that no onboarding files outside the updater settings block changed, no arbitrary execution command remains registered, and all copy matches actual behavior.

- [ ] **Step 3: Record platform limitation**

If Cargo is unavailable, explicitly state that Rust compilation and Windows behavior require CI verification before merge.

