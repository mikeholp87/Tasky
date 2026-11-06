# Diagnostic findings

Recorded before fixes; validation and disposition are updated as work proceeds.
Severity: CRITICAL, HIGH, MEDIUM, LOW, INFO. Measurements are reported only when
collected, with scope and limitations.

## ALERT-001

- Severity: MEDIUM
- Subsystem: alert duration and process identity
- Description: timed alerts use wall-clock timestamps; process rules carry an
  episode between different process lifetimes and rule edits.
- Reproduction: hold a CPU rule true, advance the wall clock; or replace a
  matched process with the same PID and a different start time before the
  duration expires. The old episode can fire for the replacement process.
- Root cause: `AlertEvaluator` stores only rule ID, wall-clock start, and fired
  flag; observations discard process start time and the prior rule definition.
- User impact: premature/missed notifications and incorrect process attribution.
- Fix: pending monotonic evaluation and episode identity regression tests.
- Validation method: deterministic clock, PID reuse, rule edit and cooldown tests.

## PROC-001

- Severity: HIGH
- Subsystem: process details and selection
- Description: frontend selection and details history follow PID alone. A reused
  PID can inherit selection, old static details and history; actions can then
  target the new live process without the user selecting its lifetime.
- Reproduction: select/open PID N, replace the metrics row with PID N at a new
  start time, deliver more ticks, inspect selection/history and action target.
- Root cause: PID-only UI state, virtual row keys and live subscription matching.
- User impact: mixed data and unsafe selection semantics.
- Fix: pending lifetime identity handling.
- Validation method: injected tick and deferred IPC regression tests.

## SEC-001

- Severity: HIGH
- Subsystem: native process actions
- Description: the action start-time check compares against the last engine
  snapshot, not the live native process. While paused or between scans, a reused
  PID can pass the cached check.
- Reproduction: inspect `perform` in process_control/mod.rs; keep the engine
  paused after a process exits, then reuse its PID (deterministic native guard
  coverage is needed; forcing OS PID reuse is unsafe/unreliable).
- Root cause: cached identity is assumed sufficient before PID-based native APIs.
- User impact: action could affect a replacement process.
- Fix: pending live identity guard; any remaining OS race will be stated.
- Validation method: stale lifetime checks plus owned-child native action tests.

## PERF-001

- Severity: MEDIUM
- Subsystem: production frontend startup
- Description: main production JavaScript chunk is 601.89 kB / 192.12 kB gzip,
  above the build warning threshold. Benchmark interface is eagerly imported.
- Reproduction: `npm run build`; inspect App.tsx imports.
- Root cause: all application pages enter the initial bundle.
- User impact: optional benchmark UI contributes to initial parsing/loading.
- Fix: pending measured code-splitting assessment.
- Validation method: before/after production bundle sizes and route tests.

## BASE-001

- Severity: INFO
- Subsystem: build and tests
- Description: all existing default checks pass; hardware tests are opt-in.
- Reproduction: commands listed in DIAGNOSTIC_BASELINE.md.
- Root cause: not applicable.
- User impact: no initial build blocker.
- Fix: none.
- Validation method: baseline command results.
