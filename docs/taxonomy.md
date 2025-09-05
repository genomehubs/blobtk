# blobtk taxonomy

Taxonomy parsing and name matching functions

## GoaT backbone taxonomy

For Genomes on a Tree ([GoaT](https://goat.genomehubs.org)), multiple sources of taxonomy data and additional synonyms are combined into a single taxdump ready for use with the [GenomeHubs](https://github.com/genomehubs/genomehubs) `genomehubs init` command.

This is a worked example detailing the steps and commands used to generate the processed taxdump.

### Fetch input files

```
INPUT_DIR=`pwd`/blobtk-input

# NCBI taxdump
mkdir -p $INPUT_DIR/ncbi-taxdump
curl -L "https://ftp.ncbi.nlm.nih.gov/pub/taxonomy/taxdump.tar.gz" -o $INPUT_DIR/ncbi-taxdump/taxdump.tar.gz
tar xf $INPUT_DIR/ncbi-taxdump/taxdump.tar.gz -C $INPUT_DIR/ncbi-taxdump
rm $INPUT_DIR/ncbi-taxdump/taxdump.tar.gz

# ToLID prefixes
mkdir -p $INPUT_DIR/tolid-prefixes
curl -L "https://gitlab.com/wtsi-grit/darwin-tree-of-life-sample-naming/-/raw/master/tolids.txt?ref_type=heads" -o $INPUT_DIR/tolid-prefixes/tolids.txt

# ADD tolids.names.yaml from somewhere

# OTT taxonomy
mkdir -p $INPUT_DIR/ott-taxonomy
OTT_JSON=$(curl -X POST -s https://api.opentreeoflife.org/v3/taxonomy/about)
OTT_VERSION=$(echo "$OTT_JSON" | jq -r '.source | sub("draft";".")')
OTT_MAJOR_VERSION=$(echo "$OTT_JSON" | jq -r '.name + .version')
curl -L "https://files.opentreeoflife.org/ott/${OTT_MAJOR_VERSION}/${OTT_VERSION}.tgz" -o $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz
tar xf $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz -C $INPUT_DIR/ott-taxonomy
mv $INPUT_DIR/ott-taxonomy/${OTT_VERSION}/* $INPUT_DIR/ott-taxonomy/
rm $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz
rmdir $INPUT_DIR/ott-taxonomy/${OTT_VERSION}

# GBIF taxonomy
mkdir -p $INPUT_DIR/gbif-backbone
curl -L https://hosted-datasets.gbif.org/datasets/backbone/current/simple.txt.gz -o $INPUT_DIR/gbif-backbone/simple.txt.gz
$INPUT_DIR/gbif-backbone/simple.txt.gz

# ENA taxonomy extra
$HOME/projects/genomehubs/goat-data/scripts/update-resources/get-ena-taxonomy-extra.py 6447
```

These will be imported in the order:

1. NCBI taxdump
2. ENA taxonomy extra
3. ToLID prefixes
4. GBIF taxonomy (names only)
5. OTT taxonomy (names only)

### Set up config file

`blobtk taxonomy` accepts command line arguments and/or a config file to define taxonomy import settings. When combining multiple taxonomies, setting options for the second and subsequent taxonomies is only possible using a YAML config file.

`$INPUT_DIR/config.yaml`

```yaml
path: ./blobtk-input/ncbi-taxdump
out: ./blobtk-output/combined-taxdump
taxonomy_format: ncbi
root_taxon_id:
  - 2759
base_taxon_id: 1
name_classes:
  - scientific name
  - synonym
  - merged taxon id
taxonomies:
  #   - path: ./blobtk-input/ena/ena-taxonomy.extra.jsonl
  #     taxonomy_format: ena
  #     xref_label: ena
  #   - path: test/taxonomy/canidae/genomehubs/tolids.names.yaml
  #     taxonomy_format: genomehubs
  #     name_classes:
  #       - scientific name
  #     xref_label: tolid
  #     create_taxa: true
  # - path: ./blobtk-input/gbif-backbone/simple.txt.gz
  #   taxonomy_format: gbif
  #   root_taxon_id:
  #     - 0
  #     - 1
  #     - 4
  #     - 5
  #     - 6
  #     - 7
  #   base_taxon_id: root
  #   name_classes:
  #     - scientific name
  #   xref_label: gbif
  - path: ./blobtk-input/ott-taxonomy
    taxonomy_format: ott
    root_taxon_id:
      - 304358
    base_taxon_id: 304358
    name_classes:
      - scientific name
    xref_label: ott
```

### Run blobtk

```
./blobtk taxonomy -c $INPUT_DIR/config.yaml
```

## GenomeHubs file validation
