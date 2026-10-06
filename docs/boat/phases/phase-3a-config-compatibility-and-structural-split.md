# Phase 3a: config compatibility layer and structural split

Status: in progress / active

> Decision record: the telomere-derived metric work is intentionally paused pending the real telomere BED source. The config compatibility and staged-model work remains active and complete enough to support the next implementation round without depending on unavailable data. The completed historical phases have been archived to the completed/ subdirectory.

## Objective

Stabilise the current import pipeline while introducing the staged config model that separates source data, metadata assignment, windowing, and derived metrics. This is intentionally a compatibility-focused phase that comes out of the main phase sequence, because the current implementation already works and should not be destabilised while the config structure is clarified.

## Why this is needed

The import pipeline is working, but the config model has started to accumulate responsibilities that do not belong in a single top-level shape. The main risks are:

- legacy config fields are kept in long, over-burdened entry points
- source metadata and window summary rules are mixed together
- file contracts are inconsistent across config blocks
- derived metrics are being added as ad hoc logic instead of a first-class layer
- compatibility shims are difficult to identify and remove later

This phase addresses those issues without forcing a hard-cut migration.

## New config policy for the next implementation step

The next implementation step should keep the staged config model simple and policy-driven rather than expanding into a large bespoke DSL. The config should be expressed as a small set of high-level policy blocks:

```yaml
windowing:
  target_size: 1000000
  bed_resolution: 1000
  remnant_policy: centered
  remnant_bounds:
    min_fraction: 0.67
    max_fraction: 1.33

assembly_policy:
  min_chromosome_fraction: 0.90
  fallback_mode: minimal

scaffold_policy:
  min_scaffold_length: 1000000
  skip_short_scaffolds_without_data: true
  index_small_scaffold_as_parent_if:
    - busco
    - annotation
    - synteny

indexing:
  profile: standard
```

This keeps the config simple while covering the main operational controls discussed in this phase:

- proportional window-size tolerance derived from the target size
- consistent remnant handling and boundary repair
- assembly-level quality gating for chromosome-vs-scaffold mode
- scaffold filtering for short unsupported fragments
- concise indexing profile switching without dozens of one-off flags

## Design rule

The runtime should be driven from a staged internal model, but legacy config should remain accepted as an input format while the migration is in progress.

Important boundary: this phase is intentionally not trying to turn the importer into a general-purpose custom analysis engine. The generic importer should handle reproducible, standardised source + assignment + window summary + geometry-driven metrics. Anything that requires custom multi-source composite logic should stay in a preprocessing layer and be emitted as a standard input track before import.

Canonical staged model:

- sequence
- annotations
- windowing
- derived_metrics

Compatibility rule:

- accept current config keys and normalize them into the staged internal model
- preserve legacy support for the short term
- make all compatibility code explicitly flagged as temporary
- fail loudly when required fields are missing

In-scope rule:

- standard file-backed sources
- standard metric transforms
- geometric derived values like distance-to-telomere
- compact flag outputs like `window_flags`
- metadata assignment onto sequence or window records

Out-of-scope rule:

- arbitrary cross-file custom formulas
- codon-aware or transcript-aware composite metrics created from GFF + CDS + sequence logic in the importer
- bespoke per-metric logic that is not a standard reusable metric family
- new analysis patterns that effectively become a mini data-prep pipeline embedded in config

## Window policy contract

The staged config should expose a single proportional window policy, not a redundant min/max size pair.

The source of truth is:

- `target_size`: nominal window size
- `remnant_bounds.min_fraction` and `remnant_bounds.max_fraction`: permitted tolerated size band around the target window size

This gives an effective valid range of:

$$
[target\_size \times min\_fraction,\\, target\_size \times max\_fraction]
$$

with the following behavioural rules:

- smaller-than-min remnant => merge into the adjacent window
- larger-than-max remnant => split to maintain the valid band
- within-range remnant => keep as-is
- `remnant_policy: centered` should be the default repair strategy for partial endpoints

