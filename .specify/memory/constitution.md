<!--
  Sync Impact Report
  ===================
  Version change: 0.0.0 → 1.0.0 (initial ratification)
  Modified principles: N/A (initial creation)
  Added sections:
    - Core Principles (5): Modular-First, Test-First, Simplicity,
      Content Integrity, Observability
    - Technology Standards
    - Development Workflow
    - Governance
  Removed sections: N/A
  Templates requiring updates:
    - .specify/templates/plan-template.md ✅ compatible (Constitution Check gate exists)
    - .specify/templates/spec-template.md ✅ compatible (requirements align)
    - .specify/templates/tasks-template.md ✅ compatible (phase structure supports TDD)
  Follow-up TODOs: None
-->

# Librarian Constitution

## Core Principles

### I. Modular-First

Every feature MUST start as a standalone, self-contained module.
Modules MUST be independently testable and documented with a clear
single purpose. No module may exist purely for organizational grouping
— it MUST deliver concrete functionality. Inter-module dependencies
MUST be explicit and minimal; prefer composition over coupling.

### II. Test-First (NON-NEGOTIABLE)

All new functionality MUST have tests written before implementation.
The Red-Green-Refactor cycle is strictly enforced:

1. Write a failing test that defines the expected behavior.
2. Implement the minimum code to make the test pass.
3. Refactor while keeping tests green.

Tests MUST cover: unit behavior, module contracts, and integration
points between modules. Skipping tests requires explicit justification
documented in the relevant spec or plan.

### III. Simplicity & YAGNI

Start with the simplest solution that satisfies current requirements.
Do NOT build for hypothetical future needs. Complexity MUST be
justified — if a simpler alternative exists, use it. Three similar
lines of code are preferable to a premature abstraction. Every
abstraction layer MUST earn its existence by solving a demonstrated,
repeated problem.

### IV. Content Integrity

Data managed by Librarian MUST remain accurate, consistent, and
retrievable. All write operations MUST be validated before persistence.
Data transformations MUST be reversible or auditable. Storage formats
MUST prioritize durability and portability over performance unless
performance requirements are explicitly specified. No silent data loss
is acceptable under any condition.

### V. Observability

All operations MUST produce structured, queryable logs. Errors MUST
surface with sufficient context to diagnose without reproducing. Text
I/O (stdin/stdout/stderr) MUST follow predictable conventions: normal
output to stdout, errors to stderr. Support both human-readable and
JSON output formats where applicable.

## Technology Standards

- **Runtime**: Bun (preferred over Node.js for all operations)
- **Testing**: `bun test` with the built-in test runner
- **Server**: `Bun.serve()` with HTML imports (no Express, no Vite)
- **Database**: `bun:sqlite` for local storage (no external drivers)
- **Package management**: `bun install` exclusively
- **Frontend**: HTML imports with React, bundled by Bun
- **Environment**: Bun auto-loads `.env` — no dotenv

All technology choices MUST align with the project CLAUDE.md. When
a Bun-native API exists for a capability, it MUST be used over
third-party alternatives.

## Development Workflow

- Every feature MUST begin with a specification (`/speckit-specify`)
  before any implementation.
- Implementation follows the spec-plan-tasks pipeline: specify first,
  plan second, generate tasks third, implement last.
- Code review MUST verify compliance with this constitution's
  principles before approval.
- Each commit SHOULD represent a single logical change. Commits MUST
  NOT introduce failing tests.
- Modules MUST be deliverable as independent increments — no feature
  requires all modules to be complete before it delivers value.

## Governance

This constitution is the highest authority governing development
practices in the Librarian project. It supersedes all other guidance
when conflicts arise.

**Amendment procedure**:

1. Propose the change with rationale in writing.
2. Document the impact on existing principles and templates.
3. Update the constitution with a version bump following semantic
   versioning (MAJOR for principle removals/redefinitions, MINOR for
   additions/expansions, PATCH for clarifications/typos).
4. Propagate changes to all dependent templates and specs.

**Compliance**: All pull requests and reviews MUST verify adherence
to these principles. Violations MUST be resolved before merge unless
an explicit, documented exception is granted.

**Version**: 1.0.0 | **Ratified**: 2026-04-17 | **Last Amended**: 2026-04-17
