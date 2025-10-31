# Snail Plots

Reproducible examples for the snail plot manuscript.

## Figure 1

Cornu aspersum GCA_964187895.1 xgCorAspe16.1

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_964187895.1 --scale-function sqrt -o GCA_964187895.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_964187895.1 --scale-function linear -o GCA_964187895.1.snail.linear.png
```

Cepaea nemoralis GCA_964147875.1 xgCepNemo3.hap1.1

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_964147875.1 --scale-function sqrt -o GCA_964147875.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_964147875.1 --scale-function linear -o GCA_964147875.1.snail.linear.png

```

Cepaea nemoralis GCA_014155875.1 NBC_Cnem_1.0

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/JACEFZ01 --scale-function sqrt -o GCA_014155875.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/JACEFZ01 --scale-function linear -o GCA_014155875.1.snail.linear.png
```

Associated assembly statistics

```
curl -H 'accept: text/tab-separated-values' "https://goat.genomehubs.org/api/v2/search?query=assembly_id%3DGCA_964187895.1%2CGCA_964147875.1%2CGCA_014155875.1&result=assembly&taxonomy=ncbi" > f1-assembly-data.tsv
```

## Figure 2

Subsample from goat search of contig vs scaffold n50

```
https://goat.genomehubs.org/search?query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000&result=assembly&taxonomy=ncbi&size=10&report=scatter&y=contig_n50&plotRatio=auto&pointSize=15&xField=scaffold_n50&xOpts=10000%3B10000000000%3B7&yOpts=1000%3B1000000000%3B7
```

GoaT results to TSV

```
curl -H 'accept: text/tab-separated-values' "https://goat.genomehubs.org/api/v2/search?query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000&result=assembly&taxonomy=ncbi&size=10000&report=scatter&y=contig_n50&plotRatio=auto&pointSize=15&xField=scaffold_n50&xOpts=10000%3B10000000000%3B7&yOpts=1000%3B1000000000%3B7" > f2-assembly-data.raw.tsv

```
