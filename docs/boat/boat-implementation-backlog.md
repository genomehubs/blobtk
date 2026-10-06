# BoaT implementation backlog

This backlog turns the high-level BoaT plan into concrete tasks mapped to the files that currently implement the importer, parser, and Elasticsearch model layer. It is intentionally scoped to the code that exists in this repository so the work can be picked up immediately.

## Phase 0: authoritative attribute registry and schema reconciliation

### Goal

Make the current attribute naming and compatibility layer explicit before deeper BoaT work expands the schema.

### Tasks

1. Create the authoritative attribute registry
   - New file: `config/attributes/boat-attribute-registry.yaml` (or equivalent config location)
   - Define canonical names, compatibility aliases, deprecation status, and mapping source.
   - Include sequence, window, BUSCO, synteny, and BED-derived fields.
   - Add a small validation script or import-time check that fails when a field is added without registry entry.

2. Reconcile naming in the importer output layer
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Review the metadata attached during sequence and window import.
   - Confirm which attributes are active, compatibility-only, or deprecated and ensure their labels are clearly separated.

3. Tighten the nested attribute model for rich payloads
   - File: [rust/src/index/es/models/nested_documents.rs](../../rust/src/index/es/models/nested_documents.rs)
   - Confirm that nested payloads such as `group_counts` and `top_transitions` remain object-shaped and queryable.
   - Preserve compatibility mechanisms for previously flattened values while moving the registry to an explicit contract.

4. Align Elasticsearch mapping to the schema contract
   - File: [rust/src/index/es/mappings/common.rs](../../rust/src/index/es/mappings/common.rs)
   - Review the `flattened_property` behaviour and ensure it is used only for rich, object-shaped analytical payloads.
   - Document which attributes are searchable vs metadata-only.

### Definition of done

- A canonical YAML registry exists and is used as the source of truth.
- All current sequence/window/feature attributes are accounted for.
- Compatibility aliases are explicitly marked and documented.
- The importer and query layer know which fields are active and which are legacy.

---

## Phase 1: baseline feature and window import

### Goal

Keep the v1 importer working with a single feature index and feature-type-based routing while supporting multiple window sizes.

### Tasks

1. Stabilize single-index feature import semantics
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Confirm `feature_type` remains the primary discriminator.
   - Ensure the importer keeps current feature-type values working without requiring split-index architecture.

2. Extend sequence metadata attachment
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Validate sequence- and scaffold-level metadata attachment remains consistent with the operational plan.
   - Confirm chromosome and length metadata are attached to sequence features before window metrics are processed.

3. Extend BUSCO parse and feature generation logic
   - File: [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs)
   - Keep BUSCO and synteny features generating consistent IDs and feature docs.
   - Verify that synteny loci and BUSCO features are both attached to sequence/window features where expected.

4. Harden window generation and aggregation
   - File: [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs)
   - Add support for multiple configured window sizes in a clean config-driven way.
   - Ensure the `WindowSpec` pattern can include both 1 Mb and 100 kb bins without bespoke logic.

5. Add a config-level regression test suite
   - Files: [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs), [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs), [rust/src/import.rs](../../rust/src/import.rs)
   - Cover: sequence metadata, BUSCO import, synteny blocks, and multi-window generation.
   - Verify that a single feature index still works for v1.

### Definition of done

- Sequence, BUSCO, and window records import together in the configured single-index model.
- Multiple window sizes can be configured without code special-casing.
- Sequence-level metadata and window metrics remain consistent and queryable.

---

## Phase 2: lineage enrichment via GoAT

### Goal

Attach taxonomic and lineage context in a way that remains decoupled from the massive coordinate indexes.

### Tasks

1. Separate lineage-provider abstraction from the import engine
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Add a provider boundary for lineage enrichment, with a concrete GoAT implementation.
   - Keep the provider pluggable so future local taxonomy or alternative sources can be substituted.

2. Add lineage metadata to imported sequence records
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Confirm taxon and lineage identifiers are attached before downstream analysis.
   - Preserve the decoupled model: lineage metadata should not force re-indexing of the entire coordinate payload.

