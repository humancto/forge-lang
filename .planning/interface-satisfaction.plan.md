# Plan: Interface Satisfaction Checking (complete)

## Goal

Upgrade the existing interface satisfaction checking to validate method signatures (param count, return type), not just name presence.

## Current State

- `check_interface_satisfaction()` exists at typechecker.rs:618
- `InterfaceMethod` stores `name`, `param_count`, `return_type`
- Currently only checks if a field or function with the right name exists
- Does NOT validate param count or return type
- Two existing tests: `interface_satisfaction_pass`, `interface_satisfaction_fail`

## Changes

### 1. Validate method signatures in `check_interface_satisfaction`

**src/typechecker.rs** — `check_interface_satisfaction()`:

- When a matching function is found, check:
  - `param_count` matches (accounting for `self` parameter if applicable)
  - `return_type` is compatible (if both are specified)
- Emit specific warnings: "method 'X' has N params but interface requires M"
- Emit: "method 'X' returns T but interface expects U"

### 2. Tests

- Interface method with wrong param count → warning
- Interface method with wrong return type → warning
- Interface method with correct signature → no warning
- Existing tests still pass

## Risk Mitigation

- Advisory warnings only (not errors)
- Existing programs unaffected (no breaking changes)
- Single file change (typechecker.rs)

## Rollback

Revert typechecker changes.
