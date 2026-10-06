# Phase 5: GFF and gene-level annotation extension

Status: not started

> Decision record: this phase remains downstream of the staged config and spatial-metric foundation. It should only begin once the current compatibility and derived-metric scopes are stable and the telomere data dependency is resolved.

## Objective

Add gene-level functional annotation and downstream spatial comparisons once the base coordinate and window framework is stable.

## Scope

- support optional GFF-based gene import for taxa with available annotation files
- parse gene coordinates and functional annotation data
- attach GO terms, domain information, and related gene metadata
- denormalize local window metrics and spatial relationships onto gene records when needed

## Why this is later

The initial BoaT implementation should not block on GFF availability. Gene- and function-level analysis is a downstream layer that depends on the base coordinate and window system being stable and generic.

## Deliverables

- optional gene import path
- GFF compatibility layer
- functional annotation integration
- gene-level denormalization for local window statistics and spatial metrics

## Acceptance criteria

- GFF files can be imported without disrupting the standard sequence/BUSCO/BED path
- functional attributes can be attached to gene records consistently
- gene-level spatial metrics can be compared against local window values

## Dependencies

- Phase 1–4 core import architecture
- GFF examples from taxa with available annotation files

## Exit criteria

The platform is capable of testing functional and gene-centric hypotheses such as H5, with the core coordinate model already in place.
