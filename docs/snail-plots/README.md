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

Mus Musculus GCA_000185125.1 AEKR01

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --scale-function sqrt -o GCA_000185125.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --scale-function sqrt -o GCA_000185125.1.snail.sqrt.scaled_span.png --max-span 2770968735

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --scale-function sqrt -o GCA_000185125.1.snail.sqrt.scaled_scaffold.png --max-span 2770968735 --max-scaffold 200127270

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --scale-function sqrt -o GCA_000185125.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --scale-function linear -o GCA_000185125.1.snail.linear.png --show-numbers
```

Mus musculus GCA_949316315.1 mMusMuc1_1

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --scale-function sqrt -o GCA_949316315.1.snail.sqrt.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --scale-function sqrt -o GCA_949316315.1.snail.sqrt.numbers.png --show-numbers


blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --scale-function linear -o GCA_949316315.1.snail.linear.png --show-numbers

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --scale-function sqrt -o GCA_949316315.1.snail.sqrt.svg

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 -o GCA_949316315.1.snail.yaml
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

## Tree figure

cut -f 6 figure3/figure3_snail_badge_table.tsv
assembly_id
GCA_900322205.1
GCA_003016195.1
GCA_001632505.1
GCA_000261425.2
GCA_002222395.1
GCA_020883555.1
GCA_001661245.1
GCA_000204055.1
GCA_964340765.1
GCA_013467465.1
GCA_014337955.1
GCA_018257905.1
GCA_964340405.1
GCA_000001215.4
GCA_003033685.1
GCA_013339765.2
GCA_963691655.1
GCA_949316315.1
GCA_019009955.1
GCA_964205295.1
GCA_963693085.1

GCA_900322205.1,GCA_003016195.1,GCA_001632505.1,GCA_000261425.2,GCA_002222395.1,GCA_020883555.1,GCA_001661245.1,GCA_000204055.1,GCA_964340765.1,GCA_013467465.1,GCA_014337955.1,GCA_018257905.1,GCA_964340405.1,GCA_000001215.4,GCA_003033685.1,GCA_013339765.2,GCA_963691655.1,GCA_949316315.1,GCA_019009955.1,GCA_964205295.1,GCA_963693085.1

https://goat.genomehubs.org/search?query=assembly_id%3DGCA_900322205.1%2CGCA_003016195.1%2CGCA_001632505.1%2CGCA_000261425.2%2CGCA_002222395.1%2CGCA_020883555.1%2CGCA_001661245.1%2CGCA_000204055.1%2CGCA_964340765.1%2CGCA_013467465.1%2CGCA_014337955.1%2CGCA_018257905.1%2CGCA_964340405.1%2CGCA_000001215.4%2CGCA_003033685.1%2CGCA_013339765.2%2CGCA_963691655.1%2CGCA_949316315.1%2CGCA_019009955.1%2CGCA_964205295.1%2CGCA_963693085.1&result=assembly&includeEstimates=true&taxonomy=ncbi&report=tree&collapseMonotypic=true&treeStyle=ring&treeThreshold=2000&pointSize=15&y=assembly_span&cat=kingdom&hideSourceColors=true&size=10
