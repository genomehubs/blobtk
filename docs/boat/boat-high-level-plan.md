# BoaT high-level implementation plan

This document provides the high-level planning layer for BoaT development. It sits alongside the operational plan and translates the biological and schema goals into an implementation-ready programme of work.

The design principles are:

- keep the initial deployment simple and direct
- prefer a single feature index with a feature_type discriminator for v1
- treat the future split-by-type architecture as a downstream optimization rather than a base requirement
- use configurable import patterns for bed-derived window metrics so that new metric classes can be added without bespoke code paths
- establish an authoritative YAML attribute registry as the bridge between current importer field names and future BoaT naming
- keep GoAT lineage lookup as the default external lineage source, while preserving a clean provider abstraction for future alternatives
- treat hypothesis readiness as phased: some metrics are available immediately, while others depend on downstream data sources or richer annotations

---

## 1. Scope and design intent

The BoaT project is intended to support a set of evolutionary hypotheses by indexing coordinate-level data in a form that is directly queryable for comparative analysis. The architecture should not be optimized too early for an eventual multi-index split. Instead, the initial implementation should aim for:

- scalable indexing of sequence, window, and feature-level records
- consistent attribute naming via a central registry
- configurable bed-driven metric ingestion
- direct support for BUSCO/synteny and window-level metrics
- lineage-aware analysis via GoAT taxon lookup
- a path for later expansion to GFF-based functional annotation and gene-level analysis

This is the right balance between biological ambition and engineering pragmatism.

---

## 2. Design decisions confirmed for implementation

### 2.1 Single index for feature data in v1

For initial development, the importer should continue to use a single feature index with a canonical `feature_type` value. This is the preferred strategy because:

- it reduces migration complexity
- it preserves compatibility with existing feature-type values already used by the import pipeline
- it enables a future split-by-type strategy with minimal rework if the platform later needs separate indexes or routing rules

The `feature_type` field should remain the key discriminator; additional type-like values can still exist as aliases if needed for compatibility.

### 2.2 Variable window sizes are a configuration concern

Small assemblies need 100 kb windows in addition to the standard 1 Mb windows. This should be implemented as a configuration-level parameter, not as a special case in the core import logic.

This means:

- window size should be configurable in the bed/window configuration
- the import pipeline should support multiple window specs simultaneously
- window-level analysis can operate across multiple bin sizes without bespoke code paths

This keeps the implementation generic and future-proof.

### 2.3 Distance to telomere is window-position derived

Distance to telomere should be calculated from the window position and the sequence length, not from telomere annotations. Telomere annotations are useful as historical structural metadata, but they are not required to compute the basic metric.

In other words:

- basic distance-to-telomere is a derived spatial statistic
- telomere annotations help interpret chromosomal context and historical fusion points
- they should not gate the core metric calculation

### 2.4 GoAT is the preferred lineage source

The external GoAT lineage dependency is acceptable and is in line with the operational constraints of the current project.

This is a sensible choice because:

- it is on the same infrastructure as the current BUSCO and dataset fetches
- it is operationally equivalent in reliability to those data sources
- it avoids the cost of loading full local taxonomy data for every import

The importer should still define a lineage provider abstraction so that an alternative local taxonomy source can be inserted later without rewriting the import pipeline.

### 2.5 Attribute names are not yet finalised

The attribute naming layer is intentionally unsettled. The right approach is to establish a canonical registry in YAML that captures:

- current names already used in the import pipeline
- planned BoaT names
- recommended final names
- compatibility aliases
- deprecation status

This becomes the source of truth for the schema contract and should be editable by the project owner as the naming stabilizes.

### 2.6 Bed-based metric import should be generic

Telomere metrics, gap density, sequence content, and future bed-derived metrics should be treated as configurable import patterns rather than special cases.

The pipeline should be able to define a metric as:

- source bed file
- value column or derived statistic
- summary function
- target aggregation level
- feature or window semantics

