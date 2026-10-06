# Blobs on a Tree (BoaT): Database Architecture & Operational Plan

This document serves as the definitive technical blueprint and database schema specification for **Blobs on a Tree (BoaT)** (`boat.genomehubs.org`), the coordinate-level sister platform and analytical companion to **Genomes on a Tree (GoaT)**.

Unlike GoaT, which acts as a global administrative tracking ledger for species and assemblies, BoaT is an analytical database designed to index, query, and analyze **interior coordinate-level sequence statistics, syntenic landmarks, and functional annotations** across the eukaryotic Tree of Life.

---

## 1. The 7 Core Evolutionary Hypotheses

BoaT is structured to support large-scale, phylogenetically corrected comparative genomic analyses. Every attribute indexed serves a specific mathematical purpose to test one or more of the following fundamental hypotheses:

### Hypothesis 1: The Recombination-GC Drive (GC-Biased Gene Conversion)

- **Theory:** Meiotic recombination systematically drives GC-biased gene conversion (gBGC) by favoring G/C nucleotides during double-strand break repair. Because recombination rates are systematically higher on smaller chromosomes (due to the obligate chiasma rule) and near telomeric regions, window-based GC-content is predicted to negatively correlate with chromosome size and positively correlate with physical proximity to active telomeres.
- **Mathematical Model:**
  $$\text{Window GC} \sim \beta_0 + \beta_1(\text{Distance to Telomere}) + \beta_2(\text{Scaffold Length}) + \text{Phylogenetic Covariance}$$

### Hypothesis 2: Structural Determinants of Karyotypic Stasis

- **Theory:** Eukaryotic lineages vary from extreme karyotypic stasis (preserving syntenic markers across hundreds of millions of years) to rapid karyotypic disruption. High localized repeat-mask densities and transposable element (TE) insertions are hypothesized to act as physical substrate hotspots for non-allelic homologous recombination, triggering structural rearrangement. Syntenic boundaries and Ancestral Linkage Group (ALG) breaks will statistically co-localize with dense repeat-mask and transposon zones.

### Hypothesis 3: The Tempo of Sex Chromosome Degeneration

- **Theory:** Once recombination is suppressed on sex-determining chromosomes (Y or W), natural selection is severely degraded. This triggers rapid pseudogenization, transposable element invasion, and heterochromatic decay. Suppressed sex scaffolds will display a parallel, predictable trajectory of rapid repeat accumulation, high assembly gap density, and a collapse in single-copy ortholog (BUSCO) completeness relative to autosomes of the same assembly.

### Hypothesis 4: Biophysical Barriers to Eukaryotic Assembly

- **Theory:** Chromosomal assembly "gaps" (N-runs) in reference genomes are not randomly distributed. They are driven by localized sequence complexity thresholds (e.g., extreme GC transitions, satellite expansions, or specific transposable element families) that represent physical limits to current long-read sequencing technologies (Oxford Nanopore vs. PacBio HiFi).

### Hypothesis 5: Spatial Compartmentalization of Gene Function

- **Theory:** Genomes partition gene functions along chromosomes to balance stability and adaptability. Essential, highly conserved housekeeping genes are concentrated in stable, repeat-depleted chromosome interiors. In contrast, genes mediating environmental interactions and defense (e.g., immune receptors, cytochrome P450s, detoxification pathways) cluster in dynamic sub-telomeric or repeat-dense regions where duplications and sequence divergence are tolerated.

### Hypothesis 6: Centromere-Driven Molecular Decay

- **Theory:** In monocentric species, pericentromeric regions undergo extreme suppression of meiotic recombination. This forms an evolutionary "sinkhole" where natural selection is inefficient, leading to the rapid accumulation of satellite repeats and transposable elements, and the systematic exclusion of active protein-coding genes.
- **Mathematical Model:**
  $$\text{Window Repeat Density} \sim \beta_0 - \beta_1(\text{Distance to Centromere}) + \text{Phylogenetic Covariance}$$

### Hypothesis 7: Holocentric Escape from Decay Sinkholes

