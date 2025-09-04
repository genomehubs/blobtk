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

# OTT taxonomy
mkdir -p $INPUT_DIR/ott-taxonomy
OTT_JSON=$(curl -X POST -s https://api.opentreeoflife.org/v3/taxonomy/about)
OTT_VERSION=$(echo "$OTT_JSON" | jq -r '.source | sub("draft";".")')
OTT_MAJOR_VERSION=$(echo "$OTT_JSON" | jq -r '.name + .version')
curl -L "https://files.opentreeoflife.org/ott/${OTT_MAJOR_VERSION}/${OTT_VERSION}.tgz" -o $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz
tar xf $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz -C $INPUT_DIR/ott-taxonomy
rm $INPUT_DIR/ott-taxonomy/$OTT_VERSION.tgz

# GBIF taxonomy
mkdir -p $INPUT_DIR/gbif-backbone
curl -L https://hosted-datasets.gbif.org/datasets/backbone/current/simple.txt.gz -o $INPUT_DIR/gbif-backbone/simple.txt.gz
$INPUT_DIR/gbif-backbone/simple.txt.gz

# ENA taxonomy
/Users/rchallis/projects/genomehubs/goat-data/scripts/update-resources/get-ena-taxonomy-extra.py 6447
```

## GenomeHubs file validation