This keeps the system extensible for future biological metric classes.

### 2.7 GFF support is staged, not a blocker

GFF inputs are available for some taxa, but they should be treated as a later extension phase. They are not required for the initial analytical pipeline. The base version should assume standard sequence report + BUSCO + BED inputs, with GFF support added once the common attribute and metric framework is stable.

---

## 3. High-level implementation architecture

The architecture can be developed in phases without a premature split-by-index design.

### 3.1 Core data layers

1. Sequence/scaffold layer
   - sequence report-derived metadata
   - chromosome class / scaffold type
   - sequence length
   - assembly and taxon metadata

2. Feature layer
   - BUSCO loci
   - synteny blocks / loci
   - repeat and other coordinate features
   - indexed with a single feature index and feature_type discriminator

3. Window layer
   - BED-derived metric aggregation into standard and configurable window sizes
   - compact summary stats used for comparative analyses

4. Lineage layer
   - GoAT-driven taxon lineage enrichment
   - optionally later expanded to local taxonomy provider or other source

5. Functional annotation layer
   - deferred to GFF/Gene phase
   - not required for v1 readiness

---

## 4. Scope of the initial implementation

The initial implementation should prioritize the data that is already available and directly supports the core analytical model without requiring broad downstream work.

This includes:

- sequence report metadata attachment
- BUSCO and synteny feature indexing
- BED-derived coverage and window metrics
- configurable window sizes
- a unified feature index with `feature_type` as a canonical discriminator
- lineage enrichment via GoAT
- attribute reconciliation through YAML registry

This is the foundation to support the main evolutionary hypotheses without forcing species-by-species or format-specific complexity.

---

## 5. Hypothesis readiness roadmap

### Phase 1: baseline hypothesis support

The following are ready or nearly ready within the current pipeline:

- H2: Structural determinants of karyotypic stasis
- H4: Biophysical barriers to assembly

These are strongly supported by the current BUSCO + BED + sequence metadata framework.

### Phase 2: lineage and spatially-aware support

The following become well-supported once lineage enrichment and configurable bed metrics are in place:

- H1: recombination-GC drive
- H3: tempo of sex chromosome degeneration
- H6: centromere-driven decay

These depend on lineage-aware comparisons and spatial metrics that are not yet fully formalized in the import pipeline.

### Phase 3: downstream functional readiness

The following are gated on further downstream work and richer annotation data:

- H5: spatial compartmentalization of gene function
- more advanced gene-level functional comparisons

These will require gene annotations and richer functional data, which should be added in a later phase.

### H7: holocentric escape from decay sinkholes

This is partially enabled by the same centromere/chromosome and repeat-density framework, but it is still a downstream analytical milestone because it depends on robust comparative chromosome architecture and shape information across taxa.

---

## 6. Key execution principles

1. Keep the v1 model simple and direct.
2. Use configuration rather than bespoke hard-coded logic for new metric classes.
3. Keep the feature index unified until a measured need for splitting emerges.
4. Treat GoAT lineage data as a built-in enrichment step rather than a secondary afterthought.
5. Put naming decisions behind a YAML-driven attribute registry so the authoritative schema can evolve without breaking imports.
6. Stage hypothesis readiness based on data availability rather than pretending every metric is needed in the first implementation.

---

## 7. Summary

The BoaT plan remains valid as a long-term analytical architecture, but the implementation should happen in stages and should follow the data that is already available in the current importer. The most important decisions are now clear:

- single feature index in v1
- configurable multi-window support
- distance-to-telomere derived from position and length
- GoAT as the lineage source
- YAML attribute registry as the schema source of truth
- bed-driven metrics as the generic extension mechanism
- downstream gene annotation as a later phase rather than a blocker for initial work

This gives a realistic roadmap for turning the operational plan into an implementation plan without overcommitting to unsupported formats or premature architectural splits.