3. Add a small lineage fetch and validation test
   - Files: [rust/src/import.rs](../../rust/src/import.rs), [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs)
   - Validate lineage enrichment on a small representative data set.
   - Check that missing lineage data fails clearly and early rather than silently.

### Definition of done

- GoAT lineage lookup is operationally integrated.
- Sequence docs can be enriched with lineage metadata cleanly.
- A provider abstraction exists for future lineage-source swaps.

---

## Phase 2b: taxon-normalized reference cache and batch normalization

### Goal

Precompute lineage-aware normalization baselines for closely related assemblies and reuse them during the main import so that biological and technical metrics are normalized against stable, taxonomically relevant batches without repeatedly re-reading the same files.

### Tasks

1. Define the taxon batch resolver
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Resolve the focal assembly's family/order/class/phylum fallback path and determine a stable batch for normalization.
   - Return a compact, stable batch ID per metric family.

2. Build a normalization baseline cache
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Store compact summaries for GC, repeat, satellite, coverage, and gap metrics keyed by metric family, batch, and coordinate system.
   - Keep the cache in memory for a single run and optionally persist to disk for repeated batch imports.

3. Split biological vs technical normalization paths
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Preserve distinct logic for biological inputs (GC/repeat/satellite) and technical inputs (depth/gap burden).
   - Ensure each path uses its own normalization semantics and batch statistics.

4. Wire the stage into the import lifecycle
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Add a preliminary baseline-precompute stage before the full taxon import runs.
   - Reuse the cached batch statistics during the actual import instead of re-reading the same source files.

5. Add regression tests for batch fallback and cache reuse
   - Files: [rust/src/import.rs](../../rust/src/import.rs), [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs)
   - Validate N >= 5 fallback logic, singleton handling, and cache reuse on repeated imports.
   - Confirm metrics remain correctly separated by biological vs technical pathway.

### Definition of done

- Each assembly resolves a stable taxonomic normalization batch with fallback logic.
- The import pipeline computes batch baselines once and reuses them for subsequent taxon imports in the same run.
- Biological and technical metrics remain on separate normalization paths.
- The design supports efficient multi-taxon imports without repeated large file scans.

---

## Phase 3: configurable BED-derived metric pipeline

### Goal

Turn the current ad hoc BED processing into a reusable, config-driven metric framework.

### Tasks

1. Generalize BED value-column processing
   - File: [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs)
   - Expand the current `ValueColumn` and `SummaryFunction` logic to support arbitrary metric definitions beyond the current set.
   - Keep the summary architecture generic.

2. Add a clear metric-spec model for future bed metrics
   - File: [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs)
   - Define how each metric should be extracted, summarized, and associated to window or sequence-level records.
   - Ensure the pattern is not hard-coded to a single biological property.

3. Wire the metric model into the import pipeline
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Attach derived values to the correct window and sequence objects.
   - Ensure the per-window logic stays local and does not accidentally mix sequence-level and window-level metrics.

4. Add realistic regression tests for derived metrics
   - Files: [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs), [rust/src/import.rs](../../rust/src/import.rs)
   - Validate mean, median, count, and variance behaviours.
   - Reinforce that metrics stay attached at the right granularity.

### Definition of done

- BED metrics can be added in config without new bespoke import code.
- The metric framework supports the planned telomere and repeat-structure analyses.
- Window-level metrics remain distinct from sequence-level summaries.

---

## Phase 4: centromere and telomere-aware derived metrics

### Goal

Turn coordinate and sequence data into the spatial metrics needed for chromosomal and evolutionary comparisons.

### Tasks

1. Derive telomere distance from sequence position and size
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Compute midpoint-to-telomere distance using sequence length and window or feature position.
   - Keep this independent of any telomere-specific annotation requirement.

2. Add centromere-aware distance calculation where available
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Add conditional centromere handling for monocentric and holocentric assemblies.
   - Ensure holocentric paths return a neutral or zero-interpretation result instead of failing.

