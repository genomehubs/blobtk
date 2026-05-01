# Feature Gates for Taxonomy Improvements - User Guide

## Overview

The taxonomy improvements (Fixes #1-#3) are gated behind feature flags for safe A/B testing and gradual production rollout. All gates default to `false` to preserve existing behavior and enable easy rollback.

## Configuration

### Via YAML Config File

Add an `experimental_fixes` section to your taxonomy configuration file:

```yaml
path: test/taxonomy/canidae/ncbi
out: test/taxonomy/canidae/combined
output_format:
  - jsonl
  - ncbi
taxonomy_format: ncbi
root_taxon_id:
  - 33208
base_taxon_id: 33208

# Enable experimental taxonomy improvements
experimental_fixes:
  bracket_name_stripping: true # Fix #1: Strip [Family] sp. patterns
  genus_rank_filtering: true # Fix #2: Prefer genus rank for synonyms
  multimatch_candidate_ranking: true # Fix #3: Rank candidates by match quality

taxonomies:
  - path: test/taxonomy/canidae/ena/ena-taxonomy.extra.jsonl
    taxonomy_format: ena
    xref_label: ena
    create_taxa: true
```

## Individual Fixes

### Fix #1: `bracket_name_stripping`

**Enabled by:** `experimental_fixes.bracket_name_stripping: true`

**What it does:**

- Strips brackets from incertae sedis names like `[Family] sp.`
- Before: `[Amblyosporidae]` → looked up as literal bracketed string
- After: `[Amblyosporidae]` → stripped to `Amblyosporidae` → properly found in backbone

**Use case:** When your JSONL contains taxa like:

```json
{ "scientificName": "[Amblyosporidae] sp. gmOTU1", "rank": "species" }
```

**Affected pattern:** `[Any] sp. [extra]`

---

### Fix #2: `genus_rank_filtering`

**Enabled by:** `experimental_fixes.genus_rank_filtering: true`

**What it does:**

- When multiple genus entries exist (primary name + synonyms), selects the one with `rank == "genus"`
- Before: `.first()` picked arbitrarily from list
- After: Filters for genus rank, ensuring deterministic parent selection

**Use case:** When your backbone has genus synonyms and you're adding:

```json
{ "scientificName": "Nosema sp. gmOTU18", "rank": "species" }
```

**Problem it solves:**

- Non-deterministic parent selection when genus has GBIF/OTT synonyms
- Prevents linking species to species entries instead of genus entries
- Ensures consistent ancestor lineage

---

### Fix #3: `multimatch_candidate_ranking`

**Enabled by:** `experimental_fixes.multimatch_candidate_ranking: true`

**What it does:**

- When multiple candidates match a family name, ranks them by:
  1. Exact rank match (prefers exact rank over other ranks)
  2. Order of occurrence
- Before: Used unsorted first() from arbitrary list
- After: Sorted candidates ensure better ancestor context

**Use case:** Family names that have multiple entries (synonyms, merged taxa)

---

## Deployment Strategies

### Strategy 1: Gradual Rollout

1. **Phase 1: Canary** (1-5% of data)

   ```yaml
   experimental_fixes:
     bracket_name_stripping: true # Only Fix #1
   ```

2. **Phase 2: Expand** (25% of data)

   ```yaml
   experimental_fixes:
     bracket_name_stripping: true
     genus_rank_filtering: true # Add Fix #2
   ```

3. **Phase 3: Full** (100% of data)
   ```yaml
   experimental_fixes:
     bracket_name_stripping: true
     genus_rank_filtering: true
     multimatch_candidate_ranking: true # All fixes
   ```

### Strategy 2: A/B Testing

Run two parallel pipelines:

**Pipeline A (Control):**

```yaml
experimental_fixes:
  bracket_name_stripping: false
  genus_rank_filtering: false
  multimatch_candidate_ranking: false
```

**Pipeline B (Test):**

```yaml
experimental_fixes:
  bracket_name_stripping: true
  genus_rank_filtering: true
  multimatch_candidate_ranking: true
```

Compare outputs and metrics before full rollout.

---

## Monitoring & Rollback

### What to Monitor

1. **Bracket stripping (Fix #1):**
   - Count of `anc_[` nodes created (should decrease)
   - Count of successfully linked species with `[` in names
   - Success rate for Microsporidia taxa

2. **Genus filtering (Fix #2):**
   - Non-determinism check: Run same input twice, verify identical output
   - Parent rank consistency: Verify all species parents have `rank == "genus"`
   - Nosema sp. variant insertion success rate

3. **MultiMatch ranking (Fix #3):**
   - Candidate selection consistency
   - Ancestor lineage changes
   - Taxa successfully linked to correct family

### Rollback Procedure

If issues detected, instantly rollback:

```yaml
experimental_fixes:
  bracket_name_stripping: false
  genus_rank_filtering: false
  multimatch_candidate_ranking: false
```

Re-run pipeline - will use original behavior. No code changes needed.

---

## Testing Locally

### Enable All Fixes

```bash
cd /Users/rchallis/projects/blobtoolkit/blobtk/rust

# With all fixes enabled
cat > test/taxonomy/config_test_all_fixes.yaml << 'EOF'
path: test/taxonomy/canidae/ncbi
out: test/taxonomy/canidae/combined_all_fixes
output_format:
  - jsonl
  - ncbi
taxonomy_format: ncbi
root_taxon_id:
  - 33208
base_taxon_id: 33208

experimental_fixes:
  bracket_name_stripping: true
  genus_rank_filtering: true
  multimatch_candidate_ranking: true

taxonomies:
  - path: test/taxonomy/canidae/ena/ena-taxonomy.extra.jsonl
    taxonomy_format: ena
    xref_label: ena
    create_taxa: true
EOF

cargo run -- taxonomy -c test/taxonomy/config_test_all_fixes.yaml
```

### Compare Results

```bash
# Control (no fixes)
cargo run -- taxonomy -c test/taxonomy/config_canidae_no_fixes.yaml \
  -O test/taxonomy/canidae/combined_no_fixes

# Test (all fixes)
cargo run -- taxonomy -c test/taxonomy/config_test_all_fixes.yaml \
  -O test/taxonomy/canidae/combined_all_fixes

# Compare outputs
diff test/taxonomy/canidae/combined_no_fixes/*.jsonl \
     test/taxonomy/canidae/combined_all_fixes/*.jsonl
```

---

## Implementation Details

### Code Locations

| Fix | File                                            | Lines   | Gate                           |
| --- | ----------------------------------------------- | ------- | ------------------------------ |
| #1  | [src/parse.rs](src/parse.rs#L260)               | 260-285 | `bracket_name_stripping`       |
| #2  | [src/parse.rs](src/parse.rs#L290)               | 290-305 | `genus_rank_filtering`         |
| #3  | [src/parse/lookup.rs](src/parse/lookup.rs#L550) | 565-580 | `multimatch_candidate_ranking` |

### Feature Gate Struct

Defined in [src/parse/feature_gates.rs](src/parse/feature_gates.rs):

```rust
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct ExperimentalFixes {
    pub bracket_name_stripping: bool,
    pub genus_rank_filtering: bool,
    pub multimatch_candidate_ranking: bool,
}
```

### Configuration Flow

```
CLI/YAML config
      ↓
TaxonomyOptions.experimental_fixes
      ↓
load_options() merges from YAML
      ↓
passed to parse functions
      ↓
Checked at decision points
      ↓
Original behavior if false
New behavior if true
```

---

## Troubleshooting

### Question: How do I know if a fix is working?

**Answer:** Check the log output and compare node counts:

```bash
# Count anc_ nodes (should decrease with bracket_name_stripping)
grep -c '"anc_' combined/nodes.jsonl

# Check for species with genus parents (should increase with genus_rank_filtering)
grep -c '"species"' combined/nodes.jsonl

# Verify deterministic results (run twice, outputs should be identical)
```

### Question: Which fix should I enable first?

**Answer:** Enable them in order: Fix #1 → Fix #2 → Fix #3

Fix #1 (bracket stripping) is lowest risk - it just cleans up names.
Fix #2 (genus filtering) is higher confidence - it adds explicit rank filtering.
Fix #3 (candidate ranking) is refinement - improves edge cases.

### Question: Can I mix and match?

**Answer:** Yes! You can enable any subset:

- Fix #1 + #2 (common)
- Fix #1 only (conservative)
- All three (full deployment)

There are no dependencies between them.

---

## Safety Guarantees

✅ **Default behavior preserved** - All gates default to `false`
✅ **No code path changes** - Fixes are additive, not replacing
✅ **Easy rollback** - Just set to `false` and re-run
✅ **Independent gates** - Each fix can be toggled separately
✅ **Deterministic** - Same input + same gates = identical output
