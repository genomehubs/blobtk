# Phase 3: Configurable BED-derived metric pipeline

## Objective

Generalize the BED import mechanism so that telomere metrics, window density metrics, gap, repeat, coverage, and similar features are handled as configurable metric classes rather than bespoke hard-coded logic.

## Scope

- extend the existing BED import configuration to support a generic metric-spec model
- allow multiple window size specs simultaneously, including 100 kb for small assemblies
- support a standard pattern for:
  - value extraction
  - summary aggregation
  - window-level output
  - feature-level output where relevant
- use this framework for telomere and other spatial metrics alongside GC/gap/repeat/coverage measures

## Concrete metric contract

Each BED-derived metric should be expressed as a compact spec, not as a special-case code path.

MetricSpec:

- id: stable metric name used in config and downstream schema
- source: bedgraph | annotation | sequence-derived | composite
- kind: numeric | flag | enum | distance
- scope: window | sequence | assembly
- anchor: start | end | midpoint | boundary (for distance metrics)
- transform: raw | zscore | robust_zscore | log2 | log2_fold_change | minmax | ...
- summary: mean | median | sum | variance | ...
- output_key: attribute name used in the final document
- availability: always | conditional | annotation-gated

This keeps the metric logic close to the data source while preserving a single implementation pattern.

## Telomere metrics

### Distance-to-telomere

This should be treated as a pure coordinate-derived metric and not as a telomere annotation check.

Required semantics:

- compute using sequence length and window position
- support both boundary-based and midpoint-based measures
- default to midpoint-based distance for downstream analyses, since this is the primary use case
- allow boundary values for cases where start/end adjacency is required

Recommended config fields:

- metric: telomere_distance
- anchor: midpoint | boundary_start | boundary_end
- orientation: min | abs | start_only | end_only

Examples:

- midpoint: distance from the window midpoint to the nearest sequence end
- boundary_start: distance from the window start to the nearest sequence boundary
- boundary_end: distance from the window end to the nearest sequence boundary

This should be a floating-point metric and should not require telomere annotation metadata.

### Telomere-validity flag

This is distinct from distance-to-telomere and should be treated as a per-window validity flag rather than a separate distance metric.

- distance_to_telomere answers: how far is this window from the nearest sequence end?
- telomere_valid answers: is that nearest end a true telomere, as opposed to a sequencing breakpoint or a non-telomeric end?

This is not the same concept as the geometric distance. The distance metric can be computed without annotation data, while the telomere-validity flag requires an annotation or a known telomere assignment for that boundary.

Recommended implementation:

- compute the distance metric at window level using either midpoint or boundary anchors
- attach a companion window-level flag such as telomere_valid or telomere_boundary with values:
  - start
  - end
  - both
  - none
- where the value is constant across a sequence, the compact source of truth may live at sequence scope, but the window should still carry the effective flag for direct filtering and indexing
- the flag should be present on each relevant window so downstream queries can filter without a second join or inference from sequence metadata

This keeps the coordinate-derived distance metric and the annotation-derived terminal validity separate while still allowing efficient downstream filtering.

## Implementation pattern for phase 3

1. Keep the current BED value-column pipeline as the base extraction layer.
2. Add a metric-spec wrapper around the value-column entry so metric behaviour is config-driven rather than hard-coded.
3. Treat window metrics, sequence metrics, and assembly flags as separate scope types.
4. Allow anchor and source selection for spatial metrics like telomere distance.
5. Keep annotation-gated flags separate from arithmetic metrics so the same config can support both known and inferred telomere states.

## Concrete examples

### Example 1: GC z-score

```yaml
value_columns:
  - label: gc
    index: 3
    type: float
    summary_functions:
      - name: mean
    normalisation:
      method: zscore
      statistic: mean_std
      scope: assembly
```

### Example 2: telomere distance from midpoint

```yaml
value_columns:
  - label: dist_to_telomere
    kind: distance
    source: sequence-derived
    anchor: midpoint
    summary_functions:
      - name: mean
```

### Example 3: telomere annotation flag

```yaml
value_columns:
  - label: telomere_state
    kind: enum
    source: annotation
    scope: sequence
    output_mode: inherited
```

Used with:

- absent or none => no known telomere at that end
- start/end/both => known telomeric annotation state for the sequence

## Acceptance criteria for phase 3

- new window metrics can be added through config without adding bespoke code branches
- multiple window sizes are supported in a single import workflow
- distance-to-telomere works from both boundary and midpoint anchors
- telomere presence is represented as an annotation-gated flag or enum, separate from distance metrics
- the config model is generic enough to cover telomere, gap, repeat, coverage, and similar metrics using the same pattern

## Exit criteria

The pipeline can express all planned window-level metrics as config-driven metric specs, with distance and annotation-gated filters clearly separated and implemented without bespoke branching.