- **Theory:** Holocentric lineages (e.g., Lepidoptera, nematodes, certain plants) lack localized centromeric constrictions; spindle fibers attach across the entire chromosome length. Consequently, they lack localized pericentromeric recombination-suppression zones. Holocentric chromosomes are predicted to exhibit significantly more homogeneous, uniform spatial distributions of repeat densities and coding genes across binned windows compared to monocentric chromosomes of similar size.

---

## 2. Decoupled Index Architecture

To balance search performance, taxonomic flexibility, and database scaling limits, BoaT divides data into **four specialized, decoupled indexes**.

```
                               ┌────────────────────────┐
                               │   GoaT TAXA Index      │ (Decoupled Taxonomy)
                               └───────────┬────────────┘
                                           │
             ┌─────────────────────────────┼─────────────────────────────┐
             ▼                             ▼                             ▼
┌────────────────────────┐    ┌────────────────────────┐    ┌────────────────────────┐
│     scaffolds Index    │    │     windows Index      │    │     features Index     │
├────────────────────────┤    ├────────────────────────┤    ├────────────────────────┤
│ • Assembly-level       │    │ • 1 Mb non-overlapping │    │ • Coordinate-focused   │
│ • Sparse metadata      │    │ • Statistical ledger   │    │ • Millions of items    │
│ • 1 doc / chromosome   │    │ • GC, repeat, gap dens │    │ • BUSCOs, ALGs, repeats│
└────────────────────────┘    └────────────────────────┘    └────────────────────────┘
                                           │
                                           │ (Denormalization)
                                           ▼
                              ┌────────────────────────┐
                              │      genes Index       │
                              ├────────────────────────┤
                              │ • Detailed function    │
                              │ • GO terms & Pfams     │
                              │ • Denormalized metrics │
                              └────────────────────────┘
```

### 1. `scaffolds` Index

- **Purpose:** Stores sequence-level metadata and chromosomal landmarks.
- **Granularity:** One document per chromosome or scaffold.
- **Curation Gate:** Restricted to representative reference-quality (chromosome-level) assemblies, indexing exactly one curated genome build per species.
- **Key Fields:** Assembly metadata, sequencer chemistry, centromere type, centromere coordinates, and chromosome conformation.

### 2. `windows` Index

- **Purpose:** The primary statistical ledger for modeling sliding-window landscapes.
- **Granularity:** **Non-overlapping, adjacent 1 Mb binned (tessellated) windows** (e.g., Bin 1: 1–1Mb; Bin 2: 1M–2Mb).
  - _Why non-overlapping:_ Overlapping windows introduce severe artificial statistical autocorrelation (violating standard independent-distribution assumptions in comparative evolutionary models) and inflate the index footprint up to 10x.
- **Key Fields:** Binned GC ratios, repeat densities, gap density, satellite repeat ratios, gene counts, and partitioned housekeeping vs. immune counts.

### 3. `features` Index

- **Purpose:** Houses millions of raw coordinate-based annotations.
- **Granularity:** One document per individual coordinate feature (e.g., a single transposable element, a BUSCO locus, or an ALG syntenic block).
- **Key Fields:** Coordinate spans (`start`/`end`), feature type, feature ID, and parent scaffold ID.
- **Anti-Bloat Strategy:** Highly lightweight documents. String descriptions, Gene Ontology lists, and protein domain arrays are strictly excluded from this index.

### 4. `genes` Index

- **Purpose:** Provides a rich, searchable functional atlas for protein-coding and non-coding gene models.
- **Granularity:** One document per annotated gene (~15,000 to 30,000 per genome).
- **Key Fields:** Gene ID, symbol, description, InterPro/Pfam domains, GO terms, and **denormalized spatial metrics** (distance to telomere, distance to centromere, local 1 Mb window GC, local window repeat density).
- **Anti-Bloat Strategy:** Restricted to active gene annotations. Keeping these heavy functional string variables separate from the high-volume `features` index keeps coordinate queries exceptionally fast.

---

## 3. Schema Attribute Specification Table

