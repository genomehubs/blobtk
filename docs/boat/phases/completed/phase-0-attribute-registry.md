# Phase 0: Attribute registry and schema reconciliation

## Objective

Establish a single authoritative, editable schema layer that maps the current importer output to the planned BoaT naming conventions and future analytical requirements.

## Why this comes first

The operational plan and the current import code do not yet use identical attribute names. We need a stable, human-editable translation layer before the rest of the implementation hardens around the schema.

## Scope

- gather all current imported attributes from:
  - sequence report fields
  - BUSCO/synteny fields
  - BED-derived window metrics
  - any currently attached compatibility fields
- map these into canonical BoaT names
- support deprecation and alias tracking
- keep compatibility names available while the schema is still evolving

## Deliverables

- `yaml` file representing the current attribute inventory
- `yaml` file representing the canonical recommended attribute names
- `yaml` file representing compatibility mappings and deprecated aliases
- import-time enum or lookup logic that reads the registry and emits canonical output names where needed

## Acceptance criteria

- any new attribute added to the import pipeline is first registered in the YAML contract
- each attribute has a clear status: active, compatibility, deprecated, or planned
- it is possible to edit the authoritative schema without changing the import pipeline logic structure
- the schema source can be used as a reference for future index or UI work

## Dependencies

- current import output structure
- current feature-window-sequence attribute set
- planning doc naming decisions

## Exit criteria

The attribute registry is the canonical source of naming truth for all subsequent phases.
