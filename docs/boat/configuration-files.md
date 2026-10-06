# Configuration files

This document is the current reference for the YAML config files used by the import pipeline. It captures the model that is already working in the codebase, plus the staged structure we want to move to for annotation-derived metadata and spatial metrics.

The key design constraints are:

- keep the current import path working without silent drops
- keep the current `path` / `local_path` split for all file-backed sources
- separate source data, assignment rules, and derived metrics
- keep window numeric summaries under the BED/window layer
- keep annotation metadata such as telomeres, centromeres, and repeats in a distinct metadata/annotation layer

---

## 1. Core rule: source files always support `path` and `local_path`

Every file-backed source should support both:

- `path`: the canonical or remote source path
- `local_path`: the local cached copy used for testing or repeated runs

This is not just for BED files. It should be the standard contract for:

- sequence reports
- BED tracks
- BUSCO tables
- BUSCO ALG tables
- GFF annotations
- repeat and telomere annotation tables
- any future metadata source

The runtime rule is:

- if `local_path` is present and readable, use it
- otherwise use `path`
- if neither is set, fail loudly

This is the most important contract for reliable local testing and caching.

---

## 2. Canonical staged config schema

The cleaned-up config model is:

```yaml
assembly:
  accession: &ACCESSION GCA_016920705.1
  taxon_id: null

es:
  host: "http://localhost"
  port: 9200
  hub:
    name: goat
    release: 2021.10.15
    taxonomy: ncbi

sequence:
  report:
    path: "https://example.org/{ACCESSION}/sequence_report.jsonl"
    local_path: "~/tmp/{ACCESSION}.sequence_report.jsonl"
  metadata:
    telomere:
      source:
        path: "https://example.org/{ACCESSION}/telomere_repeats.bed.gz"
        local_path: "~/tmp/{ACCESSION}.telomere_repeats.bed.gz"
      assign_to: [sequence]
      fields:
        telomere_state: start|end|both|none
        has_internal_telomeres: bool
    centromere:
      source:
        path: "https://example.org/{ACCESSION}/centromere.bed.gz"
        local_path: "~/tmp/{ACCESSION}.centromere.bed.gz"
      assign_to: [sequence]
      fields:
        centromere_state: start|end|both|none
        has_centromere: bool

annotations:
  busco:
    tables:
      - path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/busco/{LINEAGE}/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"
        local_path: "~/tmp/{ACCESSION}.{LINEAGE}.full_table.tsv.gz"
        lineages:
          - diptera_odb12
  genes:
    gff:
      path: "https://example.org/{ACCESSION}/genes.gff3.gz"
      local_path: "~/tmp/{ACCESSION}.genes.gff3.gz"
      assign_to: [feature]
  repeats:
    source:
      path: "https://example.org/{ACCESSION}/repeats.bed.gz"
      local_path: "~/tmp/{ACCESSION}.repeats.bed.gz"
    assign_to: [sequence, window]
    fields:
      repeat_density: float

windowing:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
    - type: proportion
      proportion: 0.1
  files:
    - path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/base_content/k1/{ACCESSION}.GC.1k.bedGraph.gz"
      local_path: "~/tmp/{ACCESSION}.GC.1k.bedGraph.gz"
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

    - path: "https://gap.cog.sanger.ac.uk/{ACCESSION}/base_content/k1/{ACCESSION}.N.1k.bedGraph.gz"
      local_path: "~/tmp/{ACCESSION}.N.1k.bedGraph.gz"
      value_columns:
        - label: n
          index: 3
          type: float
          summary_functions:
            - name: mean
          normalisation:
            method: zscore
            statistic: mean_std
            scope: assembly

derived_metrics:
  - name: distance_to_telomere
    target: window
    source: sequence
    anchor: midpoint
    output_type: float

  - name: window_flags
    target: window
    source: sequence
    flags:
      - nearest_telomere_valid
      - has_internal_telomeres
      - has_telomere_start
      - has_telomere_end

import:
  entity_types:
    - sequence
    - window
    - busco
    - attribute
  busco_tallies:
    lineages:
      - diptera_odb12
    assembly_counts_output: "./busco_assembly_counts.tsv"
```

This is the model to target long-term. It cleanly separates:

- source files
- metadata assignment
- window summarisation
- derived spatial outputs

---

## 3. File-backed source contract

Every source block must follow this pattern:

```yaml
source:
  path: "https://example.org/resource.bed.gz"
  local_path: "~/tmp/resource.bed.gz"
```

or, for a simple item:

```yaml
path: "https://example.org/resource.gff3.gz"
local_path: "~/tmp/resource.gff3.gz"
```

Rules:

- `path` is the canonical remote or primary source path
- `local_path` is optional but strongly recommended for testing and cached re-runs
- `local_path` wins when it exists and is readable
- if `local_path` is absent, the import should proceed using `path`
- if neither is valid, fail with a descriptive error

This pattern should be used across all metadata and annotation sources.

---

## 4. Sequence metadata layer

The sequence layer is where chromosome-level state lives. Examples include:

- telomere state
- centromere state
- scaffold QC state
- karyotype flags
- assembly-level feature classification

Example:

```yaml
sequence:
  report:
    path: "https://example.org/{ACCESSION}/sequence_report.jsonl"
    local_path: "~/tmp/{ACCESSION}.sequence_report.jsonl"
  metadata:
    telomere:
      source:
        path: "https://example.org/{ACCESSION}/telomere_repeats.bed.gz"
        local_path: "~/tmp/{ACCESSION}.telomere_repeats.bed.gz"
      assign_to: [sequence]
      fields:
        telomere_state: start|end|both|none
        has_internal_telomeres: bool
```

