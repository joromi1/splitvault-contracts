# Add no-op coverage for repeated payment-condition confirmations (#277)

## Summary

`confirm_condition` previously set `condition_met` and appended an `oracle_ok` audit entry on **every** invocation,
including repeats. This change defines the repeated-call policy explicitly as an **authenticated no-op**: the oracle
still authenticates, but a condition that is already met is not re-persisted and produces no duplicate audit-trail
transition entry or event.

## What changed

- **`contracts/split/src/lib.rs` — `confirm_condition`**
  - After `require_auth()` and the existing paused/disputed/oracle guards, the function returns early when
    `invoice.condition_met` is already `true`.
  - The no-op happens before `save_invoice` and `append_audit_entry`, so repeated authorized calls cannot add a
    second `oracle_ok` row or emit a new event. Audit consumers can now treat `oracle_ok` as a single transition
    marker identifying the actual condition-met transition.
  - Authorization is not weakened: repeats must still come from the configured oracle.

- **`contracts/split/src/test_confirm_condition.rs`** (new, registered in `lib.rs` under `#[cfg(test)]`)
  - `test_confirm_condition_appends_one_transition_entry` — first confirmation sets `condition_met`, appends exactly
    one `oracle_ok` entry with the expected actor and ledger timestamp.
  - `test_repeated_confirm_condition_is_a_noop` — two further confirmations at later ledger timestamps leave the
    audit log length, its single timestamp and the event count unchanged.
  - `test_repeat_noop_is_scoped_to_the_invoice` — repeats on one invoice do not suppress the first confirmation on a
    different invoice.

## Acceptance criteria addressed

- **Repeated authorized confirmation follows a specified no-op policy** — documented in code and enforced by the early
  return after authentication.
- **No duplicate transition entry** — `append_audit_entry` is not reached on repeats; covered by the regression tests.

## Validation note

The repository does not currently build in this environment, for reasons unrelated to this change: the workspace lists a
missing `tests/e2e` member, `contracts/dashboard/src/lib.rs` contains duplicated helper definitions and an out-of-scope
`amount`/`total` reference, and `contracts/split/src/lib.rs` has 100+ pre-existing compile errors in test configuration
(undefined `SplitVaultClient`, `String::concat` on the pinned SDK, moved `token`, etc.). No Rust source outside the
confirmation path was modified to work around this, and all temporary local scaffolding used to investigate was
removed. The new tests are ready to run once the pre-existing build breakage is resolved.

## Backwards compatibility

Only the repeated-call path changes; the first confirmation behaves exactly as before. No storage layout, ABI or event
schema changes.

closes #277
