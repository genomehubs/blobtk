# Config-driven BED import and canonical metric naming

This document records the intended contract for sequence/window summary metrics and the import configuration model they depend on.

## Core rule

Sequence and window statistics are not a fixed list of hard-coded names. They are generated from the configured BED import definitions and should be treated as config-driven metrics.

The canonical source of truth for these metrics is the BED import config, not the sequence report parser. This is especially important for:

- GC
- coverage
- masked proportion
- any other derived sequence composition or window summary statistics

These fields should be configured explicitly in the import YAML and the resulting key names should be driven by the configured `label` and summary function combination.

## Why the BED source is canonical

Using BED as the canonical source makes the metrics consistent across:

- sequence-level summaries
- window summaries
- assembly-level summaries
- downstream comparative hypothesis analyses

This avoids drifting semantics when the same concept is derived from two different sources, for example:

- GC from a sequence report at one stage
- GC from a bed file at another stage
- a different convention for count versus proportion in different windows

A configured BED metric is explicit about both the quantity and the aggregation behaviour. That makes the distinction clear between:

- raw count metrics
- proportion or mean metrics
- summary statistics over a window
- sequence-length-normalized values

## Canonical naming guidance

The registry should prefer names that reflect the business meaning, not the derivation mechanism.

Examples:

- `gc` is the canonical short name for the BED-derived GC metric
- `coverage` is the canonical short name for the configured coverage metric
- `masked` is the canonical short name for the configured masked proportion metric
- `seq_proportion` should be considered deprecated unless a specific use case requires it

In general:

- prefer explicit metric names over generic placeholders
- prefer compact names where the meaning is already clear
- keep `group_counts` and `top_transitions` separate from active compact fields
- do not collapse rich analytic payloads into the core summary set

## Configuration pattern

The BED config is the mechanism that defines the metric source and semantics. A configuration entry should define:

- the path to the BED file
- the value column index
- the value type
- the summary function(s)
- the window spec(s)

Example pattern:

```yaml
bed:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
    - type: size
      size: 100000
  files:
    - path: /path/to/gc.bed.gz
      value_columns:
        - label: gc
          index: 3
          type: float
          summary_functions:
            - mean
    - path: /path/to/coverage.bed.gz
      value_columns:
        - label: coverage
          index: 3
          type: float
          summary_functions:
            - mean
            - sum
    - path: /path/to/masked.bed.gz
      value_columns:
        - label: masked
          index: 3
          type: float
          summary_functions:
            - mean
```

The actual index key names are generated from the configured `label` plus the selected summary function, for example:

- `gc_mean`
- `coverage_mean`
- `masked_mean`
- `coverage_sum`

This should be the authoritative pattern rather than a fixed schema layer with hard-coded names.

## Handling count versus proportion values

The import config should make the distinction between counts and proportions explicit.

Use:

- `count` for raw counts or number of loci/segments
- `mean` or a proportion alias for normalized values
- `sum` only when the semantic meaning is truly additive

This distinction matters for the BoaT hypotheses because the same raw signal can ask different biological questions depending on whether the metric is:

- a direct count
- a normalized proportion
- a summary over a window
- a fixed-size lifted summary over the sequence

Do not rely on a generic `seq_proportion` field as the universal solution; prefer explicit bed-derived metrics with the correct semantics.

## Review recommendation

The registry should mark fixed fields as either:

- `canonical` when they are the current preferred name
- `config-driven` when they are imported from bed config
- `derived` when they are computed from the feature boundaries or similar internal logic
- `deprecated` when they are legacy or low-value generic fields

This is the right contract for review and future implementation.