| Index        | Attribute Name                | Elasticsearch Type | Unit / Format                                                | Related Hypotheses | Technical Definition & Rules                                                               |
| :----------- | :---------------------------- | :----------------- | :----------------------------------------------------------- | :----------------- | :----------------------------------------------------------------------------------------- |
| **Scaffold** | `scaffold_id`                 | `keyword`          | String (GenBank Accession)                                   | All                | Unique sequence-level identifier.                                                          |
| **Scaffold** | `scaffold_length`             | `long`             | Base pairs (bp)                                              | H1, H3             | Total physical span of the sequence.                                                       |
| **Scaffold** | `scaffold_type`               | `keyword`          | `chromosome` \| `unplaced`                                   | H1, H5             | Filters out fragmented drafts from chromosomal models.                                     |
| **Scaffold** | `centromere_type`             | `keyword`          | `monocentric` \| `holocentric` \| `polycentric` \| `unknown` | H6, H7             | Dictates how downstream window distances are derived.                                      |
| **Scaffold** | `centromere_start`            | `long`             | Coordinates (bp)                                             | H6                 | Start of centromeric constriction (nullable).                                              |
| **Scaffold** | `centromere_end`              | `long`             | Coordinates (bp)                                             | H6                 | End of centromeric constriction (nullable).                                                |
| **Scaffold** | `sequencing_platform`         | `keyword`          | `PacBio_HiFi` \| `ONT` \| `ONT+HiFi`                         | H4                 | Identifies primary raw sequence read technology.                                           |
| **Scaffold** | `assembly_curation`           | `keyword`          | `manually_curated` \| `raw_pipeline`                         | H4                 | Flags if manual curation (e.g., PretextView) was performed.                                |
| **Feature**  | `feature_type`                | `keyword`          | `BUSCO_locus` \| `repeat_element` \| `ALG_block`             | H2, H3, H5         | Classification discriminator.                                                              |
| **Feature**  | `feature_name`                | `keyword`          | String (e.g., `EOG090X`, `Gypsy_element`)                    | H2, H3             | Feature identifier or family name.                                                         |
| **Feature**  | `start` / `end`               | `long`             | Coordinates (bp)                                             | H2, H3             | Exact physical span on the sequence.                                                       |
| **Gene**     | `gene_id`                     | `keyword`          | String (e.g., `DenFor_G00124`)                               | H5                 | Unique gene locus identifier.                                                              |
| **Gene**     | `gene_symbol`                 | `keyword`          | String (e.g., `CYP4G1`)                                      | H5                 | Common standard nomenclature (nullable).                                                   |
| **Gene**     | `description`                 | `text`             | String                                                       | H5                 | Full-text functional description (analyzed).                                               |
| **Gene**     | `domains`                     | `keyword`          | Array (e.g., `["PF00067", "IPR001180"]`)                     | H5                 | Indexed protein domain identifiers.                                                        |
| **Gene**     | `go_terms`                    | `keyword`          | Array (e.g., `["GO:0005506"]`)                               | H5                 | Indexed Gene Ontology identifiers.                                                         |
| **Gene**     | `distance_to_telomere`        | `long`             | Base pairs (bp)                                              | H1, H5             | Calculated as: $\min(\text{midpoint}, \text{scaffold\_length} - \text{midpoint})$.         |
| **Gene**     | `distance_to_centromere`      | `long`             | Base pairs (bp)                                              | H6, H7             | Midpoint to centromere boundary. **Rule:** Set to `0` if `centromere_type == holocentric`. |
| **Gene**     | `local_window_gc`             | `float`            | Proportion (0.000 to 1.000)                                  | H1, H5             | **Denormalized:** GC of the 1 Mb window enclosing the gene midpoint.                       |
| **Gene**     | `local_window_repeat_density` | `float`            | Proportion (0.000 to 1.000)                                  | H2, H5             | **Denormalized:** Repeat density of the 1 Mb window enclosing the gene midpoint.           |
| **Window**   | `window_id`                   | `keyword`          | String (`{scaffold}_win_{number}`)                           | All                | Unique window document identifier.                                                         |
| **Window**   | `window_start` / `_end`       | `long`             | Coordinates (bp)                                             | All                | Boundaries of the 1 Mb physical bin.                                                       |
| **Window**   | `window_gc`                   | `float`            | Proportion (0.000 to 1.000)                                  | H1, H4             | GC-content of binned sequence.                                                             |
| **Window**   | `window_repeat_density`       | `float`            | Proportion (0.000 to 1.000)                                  | H2, H3, H5         | Total bases masked by repeat element spans in the window.                                  |
| **Window**   | `window_satellite_density`    | `float`            | Proportion (0.000 to 1.000)                                  | H6, H7             | Proportions of bases matching satellite repeat elements.                                   |
| **Window**   | `window_gap_density`          | `float`            | Proportion (0.000 to 1.000)                                  | H3, H4             | Total assembly gaps (runs of Ns) binned in the window.                                     |
| **Window**   | `window_telomere_density`     | `float`            | Proportion (0.000 to 1.000)                                  | H1                 | Proportion of sequence matching species telomeric motif.                                   |
| **Window**   | `window_gene_density`         | `float`            | Proportion (0.000 to 1.000)                                  | H5, H6             | Proportion of window sequence coding for exons.                                            |
| **Window**   | `window_housekeeping_count`   | `integer`          | Count                                                        | H5                 | Total genes matching core-physiological GO terms.                                          |
| **Window**   | `window_immune_gene_count`    | `integer`          | Count                                                        | H5                 | Total genes matching environmental interaction GO terms.                                   |

