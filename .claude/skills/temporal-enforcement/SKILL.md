---
name: temporal-enforcement
description: >
  Temporal layer enforcement and validation.
---
# Temporal Enforcement Skill

## Core Invariant
NO document enters the vault without `observed_at`. Enforced at the
TemporalGate in `crates/mkb-vault/src/temporal_gate.rs` (Rust) and
`python/mkb_ai/temporal/gate.py` (Python ingestion).

## Required Fields (ALL documents)
- `observed_at: datetime` — MANDATORY
- `valid_until: datetime` — MANDATORY, computed by decay if not provided
- `temporal_precision: enum` — MANDATORY, defaults to "inferred"

## Extraction Priority Chain
1. Source API timestamp (exact)
2. User-provided --observed-at flag (user-specified)
3. File metadata / filename pattern (approximate)
4. AI temporal inference (inferred, confidence penalty -0.15)
5. REJECTION (logged to ingestion/rejected/)

## Key Tests
- `test_temporal_gate_rejects_no_timestamp`
- `test_temporal_gate_accepts_explicit_timestamp`
- `test_decay_model_halves_confidence_at_half_life`
- `test_staleness_sweep_archives_expired`
