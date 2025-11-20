# Snail Plots

Reproducible examples for the snail plot manuscript.

Panels for Figures 1-4 were generated using the `prepare_figures.py` script

## Figure 1

### Scripted version:

```
./prepare_figure.py -f 1
```

### Equivalent command

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 -o figure1/1.svg
```

## Figure 2

### Scripted version:

```
./prepare_figure.py -f 2
```

### A. Mus musculus GCA_949316315.1 mMusMuc1_1

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 -o figure2/2A.png
```

### B., C., D. Mus Musculus GCA_000185125.1 AEKR01

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 -o figure2/2B.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --max-span 2770968735 -o figure2/2C.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/AEKR01 --max-span 2770968735 --max-scaffold 200127270 -o figure2/2D.png
```

## Figure 3

### Scripted version:

```
./prepare_figure.py -f 3
```

### A., B. Mus musculus GCA_949316315.1 mMusMuc1_1

```
blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --show-numbers -o figure3/3A.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 --scale-function linear --show-numbers -o figure3/3B.png

blobtk plot -v snail -d https://blobtoolkit.genomehubs.org/api/v1/dataset/id/mMusMuc1_1 -o figure3/GCA_949316315.1.yaml
```

## Figure 4

### Scripted version:

```
./prepare_figure.py -f 1
```

### Partially equivalent commands

Genomes are subsampled from a from [goat search of contig vs scaffold n50](https://goat.genomehubs.org/2025.04.21/search?query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000&result=assembly&taxonomy=ncbi&size=10000&report=scatter&y=contig_n50&plotRatio=auto&pointSize=15&xField=scaffold_n50&xOpts=10000%3B10000000000%3B7&yOpts=1000%3B1000000000%3B7)

The full list of results is fetched vai the API

```
curl -H 'accept: text/tab-separated-values' "https://goat.genomehubs.org/api/v2/search?query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000&result=assembly&taxonomy=ncbi&size=10000&report=scatter&y=contig_n50&plotRatio=auto&pointSize=15&xField=scaffold_n50&xOpts=10000%3B10000000000%3B7&yOpts=1000%3B1000000000%3B7" > f2-assembly-data.raw.tsv
```

This output is processed to randomly select representative assemblies for each contig/scaffold bin, using a seed value of 1031 for reproducibility, so remaining steps must be run using the `prepare_figures.py` script.

## Figure 5

A taxonomy tree view of assemblies in Figure 4 was generated using GoaT.

### Get list of assemblies from Figure 4 tsv file

```
cut -f 6 figure4/figure4_snail_badge_table.tsv
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
```

### Use to build a GoaT URL to view the tree

Resulting [GoaT URL](https://goat.genomehubs.org/search?query=assembly_id%3DGCA_900322205.1%2CGCA_003016195.1%2CGCA_001632505.1%2CGCA_000261425.2%2CGCA_002222395.1%2CGCA_020883555.1%2CGCA_001661245.1%2CGCA_000204055.1%2CGCA_964340765.1%2CGCA_013467465.1%2CGCA_014337955.1%2CGCA_018257905.1%2CGCA_964340405.1%2CGCA_000001215.4%2CGCA_003033685.1%2CGCA_013339765.2%2CGCA_963691655.1%2CGCA_949316315.1%2CGCA_019009955.1%2CGCA_964205295.1%2CGCA_963693085.1&result=assembly&includeEstimates=true&taxonomy=ncbi&report=tree&collapseMonotypic=true&treeStyle=ring&treeThreshold=2000&pointSize=15&y=assembly_span&cat=kingdom&hideSourceColors=true&size=10)

## Figure 6

Is a version of Figure 4 manually edited to include snail score and taxonomy information.