---

## 4. Taxonomic Decoupling & Query-Time Enrichment

To prevent taxonomic updates (splits, merges, renames) from forcing expensive re-indexing of millions of window and feature documents, BoaT implements a **Decoupled Taxonomic Query Model**.

### 1. No Taxonomic Strings in Windows or Features

Window and feature documents store **only** two numeric taxonomic keys:

- **`taxon_id`** _(Integer)_: The direct NCBI taxon ID of the species.
- **`ancestral_taxid`** _(Integer Array)_: The path-enumeration list of ancestral taxids (e.g., `ancestral_taxid: [2759, 33208, 6072, 7088, 104385]`).

### 2. Query-Time Descent Routing

When a user searches for sequences in the order `"Lepidoptera"` (TaxID: 7088):

1.  The API intercepts the query and scans the lightweight, independent **`taxa` index** (which is synced nightly with NCBI taxonomy).
2.  The API extracts the array of descendant `taxon_id` values under Lepidoptera.
3.  The API executes a highly efficient `terms` query on the `windows` or `features` index matching those numeric keys:
    ```json
    {
      "query": {
        "terms": {
          "taxon_id": [7088, 7089, 7123, 7124]
        }
      }
    }
    ```

### 3. Render-Time UI Enrichment

To allow the front-end to draw plots grouped by taxonomic strings (e.g., color-coding scatter plots by "Family"):

1.  The search engine returns a page of 100 binned window records containing only coordinates and metrics.
2.  The API extracts the 2 or 3 unique species `taxon_id` keys present in those 100 documents.
3.  The API performs a single fast `MGET` against the `taxa` index to retrieve the current, authoritative names (e.g., `"Veneridae"`, `"Tenebrionidae"`) for those IDs.
4.  The API decorates the JSON payload on the fly before sending it to the user interface.

_This decouples our massive physical sequence metrics completely from taxonomic churn, making our data storage 100% immutable and resilient._

---

## 5. 10-Step Ingestion & Pipeline Sequence

The integration of functional annotations and cytogenetic metrics is achieved via a strict, one-way sequential pipeline that resolves spatial dependencies during the ingestion phase:

1.  **Step 1: Parse NCBI Sequence Reports**
    Read sequence-level report tables. Establish taxonomic links, determine total sequence spans, scaffold counts, and chromosome designations. Define physical coordinates for the non-overlapping 1 Mb window boundaries.
2.  **Step 2: Load Ancestral Linkage Group (ALG) Matrices**
    Parse syntenic lookup maps and assign ancestral karyotypic block flags to chromosome regions.
3.  **Step 3: Parse Ortholog Completeness (BUSCO) Tables**
    Read full-table outputs from the BUSCO pipeline to identify physical coordinate arrays of conserved single-copy loci.
4.  **Step 4: Define Synteny Blocks & Index Loci**
    Run synteny block reconstruction, determine breakpoints, and write orthologous loci to the **`features` index** as coordinate-only elements.