3. Attach derived spatial metrics to imported objects
   - File: [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs)
   - Confirm the rich metrics layer can hold spatial outputs alongside BUSCO/synteny groups and counts.
   - Export these values through the same nested attribute mechanism already used by rich analytical payloads.

4. Add validation around spatial metric semantics
   - Files: [rust/src/import.rs](../../rust/src/import.rs), [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs)
   - Validate distance computation against representative windows and sequence lengths.
   - Confirm no window-specific metrics leak into adjacent windows.

### Definition of done

- Distance-to-telomere is computed from coordinate context and sequence length.
- Centromere-aware metrics are available when centromere metadata exists.
- Spatial metrics are attached consistently to the correct document granularity.

---

## Phase 5: GFF and gene-level functional annotations

### Goal

Add the optional gene-level annotation path once the coordinate and metric framework is stable.

### Tasks

1. Add optional GFF import path
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Keep GFF import behind a staged feature toggle rather than a mandatory part of v1.
   - Preserve the standard sequence/BUSCO/BED import path as the primary route.

2. Extend the attribute and nested model for gene metadata
   - Files: [rust/src/index/es/models/nested_documents.rs](../../rust/src/index/es/models/nested_documents.rs), [rust/src/index/es/mappings/common.rs](../../rust/src/index/es/mappings/common.rs)
   - Confirm the nested model supports gene-level functional metadata and denormalized window values.

3. Add gene-level denormalization support
   - File: [rust/src/import.rs](../../rust/src/import.rs)
   - Enrich gene docs with local metric values such as GC, repeat density, and distance-to-centromere/telomere.
   - Ensure gene records are assigned using midpoint-based single-window semantics.

4. Add optional tests for GFF-driven gene enrichment
   - Files: [rust/src/import.rs](../../rust/src/import.rs), [rust/src/index/es/models/nested_documents.rs](../../rust/src/index/es/models/nested_documents.rs)
   - Validate that gene enrichment does not disrupt the sequence/window data model.

### Definition of done

- Optional GFF annotation import exists without breaking the default flow.
- Gene and function-level annotations can be enriched with local coordinate metrics.
- The code remains compatible with the single-index v1 design.

---

## Cross-cutting tasks by file

### [rust/src/import.rs](../../rust/src/import.rs)

Primary backlog items:

- sequence metadata binding and assembly-level import
- window-specific metric attachment
- BUSCO and synteny metric attachment
- lineage provider execution
- derived coordinate metrics and gene-level enrichment
- import-time validation and filter logic

### [rust/src/parse/busco.rs](../../rust/src/parse/busco.rs)

Primary backlog items:

- BUSCO feature parsing and grouping
- synteny block construction and metrics
- attributes emitted during synteny/ALG processing
- rich group and transition payload generation
- compact vs compatibility field separation

### [rust/src/parse/bed.rs](../../rust/src/parse/bed.rs)

Primary backlog items:

- generic summary functions and BED metric calculations
- multi-window configuration support
- 100 kb and 1 Mb aggregation logic
- config-driven metric expansion for future biological metrics

### [rust/src/index/es/models/nested_documents.rs](../../rust/src/index/es/models/nested_documents.rs)

Primary backlog items:

- object-shaped flattened payload handling
- compatibility and rich payload serialization
- gene-level and analytical nested metadata support
- deprecation metadata tracking for legacy values

### [rust/src/index/es/mappings/common.rs](../../rust/src/index/es/mappings/common.rs)

Primary backlog items:

- mapping definitions for flattened analytical payloads
- schema contract alignment for feature, window, and attribute docs
- field-type choices for compatibility and rich metrics

---

## Recommended execution order

1. Phase 0: attribute registry and schema reconciliation
2. Phase 1: baseline feature and window import
3. Phase 2: lineage enrichment via GoAT
4. Phase 3: configurable BED-derived metrics
5. Phase 4: centromere and telomere-aware derived metrics
6. Phase 5: GFF and gene-level annotation extension

This order keeps the importer stable while the schema and metric models become explicit, then expands into phylogenetic and functional analysis once the core framework is solid.
