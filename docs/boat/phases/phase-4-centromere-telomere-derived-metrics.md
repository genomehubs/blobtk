# Phase 4: Centromere and telomere-aware derived metrics

Status: paused / waiting for telomere annotation data

> Decision record: the sequence-derived telomere metric logic is already implemented and unit-tested, but the biological telomere annotation path remains pending the availability of a real telomere BED or equivalent annotation source. This phase is intentionally paused until that input exists; the active config-compatibility work remains in Phase 3a.

## Objective

Convert the raw coordinate and window data into the biological distance and spatial measures needed for the main evolutionary hypotheses.

## Scope

- derive distance to telomere from window position and sequence length
- keep telomere annotations as optional structural metadata rather than a requirement for the metric itself
- derive centromere-aware spatial distances where centromere annotations are available
- attach spatial metrics consistently to features and gene-level summaries as needed

## Key implementation points

- distance to telomere must be a function of sequence length and coordinate position
- telomere annotation can be used for historical or structural interpretation, but it is not required for the basic metric
- centromere-aware distance metrics are needed for H6 and H7, but they remain downstream of the more basic BED and sequence metadata work

## Deliverables

- window-distance metrics derived from sequence architecture
- consistent feature and gene-level spatial summaries
- a reusable spatial-metric layer for downstream analyses

## Acceptance criteria

- distance to telomere can be computed reliably without telomere annotations
- downstream analyses can use spatial relationships for window- and feature-level comparisons
- centromere-aware metrics are available where centromere metadata exists

## Dependencies

- Phase 1 baseline import
- Phase 2 lineage enrichment
- Phase 3 generic bed metric framework

## Exit criteria

The dataset is ready for the main chromosome architecture and evolutionary-distance analyses central to the BoaT hypotheses.
