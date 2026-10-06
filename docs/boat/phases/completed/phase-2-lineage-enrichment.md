# Phase 2: Lineage enrichment via GoAT

## Objective

Add lineage and taxonomic contextual enrichment to the imported records so that hypotheses requiring phylogenetic structure can be tested directly in the indexed data.

## Scope

- resolve lineage data for each assembly/taxon from GoAT using the taxon ID
- add a clean abstraction for lineage providers
- allow a fallback or alternative provider later if needed
- attach lineage or ancestral taxon context to relevant sequence/window/feature documents

## Why this is necessary

The BoaT model depends on comparative evolutionary context. Without lineage information, the data are still useful for local analysis, but they are not yet ready for the larger evolutionary hypothesis work.

## Deliverables

- lineage provider abstraction with a GoAT implementation
- lineage lookup integration into current import flow
- lineage-enriched documents or query-ready metadata attached to imported records

## Acceptance criteria

- taxon lineage information can be attached during import without forcing a full local taxonomy import
- lineage data is present where needed for comparative analyses
- the provider abstraction allows future local taxonomy or alternative source integration without reworking the importer

## Dependencies

- current taxonomy and import configuration
- GoAT infrastructure availability
- sequence-level taxon assignment

## Exit criteria

The import pipeline can attach lineage context to the analytical records needed for downstream phylogenetic comparisons.

## Follow-on phase

The next implementation step is a dedicated normalization-cache phase, documented in [phase-2b-taxon-normalization-cache.md](phase-2b-taxon-normalization-cache.md). That phase addresses the requirement to normalize local metrics against closely related assemblies efficiently by precomputing lineage-aware baselines once and reusing them during full taxon imports.