5.  **Step 5: Load GFF3 and Functional Annotations (Genes)**
    Ingest GFF3 coordinate fields, InterProScan domain signatures, and GO term mappings. Initialize these records as base files in the **`genes` index**.
6.  **Step 6: Read High-Resolution sequence BED Files**
    Incorporate raw 1 kb sliding window BED files containing sequence metrics (GC content, entropy, gap density, and sequencing coverage).
7.  **Step 7: Group BED Data into Tessellated Windows**
    Aggregate the high-resolution 1 kb data blocks mathematically into discrete, non-overlapping 1 Mb physical windows.
8.  **Step 8: Finalize Scaffold Metadata & Index**
    Calculate physical chromosome statistics (sizes, curation statuses, sequencer type, and centromeric types/coordinates). Write the records to the **`scaffolds` index**.
9.  **Step 9: Compile Window Statistics & Index**
    Add localized repeat mask distributions, satellite counts, gene density counts, and core-GO counts. Write the finalized window metrics to the **`windows` index**.
10. **Step 10: Midpoint Spatial Match & Denormalize**
    Run a single-pass spatial lookup matching the midpoint coordinate of every gene in the `genes` index against the finalized 1 Mb window boundaries. Write `local_window_gc`, `local_window_repeat_density`, `distance_to_telomere`, and `distance_to_centromere` directly onto the **`genes` index**.

---

## 6. Batch-Normalised Reference Cache for Lineage-Aware Scaling

The normalization layer must be explicit and deterministic. It cannot be implemented as a filesystem guessing game in which the importer scans arbitrary directories and tries to infer which assemblies belong to the same biological batch. That would make imports non-reproducible, fragile to missing or stale files, and dependent on local directory structure rather than the taxonomic and configuration metadata that define the analysis.

### 6.1 Design intent

The importer remains centred on the per-assembly configuration file as the primary unit of work. Each assembly import still reads its own sequence, BUSCO, and BED inputs, but the normalization stage is permitted to consult a separate batch manifest that defines which related assemblies should contribute to the baseline for each taxon.

This creates a clean split:

- Assembly config = the direct import unit
- Batch manifest = the normalization context
- Lineage provider = the source of the taxonomic hierarchy used to resolve fallback batches

This preserves the current import architecture while enabling the family/order/class/phylum fallback rules described in the statistical framework.

### 6.2 Where the grouping information comes from

There are two distinct sources of truth, and they should not be conflated:

1. **Taxonomic grouping** comes from the lineage provider (GoAT or an equivalent source), which resolves the focal assembly into a hierarchical family/order/class/phylum context.
2. **Operational grouping** comes from an explicit batch manifest, which lists the participating assemblies for a given normalization batch.

The practical rule is:

- the lineage service determines the candidate comparison level and fallback path
- the manifest determines the actual set of assemblies contributing to that batch

This avoids wild card directory scans while still allowing taxonomically guided batching.

### 6.3 Recommended batch manifest format

The manifest should be explicit YAML rather than ad hoc file discovery. A minimal structure is:

```yaml
batches:
  - id: family_12345_gc
    rank: family
    taxon_id: 12345
    lineage_name: Mycetophilidae
    metric_families:
      - gc
      - repeat
      - satellite
    members:
      - assembly_id: GCA_000001
        config: ../assemblies/GCA_000001/import.yaml
      - assembly_id: GCA_000002
        config: ../assemblies/GCA_000002/import.yaml
      - assembly_id: GCA_000003
        config: ../assemblies/GCA_000003/import.yaml
```

Each assembly config can then opt into the batch by reference:

```yaml
normalization:
  enabled: true
  batch_manifest: ../batches/normalization.yaml
  batch_id: family_12345_gc
  metric_families:
    - gc
    - repeat
    - satellite
```

This keeps the importer deterministic and makes missing or stale batch members fail clearly during validation rather than silently disappearing in a path scan.

### 6.4 Why not scan a directory as the default runtime logic?

Directory scanning can be useful as a helper that generates a manifest, but it should not be the core implementation strategy. The default runtime path should remain:

- assembly config defines the input
- batch manifest defines the batch set
- lineage metadata defines the fallback hierarchy

A directory scan is acceptable only as a bootstrap convenience for generating an initial manifest from a known collection of assembly configs. After that, the manifest becomes the authoritative record. This is much easier to debug, much easier to validate, and much easier to reproduce in a batch import run.

### 6.5 Execution order for a normalized import

The normalization pipeline should run in a staged way, not by trying to resolve everything inside a single import pass:

1. **Resolve lineage context**
   - read the focal assembly's taxon ID and lineage metadata
   - determine the family/order/class/phylum fallback path
   - select the target batch rank according to the minimum-size rule

2. **Load the batch manifest**
   - confirm that the batch exists
   - confirm that each member assembly is present and has the required files
   - reject missing batch members early

3. **Precompute the normalization baselines**
   - read only the relevant data sources for the selected metric family
   - compute summary statistics for equivalent coordinate bins or equivalent window classes
   - store a compact summary rather than raw arrays

4. **Cache the results**
   - key by batch ID, metric family, coordinate system, and version
   - persist in memory for a single batch and optionally to disk for repeat runs

5. **Run the full assembly import**
   - import the assembly's sequence/BUSCO/BED data normally
   - apply the cached baseline in the correct normalization path for biological and technical metrics

This produces one effective import flow for the assembly while ensuring the batch statistics are computed once and reused efficiently across the relevant set of taxa.

### 6.6 Cache structure and data content

The normalization cache should be compact and analysis-specific. It should not store raw BED rows or full coordinate arrays. It should store only the values needed to compute the next-stage Z-score or relative scaling:

- batch ID
- metric family
- coordinate system
- count of assemblies in the batch
- mean
- standard deviation
- median and MAD where robust fallback is useful
- coordinate bin or bin-class mapping
- version stamp for the underlying metric definition

This keeps the cache small enough to be reused across the same taxonomy-informed import run while still supporting downstream regression models and taxonomic fallback logic.

### 6.7 Biological vs technical normalization path

The implementation should retain the split already defined in the statistical framework:

- **Biological metrics**: GC content, repeat density, satellite fraction
  - normalize directly against equivalent coordinates in the batch distribution
- **Technical metrics**: depth/coverage, assembly gap burden
  - first compute the assembly-relative value, then normalize against the batch distribution for equivalent coordinates

These should be computed through separate cache entries and separate fallback logic because the transformation is not equivalent in meaning or variance structure.

### 6.8 Failure and validation rules

The batch normalization workflow should fail early with explicit messages when any of the following occur:

- no manifest is provided for a batch-enabled import
- the manifest does not define the required batch ID
- a batch member is missing its config file or required source files
- the batch size is too small even after fallback to higher ranks
- the batch contains incompatible coordinate systems or incompatible metric definitions

This is essential because silent fallback to an undersized or mismatched batch would undermine the statistical assumptions behind the model.

### 6.9 Summary

The concrete design is therefore:

- keep the assembly config as the import unit
- add a batch manifest as the explicit normalization context
- resolve the batch by lineage fallback, not by directory scanning
- compute batch baselines once and cache them
- apply those cached baselines during the actual assembly import

This is the implementation pattern that keeps the import code deterministic, efficient, and faithful to the lineage-aware normalization model described in the operational plan.

---

## 7. Architecture & Bloat Review Checklist

To safeguard the performance of our Elasticsearch cluster, the development team must verify adherence to the following four performance constraints:

- [ ] **Zero Nested Join Queries:** Are all structural, physical, and environmental metrics needed for Hypotheses 1–7 pre-calculated and stored directly on the document being queried?
- [ ] **Curation Filter in Place:** Are windowing and coordinate computations restricted strictly to `scaffold_type: chromosome` for representative builds?
- [ ] **Midpoint Assignment Confirmed:** Are genes assigned to exactly one window using midpoint-coordinate matching to prevent multi-document duplication?
- [ ] **Decoupled Taxon Model Active:** Are textual taxonomic strings completely absent from the `windows`, `features`, and `genes` indices to prevent re-indexing cycles?