This is the correct place for state that belongs to the chromosome or scaffold itself, not to the window or feature summary.

---

## 4a. Future metadata block

The staged model reserves a dedicated metadata block for data that is biologically valid but not yet available in the current source pipeline. These sections should stay commented out in active configs until the source data exists, rather than being treated as live imports.

```yaml
sequence:
  metadata:
    # telomere:
    #   source:
    #     path: "https://example.org/{ACCESSION}/telomere_repeats.bed.gz"
    #     local_path: "~/tmp/{ACCESSION}.telomere_repeats.bed.gz"
    #   assign_to: [sequence]
    #   fields:
    #     telomere_state: start|end|both|none
    #     has_internal_telomeres: bool
    # centromere:
    #   source:
    #     path: "https://example.org/{ACCESSION}/centromere.bed.gz"
    #     local_path: "~/tmp/{ACCESSION}.centromere.bed.gz"
    #   assign_to: [sequence]
    #   fields:
    #     centromere_presence: present|absent
    #     centromere_pattern: mono|multi|holo
    #     centromere_count: integer
    # repeats:
    #   source:
    #     path: "https://example.org/{ACCESSION}/repeats.bed.gz"
    #     local_path: "~/tmp/{ACCESSION}.repeats.bed.gz"
    #   assign_to: [sequence, window]
    #   fields:
    #     repeat_density: float
```

This keeps the schema future-proof without implying that telomere, centromere, or repeat annotations are already part of the active import contract.

---

## 5. Annotation layer

The annotation layer covers all non-window source datasets that are attached to a biological entity.

This includes:

- BUSCO tables
- GFF3/GTF features
- repeats
- telomere/centromere source annotations
- breakpoints, compartments, and similar annotation tracks

Example:

```yaml
annotations:
  repeats:
    source:
      path: "https://example.org/{ACCESSION}/repeats.bed.gz"
      local_path: "~/tmp/{ACCESSION}.repeats.bed.gz"
    assign_to: [sequence, window]
    fields:
      repeat_density: float
```

Key rule:

- `assign_to` defines whether the annotation contributes to sequence-level, window-level, or feature-level records
- the source file is separate from the derived metric output

This avoids collapsing all annotation types into the BED metric layer.

---

## 6. Windowing layer

The windowing layer is dedicated to numeric summaries over coordinate windows.

This is the existing working model for BED-based window summarisation and should remain the canonical layer for:

- GC
- coverage
- N-rich / masked proportion
- sequence composition summaries
- downstream transformed metrics

Example:

```yaml
windowing:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
  files:
    - path: "https://example.org/{ACCESSION}/gc.bed.gz"
      local_path: "~/tmp/{ACCESSION}.gc.bed.gz"
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

The `windowing` section should not absorb telomere/centromere/annotation semantics unless they are purely geometric and window-derived. Those states belong in `sequence.metadata` and `derived_metrics`.

---

## 7. Derived metrics layer

Derived metrics are values computed after the source data and sequence state are known.

This includes:

- `distance_to_telomere`
- `distance_to_centromere`
- `window_flags`
- relative block position
- breakpoint distances
- any geometry-driven values

Example:

```yaml
derived_metrics:
  - name: distance_to_telomere
    target: window
    source: sequence
    anchor: midpoint
    output_type: float

  - name: window_flags
    target: window
    source: sequence
    flags:
      - nearest_telomere_valid
      - has_internal_telomeres
      - has_telomere_start
      - has_telomere_end
```

This is the right place for the telomere contract we want to lock in:

- numeric metric values remain standalone fields
- binary or categorical descriptors are grouped into `window_flags`
- multiple new feature types can be added without creating one-off attributes for each value

---

## 8. Legacy compatibility and transition plan

The current config structure is already working, so the new model should be introduced as an explicit superset, not a hard replacement.

Recommended migration rules:

1. Keep the existing top-level `assembly`, `sequence_report`, `bed`, `busco`, and `import` keys valid during the transition.
2. Add the new `sequence`, `annotations`, `windowing`, and `derived_metrics` model as the canonical structure.
3. Internally normalise legacy config to the new staged model before execution.
4. Fail loudly on missing required source metadata.
5. Never silently drop a field or annotation source just because it is not in the current import path.

This ensures that the migration is safe while we refactor the config model.

---

## 9. Recommended conventions

When writing configs, prefer the following rules:

- keep `path` and `local_path` on every file-backed source
- keep source metadata distinct from derived metrics
- keep BED/window summarisation separate from annotation/state metadata
- keep `window_flags` as a compact list, not a one-off boolean attribute per flag
- keep the attribute registry as the final validation layer for output names
- ensure every derived metric has a source and a target

---

## 10. Minimal example with the new contract

```yaml
assembly:
  accession: GCA_016920705.1

sequence:
  report:
    path: "https://example.org/{ACCESSION}/sequence_report.jsonl"
    local_path: "~/tmp/{ACCESSION}.sequence_report.jsonl"

windowing:
  lines_per_unit: 1000
  windows:
    - type: size
      size: 1000000
      remnant_policy: Centered
  files:
    - path: "https://example.org/{ACCESSION}/gc.bed.gz"
      local_path: "~/tmp/{ACCESSION}.gc.bed.gz"
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

derived_metrics:
  - name: distance_to_telomere
    target: window
    source: sequence
    anchor: midpoint
    output_type: float

  - name: window_flags
    target: window
    source: sequence
    flags:
      - nearest_telomere_valid
      - has_internal_telomeres
```

This is the minimal pattern: a single sequence report, a single windowed BED metric, and a geometry-derived window metric set. It is deliberately simple, but it is the right foundation for telomeres, centromeres, repeats, and other annotation-driven feature classes as the project grows.
