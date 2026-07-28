# Blend System Specification Index

This index defines how the frozen Blend v2 behavioral baseline and the
experimental Blend v3 candidate specification compose into one normative system
specification.

## Specification set

1. [V2_SYSTEM_SPEC.md](V2_SYSTEM_SPEC.md) defines the inherited behavior of
   the frozen SDK-22 Blend v2 contracts and deployed legacy emitter boundary.
2. [V3_SYSTEM_SPEC.md](V3_SYSTEM_SPEC.md) defines only the v3 additions,
   replacements, extensions, approved exceptions, and safety fixes.
3. [VALUATION_ADAPTER.md](VALUATION_ADAPTER.md) is the normative component
   specification for the immutable v3 production LP-valuation boundary.
4. [V2_PARITY.md](V2_PARITY.md) is the concise cross-reference matrix. It does
   not replace the normative specifications.
5. [MIGRATION_TEST_PLAN.md](MIGRATION_TEST_PLAN.md) defines the executable
   acceptance matrix for the composed specification.

[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) records current progress
but is non-normative and cannot authorize a protocol change.

Declarative behavioral requirements and the terms `MUST`, `MUST NOT`,
`SHOULD`, and `MAY` in the normative specifications are acceptance criteria.

## Inheritance rule

Blend v3 inherits every requirement and executable behavior in the frozen v2
baseline unless `V3_SYSTEM_SPEC.md` explicitly classifies a requirement as:

- **Added:** behavior with no v2 equivalent.
- **Replaced:** v3 behavior that supersedes a named v2 rule.
- **Extended:** a v2 rule generalized only as stated for v3.
- **Safety fix:** an intentional, approved deviation from v2.
- **Approved exception:** an intentional economic or administrative deviation
  approved in the v3 specification.

Omission from the v3 specification means inheritance, not unspecified behavior.
Restating an inherited rule for context does not create a v3 difference.

## Authority and conflict resolution

Apply the following order:

1. An explicit v3 addition or override, including a component requirement
   incorporated by `V3_SYSTEM_SPEC.md`, governs the v3 candidate.
2. Otherwise, the frozen v2 executable behavior governs.
3. `V2_SYSTEM_SPEC.md` documents that executable baseline.

If the written v2 specification conflicts with the frozen SDK-22 contracts,
tests, or deployed legacy-emitter behavior, the executable baseline wins and
the document MUST be corrected. A conflict MUST NOT be resolved by silently
creating a new v3 policy.

The frozen source provenance and toolchains are recorded in
[BASELINE.md](BASELINE.md). New economic behavior still requires a separately
approved v3 change.