This is intentionally simpler than separate `min_window_size` and `max_window_size` values because it scales cleanly across target resolutions without conflicting with the remnant logic.

## Validation rules for the next implementation

The config validation layer should enforce the following before import begins:

- `target_size > 0`
- `bed_resolution > 0`
- `target_size > bed_resolution`
- `remnant_bounds.min_fraction > 0`
- `remnant_bounds.max_fraction > 0`
- `remnant_bounds.min_fraction <= 1`
- `remnant_bounds.max_fraction >= 1`
- `remnant_bounds.min_fraction < remnant_bounds.max_fraction`
- if `remnant_policy == centered`, then `min_fraction < 1 < max_fraction`
- if `min_chromosome_fraction` is present, it must be within `[0, 1]`
- `fallback_mode` must be one of `standard`, `minimal`, `skip_windows`, or `abort`
- `min_scaffold_length > 0`
- `index_small_scaffold_as_parent_if` values must be restricted to allowed values such as `busco`, `annotation`, and `synteny`
- `indexing.profile` must be one of the supported profiles such as `standard`, `minimal`, `chromosome_only`, or `feature_only`

These validation rules should fail early with explicit user-facing messages rather than silently allowing inconsistent window sizing or import profiles.

---

## Scope boundary for this phase

This phase should not overreach into custom metric authoring. It is about building a generic, maintainable config shell for standard import tasks.

The importer is responsible for:

- source definition and file resolution
- sequence metadata assignment
- window summarisation
- standard metric scaling / normalisation
- geometry-derived metrics
- compact flag outputs for structural states

The importer is not responsible for:

- multi-file feature fusion for bespoke metrics
- codon-position or transcript-aware composite logic
- ad hoc metric definitions tied to a single assembly-specific analysis
- anything that should really be a preprocessing pipeline step

This keeps the config generic enough to be reused, while avoiding a hard-to-maintain DSL that tries to replace the data-prep stage.

---

## File-by-file checklist

### 1. rust/src/import.rs

Primary responsibility: orchestration entry point and compatibility dispatch.

Checklist:

- [ ] keep import orchestration logic in this file as the top-level entry point
- [ ] add a single compatibility normalization step near config loading
- [ ] convert legacy config into the staged internal representation before runtime execution
- [ ] keep file path resolution and validation separate from import execution
- [ ] ensure any legacy-specific helper is clearly labelled as compatibility-only
- [ ] avoid embedding all config/schema logic directly in this file
- [ ] add a single, obvious comment block describing the migration boundary
- [ ] add `#[deprecated(...)]` to any compatibility helper that is retained only for legacy support
- [ ] verify there is no silent dropping of config keys or output fields

Notes:

- This file remains the orchestrator, not the schema repository.
- The goal is to reduce the size and responsibility of this file over time, not to move everything into it.

---

### 2. rust/src/config/mod.rs

Primary responsibility: expose the config subsystem and keep the import entry point clean.

Checklist:

- [ ] create or expand the module entry for config-specific logic
- [ ] export the normalized config types used by the import pipeline
- [ ] keep the config API small and explicit
- [ ] ensure this module does not duplicate runtime logic from parse or import layers

Notes:

- This is the public boundary for config parsing and normalization.
- Prefer a clean internal API over exposing raw YAML structs directly throughout the project.

---

### 3. rust/src/config/legacy.rs

Primary responsibility: translate legacy keys into the staged internal model.

Checklist:

- [ ] add compatibility parsing for `sequence_report`
- [ ] add compatibility parsing for `bed`
- [ ] add compatibility parsing for `busco`
- [ ] add compatibility parsing for any other legacy top-level blocks still in use
- [ ] convert legacy source entries to `sequence`, `annotations`, `windowing`, and `derived_metrics`
- [ ] retain legacy support only through explicit conversion functions
- [ ] mark each conversion helper as compatibility-only with a deprecation notice if kept long-term
- [ ] ensure conversion preserves values and does not silently discard unsupported fields
- [ ] add tests for each legacy-to-staged translation path

