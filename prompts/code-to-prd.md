# Generate Comprehensive Technical Documentation for a Software Project

You are a technical documentation specialist. Your task is to produce a complete, exhaustive technical reference for a software project by reading its source code and producing a series of numbered markdown documents. Every statement you write must be derived directly from the code — no assumptions, no aspirations, no opinions. Document WHAT IS, not what should be.

## PROJECT TO DOCUMENT

[Provide: repo path, language, brief one-sentence description of what it does]

## OUTPUT FORMAT

Produce a numbered series of markdown files (01_ through N_) covering the domains listed below. Each document must:

1. Start with a top-level heading matching the topic
2. List "Source files covered:" at the top — every source file the document draws from
3. Use numbered sections (## 1. ..., ## 2. ...) with subsections (### 1.1, ### 1.2)
4. Use markdown tables for all structured data (schemas, parameters, configs, function signatures, enums, constants)
5. Include actual code snippets only when the exact syntax matters (hash formulas, SQL DDL, CLI commands, config formats)
6. Reference file paths as `path/to/file.py` inline whenever making a claim about code behavior
7. Cross-reference other documents in the series where relevant: "See [04_database_schema.md](04_database_schema.md)"

## DOCUMENTS TO PRODUCE

### 01_system_overview.md

High-level architecture document. Must include:

- One-paragraph summary of what the system does and how it's used
- Architecture diagram (describe as table: process/port/technology/purpose)
- Full technology stack table (layer → technology with version constraints)
- Complete list of all public APIs/tools/endpoints with one-line descriptions, grouped by domain
- All entry points (console scripts, server commands, CLI invocations)
- Project directory structure (what gets created at runtime, where data lives)
- Storage tier classification if applicable (what's sacred vs regenerable vs ephemeral)
- Exception/error hierarchy (base class → specialized classes with descriptions)
- Summary of every major subsystem (one paragraph each explaining what it does, key algorithms, inputs/outputs)

### 02_source_code_map.md

Complete file tree and dependency graph. Must include:

- Full file tree with one-line descriptions for EVERY file (format: `path/to/file.py  # One-line description of what this module does`)
- Module dependency graph showing which modules import what (use indented arrow notation: `module_a -> module_b (imported_names)`)
- Entry point traces (from main → through imports → to leaf modules)
- Build/package configuration (what gets included in builds, install targets)

### 03_configuration.md

All configuration options. Must include:

- Config file location and format
- Every config key with: name, type, default value, valid range, description
- Environment variables if any
- Config validation rules
- How config is loaded and merged (file vs env vs defaults)

### 04_database_schema.md

Complete database documentation. Must include:

- Connection management (how connections are created, pooling, pragmas, extensions)
- Schema versioning strategy
- EVERY table with EVERY column: name, type, constraints (PK/FK/NOT NULL/DEFAULT/UNIQUE), description
- All indexes: name, table, columns, purpose
- All migrations: version number, what changes, algorithm
- Query helper functions if any
- Virtual tables, extensions, special features

### 05_[domain_specific_subsystem].md (one per major subsystem)

Deep-dive into each major subsystem. For each, include:

- All public functions/classes with full signatures (parameters, types, return types)
- Algorithm descriptions — not just "it sorts" but which algorithm, what steps, what the complexity is
- Data models (Pydantic models, dataclasses, TypedDicts) with all fields
- Formulas if any (hash computation preimages, scoring functions, normalization)
- Error handling (what exceptions, when raised, what recovery)
- State machines if any (states, transitions, triggers)
- Integration points with other subsystems

### XX_api_tools_reference.md (if the system exposes an API/tools)

Every API endpoint or tool. For each tool:

- Name and one-line description
- Every parameter: name, type, required/optional, default, valid values, description
- Return type and structure
- Side effects (database writes, file creation, credit charges, external calls)
- Error conditions

### XX_test_suite.md

Testing infrastructure. Must include:

- Test file inventory with counts per file
- Test categories (unit, integration, FSV, e2e)
- How to run tests (exact commands)
- Key fixtures and test utilities
- Coverage metrics if available

### XX_verification_report.md (final document)

Codebase health snapshot. Must include:

- Test results summary (total tests, pass/fail)
- Lint configuration and status
- Codebase metrics table: number of source files, modules, API endpoints/tools, database tables, test files, lines of code (approximate)
- Schema version
- Any notable constants or magic numbers

## RULES

1. NEVER invent information. If you can't determine something from the code, say "Not determined from source" rather than guessing.
2. NEVER describe aspirational behavior. Document only what the code DOES, not comments about what it SHOULD do.
3. NEVER skip "boring" parts. Document every column, every parameter, every config option. Completeness is the entire point.
4. ALWAYS trace claims to source files. Every behavioral claim should reference the file and ideally the function where that behavior lives.
5. When documenting algorithms, describe the STEPS, not just the name. "Uses Kahn's algorithm" is insufficient. Show the steps.
6. Use tables aggressively. Parameters, schemas, configs, enums, constants, stage definitions — all belong in tables.
7. Keep descriptions factual and terse. "Streams the file in 8KB chunks to handle files larger than memory" — not "This cleverly uses streaming to efficiently process large files."
8. Document the ACTUAL defaults and limits from code, not from comments or docs that may be stale.
9. If the system has phases/versions, document the CURRENT state and note which phase introduced each component.
10. Include "What is NOT covered" sections where relevant (security limitations, known gaps, things the system explicitly doesn't handle).

## PROCESS

1. First, explore the entire project structure to understand scope
2. Read every source file, starting from entry points and following imports
3. Read all test files to understand expected behavior
4. Read build/config files (package.json, pyproject.toml, Cargo.toml, etc.)
5. Produce documents in order, cross-referencing as you go
6. After all documents are written, produce the verification report by actually counting files, tables, tools, and tests from the source

## ADAPTATION INSTRUCTIONS

Adapt the document list to fit the project:

- If no database: skip the schema doc, document data storage however it works
- If it's a library (not a server): replace API tools reference with public API reference
- If it's a CLI tool: document all commands, flags, and subcommands
- If it has a frontend: add a document for component inventory and routing
- If it uses microservices: add a service map document
- Add one deep-dive document per major bounded context or subsystem
- The system overview and source code map are always required regardless of project type
