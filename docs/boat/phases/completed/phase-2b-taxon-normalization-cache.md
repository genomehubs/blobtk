# Phase 2b: taxon-normalized reference cache and batch normalization

## Objective

Add a staged normalization layer that can compare a focal assembly against closely related assemblies before the main import is finalized. This must work efficiently without re-reading the same external files repeatedly for every taxon in a large import batch.

## Why this is necessary

Several of the BoaT analyses are not valid if they are normalized only against a single assembly or a single local window. Biological metrics such as GC, repeat density, and satellite fractions need a family- or lineage-level baseline, while technical metrics such as depth and gap burden need a relative assembly correction before comparative normalization.

The core challenge is operational rather than biological: the relevant batch statistics may need to be calculated for a set of related assemblies before the focal assembly is fully imported, and those baseline values must then be reused for the full taxon import rather than recomputed each pass.

## Scope

- identify a taxonomic batch for each focal assembly using lineage-aware fallback rules
- precompute a compact baseline summary over the relevant assemblies
- persist the summary in a cache keyed by taxonomic batch and metric family
- use the cached baseline in the full import for normalization steps
- keep the cache reusable across multiple assemblies in the same lineage batch

## Design principles

### 1. Batch-first normalization

For each focal assembly, select a target comparison group in this order:

1. Family-level batch
2. Order-level fallback
3. Class-level fallback
4. Phylum-level fallback

The baseline should only be used when enough members exist to make the estimate stable. A practical threshold is `N >= 5`; otherwise the fallback hierarchy should continue upward until a stable batch is found.

This matches the dynamic fallback logic already described in the operational plan and ensures that singleton or very small families do not produce unstable Z-score baselines.

### 2. Separate biological and technical normalization paths

The implementation should preserve the distinction already described in the operational plan:

- Biological metrics: GC, repeat density, satellite fractions
  - normalize directly against the batch distribution for equivalent coordinates
- Technical metrics: coverage/depth and assembly gap burden
  - first convert to a relative assembly value, then normalize against the batch distribution

These should not share the same cache or the same weighting rules because their statistical meaning differs.

### 3. One read, many reuse

The pipeline should not repeatedly reparse large BED or sequence-summary files for every assembly in the same batch. Instead, the design should be:

1. discover all assemblies in the target batch
2. read the minimal required source files once
3. compute the batch summaries and store them in a compact cache
4. reuse those summaries during the full import for each focal taxon

This reduces IO, avoids repeated data scans, and ensures consistent batch statistics across the full import run.

## Formal implementation checklist

Use the following as the delivery checklist for the Phase 2b implementation. Each item is intentionally scoped to the manifest-driven normalization model and should be treated as required before the next phase can begin.

- [ ] Define the explicit batch manifest schema in YAML and validate it during import startup.
- [ ] Add a `batch resolver` service that accepts a focal assembly, lineage metadata, and metric family and returns a stable batch ID plus member set.
- [ ] Implement the family → order → class → phylum fallback chain with a minimum-stable-size rule (`N >= 5`).
- [ ] Add explicit handling for singleton and undersized batches so they fail clearly rather than silently normalizing on unstable baselines.
- [ ] Separate biological metric normalization from technical metric normalization in the codepath and data model.
- [ ] Create a compact normalization cache keyed by batch ID, metric family, coordinate system, and schema version.
- [ ] Persist only the summary statistics required by the downstream model (mean, SD, median/MAD where necessary, bin mapping, batch size, version stamp).
- [ ] Add a precompute stage to the import lifecycle that resolves the batch and fills the normalization cache before the focal import is finalized.
- [ ] Ensure the full taxon import consumes the cached values instead of re-reading the same source files for each closely related assembly.
- [ ] Add regression tests for: singleton fallback, class/order fallback, batch cache reuse, and biological vs technical metric separation.
- [ ] Confirm the implementation does not rely on filesystem scanning as the runtime source of truth for batch membership.
- [ ] Validate that manifest-driven batch membership is deterministic and reproducible across repeated import runs.

## Proposed implementation architecture

### A. Batch resolver

A small service should resolve a normalization batch for a focal assembly:

- `taxon_id`
- lineage path from GoAT
- preferred comparison level
- fallback level if batch size is below threshold
- resulting batch member set

This component should produce a stable batch ID such as:

- `family:<family_id>:gc`
- `order:<order_id>:coverage`
- `class:<class_id>:repeat`

### B. Metric-specific cache entries

Cache entries should be keyed by:

- metric family (`gc`, `repeat`, `satellite`, `coverage`, `gaps`)
- batch ID
- coordinate system (`physical_window`, `proportional_bin`, `sequence_position`)
- sequence type or subset (e.g. chromosome vs scaffold, gene-containing only)

Each entry should store a compact summary, not the full raw feature set:

- count of assemblies in the batch
- mean and standard deviation per coordinate bin or bin class
- optional median and MAD for robust fallback
- a version stamp for the dataset and metric config

The cache object should live either in memory for a single import batch or on disk as a small JSON/Parquet snapshot for repeated runs.

### C. Import-phase execution model

Use a staged execution plan:

1. `bootstrap_lineage_context`
   - resolve taxon lineage and batch membership
2. `precompute_normalization_baselines`
   - read the minimal relevant files for the batch
   - compute the batch summaries once
   - write or keep the cache
3. `full_taxon_import`
   - import sequence/BUSCO/BED data for the focal taxon
   - apply cached batch stats during the normalization step

This allows the full import to remain conceptually one pass for the focal taxon, while the reference data needed for normalization is prepared in a background or pre-pass stage.

## Efficiency considerations

### Prefer compact summaries over full raw data

The cache should store only the statistics needed for downstream normalization, not the full coordinate arrays for every assembly. This keeps memory and disk usage bounded while still supporting correct batch scaling.

### Use batch-level deduplication

If several assemblies in the same family share the same normalization group, compute that group once and reuse it aggressively.

### Keep all metric families separate

Different metric classes should be computed independently. Bio and technical metrics have different normalization semantics and should not share the same aggregated cache object.

## Deliverables

- taxonomy-aware batch resolver
- baseline-normalization cache structure
- staged import hook for batch precomputation
- normalized summary outputs for biological and technical metrics
- cache invalidation/versioning support for data refreshes

## Acceptance criteria

- each focal assembly resolves a valid comparison batch without division-by-zero or singleton instability
- the normalization baseline is computed once per batch and reused across the same import run
- the full import path can consume the cached normalization values without re-reading the same external inputs repeatedly
- biological and technical metrics remain on distinct normalization paths

## Dependencies

- Phase 2 lineage enrichment via GoAT
- sequence and BED import config for the relevant metric classes
- a stable batch or taxonomic lookup abstraction

## Exit criteria

The importer can compute taxon-aware normalization baselines using a shared cache, and the resulting normalized values are used during full import with no repeated batch re-scan for closely related assemblies.