Notes:

- This is the main compatibility bridge and should be obvious in the codebase.
- It should be the first place a maintainer looks when checking what remains from the old config model.

---

### 4. rust/src/config/schema.rs

Primary responsibility: define the canonical internal schema used by the runtime.

Checklist:

- [ ] define `ResolvedPathConfig` with `path` and `local_path`
- [ ] define `SequenceMetadataConfig`
- [ ] define `AnnotationSourceConfig`
- [ ] define `WindowingConfig`
- [ ] define `DerivedMetricConfig`
- [ ] define the top-level staged config container used by runtime execution
- [ ] keep the model clear and minimal; no ad hoc fields for one-off metrics
- [ ] use explicit names for source, target, assignment, and output semantics
- [ ] ensure a field has a clearly defined source and target

Notes:

- This file is the permanent design anchor.
- The legacy conversion layer should feed this structure, not vice versa.

---

### 5. rust/src/config/normalize.rs

Primary responsibility: normalize aliases, defaults, and internal consistency rules.

Checklist:

- [ ] resolve `path` / `local_path` precedence rules
- [ ] expand default values for missing optional fields
- [ ] normalize aliases and old field names to the canonical staged model
- [ ] ensure `local_path` wins when present and readable
- [ ] validate that each required file-backed source has at least one valid source path
- [ ] validate that a derived metric references an actual source or assignment target
- [ ] keep this logic separate from parsing and from import execution

Notes:

- This should not become a dumping ground for all config logic.
- The purpose is to make the staged model internally consistent before use.

---

### 6. rust/src/config/paths.rs

Primary responsibility: path resolution and file access policy.

Checklist:

- [ ] implement the generic path-selection helper: `local_path` preferred over `path`
- [ ] handle cached local files for testing and reruns
- [ ] return an explicit error when neither path exists
- [ ] support remote and local resource handling consistently
- [ ] centralize path resolution so all file-backed config types use the same policy
- [ ] ensure the helper is generic enough for BED, BUSCO, annotation, and metadata files

Notes:

- This is the source of the standard `path` / `local_path` contract.
- Every future file-backed source should call through here.

---

### 7. rust/src/config/derived.rs

Primary responsibility: derived metric contract and metric registration support.

Checklist:

- [ ] define the common derived metric structure
- [ ] include `name`, `target`, `source`, `anchor`, and `output_type`
- [ ] allow compact flag-based metrics such as `window_flags`
- [ ] allow floating-point distance metrics such as `distance_to_telomere`
- [ ] keep metric definitions independent from their implementation site
- [ ] ensure all derived outputs are registry-checked before export
- [ ] keep this as a general model for telomere, centromere, and future spatial metrics

Notes:

- This module is the design point for a clean, generic derived metric model.
- It should not contain one-off telomere logic; it should define the contract that telomere logic implements.

---

### 8. rust/src/config/validation.rs

Primary responsibility: final validation before processing.

Checklist:

- [ ] validate all required config blocks are present
- [ ] validate all source paths resolve correctly
- [ ] validate all registry-backed output names are allowed
- [ ] validate derived metrics have declared sources and targets
- [ ] validate window sizing policy semantics before import starts
- [ ] validate assembly gate semantics before import starts
- [ ] validate scaffold policy semantics before import starts
- [ ] validate indexing profile selection before import starts
- [ ] fail clearly if a feature or field is unknown to the registry
- [ ] keep validation separate from normalization and parsing
- [ ] ensure any validation failure is user-facing and actionable

Notes:

- This is the safety net before the import pipeline starts.
- It should catch silent configuration drift that would otherwise lead to data loss or missing outputs.
- Window-size validation is especially important here because it prevents invalid or contradictory import geometry from entering the runtime.

---

### 9. rust/src/parse/bed.rs

Primary responsibility: window parser, summary generation, and derived window outputs.

Checklist:

- [ ] keep BED parsing logic here
- [ ] keep summary aggregation logic here
- [ ] keep normalisation logic here
- [ ] keep window-derived value implementations here
- [ ] implement the remnant repair logic using the proportional bounds around `target_size`
- [ ] implement `distance_to_telomere` as a coordinate-derived metric
- [ ] implement `window_flags` as a compact list output rather than a per-flag boolean explosion
- [ ] keep the telomere contract separate from the annotation contract
- [ ] do not move config translation logic into this file
- [ ] add or update unit tests covering geometry semantics, remnant repair, and flag logic

Notes:

- This file should be the implementation home for metrics, not the config migration layer.
- It already contains the logic most needed for the next metrics iteration and should remain focused on that.
- The remnant repair policy is a core implementation requirement for the new windowing config and should be explicitly tested.

---

### 10. rust/config/attributes/boat-attribute-registry.yaml

Primary responsibility: authoritative output metadata registry.

Checklist:

- [ ] add registry entries for new derived metrics
- [ ] ensure all emitted attribute names are registered
- [ ] ensure compact `window_flags` values are recognised as valid output metadata
- [ ] keep compatibility values and deprecated names clearly separated from active output names
- [ ] ensure newly added derived values are documented in the registry before they are emitted

Notes:

- Registry enforcement is the final guardrail against silent output drift.
- This is especially important when the config model is in transition and more metrics are being added.

---

### 11. docs/boat/configuration-files.md

Primary responsibility: user-facing config documentation and migration guidance.

Checklist:

- [ ] document the staged config model
- [ ] document the `path` / `local_path` contract
- [ ] record the compatibility transition path from legacy to staged config
- [ ] describe the derived metric model and the `window_flags` convention
- [ ] describe the intended future annotation metadata layer
- [ ] keep legacy references explicit so users know what is still supported temporarily
- [ ] update with examples that reflect the staged structure

Notes:

- This doc is the user-facing contract that should intentionally explain the migration path, not just the final shape.

---

## Compatibility deprecation checklist

These are the rules for retained compatibility code:

- [ ] all compatibility helpers must be easy to locate by name
- [ ] each legacy helper should have a focused comment explaining the deprecation timeline
- [ ] compatibility shims should be isolated to one module rather than spread across runtime code
- [ ] no compatibility code should be hidden inside parsing or derived metric logic
- [ ] any helper retained for migration should be marked with a deprecation note or TODO
- [ ] tests should cover both the legacy path and the staged path

This makes the migration tractable and prevents “compatibility drift” from becoming an unavoidable future refactor.

---

## Implementation sequence

Recommended order:

1. create the config submodule and typed internal schema
2. add the compatibility translation layer for legacy config
3. add `path` / `local_path` resolution and validation
4. add the window policy validation and remnant-bound repair logic
5. add the assembly quality gate and scaffold inclusion policy validation
6. wire the staged model into the runtime import path
7. implement the first real derived metrics (telomere distance + `window_flags`)
8. validate that all outputs are registry-backed
9. extend the same pattern to centromeres, repeats, and future annotations

---

## Acceptance criteria for this phase

- [ ] the existing import path continues to work with legacy config input
- [ ] the runtime uses an explicit staged internal config model
- [ ] all file-backed sources follow the `path` / `local_path` contract
- [ ] windowing policy validation enforces valid target size, bed resolution, and remnant-band semantics
- [ ] assembly quality gating can downgrade or abort imports based on chromosome assembly fraction
- [ ] scaffold inclusion logic can skip low-value short scaffolds while preserving evidence-bearing ones
- [ ] indexing profile selection supports the minimal/standard chromosome-only feature gating model
- [ ] compatibility helpers are identifiable and clearly marked for deprecation
- [ ] the config model is split sufficiently to remain manageable even as new field types are added
- [ ] the first derived metric set is implemented using the staged model and validated against the registry

## Exit criteria

The project has a stable compatibility bridge, a clearly separated config subsystem, and a simple policy-driven import model that controls window geometry, assembly quality, scaffold retention, and profile selection without reintroducing the same ad hoc config drift.
