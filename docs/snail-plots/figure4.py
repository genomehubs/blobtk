#!/usr/bin/env python3

import math
import os
import random
import re
import subprocess

import requests
import yaml


def order_of_magnitude(number):
    return math.floor(math.log(number, 10))


def fetch_goat_data():
    query_url = (
        "https://goat.genomehubs.org/api/2025.04.21/search?"
        "query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000"
        "&result=assembly"
        "&taxonomy=ncbi"
        "&size=10000"
        # "&report=scatter"
        # "&y=contig_n50"
        # "&plotRatio=auto"
        # "&pointSize=15"
        # "&xField=scaffold_n50"
        # "&xOpts=10000%3B10000000000%3B7"
        # "&yOpts=1000%3B1000000000%3B7"
    )

    # fetch query_url with accept header tsv. use python module requests
    headers = {"Accept": "text/tab-separated-values"}
    response = requests.get(query_url, headers=headers, timeout=60)
    if response.status_code != 200:
        raise RuntimeError(
            f"Error fetching GoaT data: {response.status_code} {response.text}"
        )

    # Parse the TSV response
    data = [[[] for _ in range(6)] for _ in range(6)]
    header = None
    scaffold_n50_index = None
    contig_n50_index = None
    assembly_id_index = None
    for line in response.text.strip().split("\n"):
        if header is None:
            header = line.split("\t")
            scaffold_n50_index = header.index("scaffold_n50")
            contig_n50_index = header.index("contig_n50")
            assembly_id_index = header.index("assembly_id")
            continue
        fields = line.split("\t")
        scaffold_n50 = int(fields[scaffold_n50_index])
        contig_n50 = int(fields[contig_n50_index])
        assembly_id = fields[assembly_id_index]
        x_bin = order_of_magnitude(scaffold_n50) - 4
        y_bin = order_of_magnitude(contig_n50) - 3
        data[x_bin][y_bin].append(
            {
                "assembly_id": assembly_id,
                "scaffold_n50": scaffold_n50,
                "contig_n50": contig_n50,
                "fields": dict(zip(header, fields)),
            }
        )
    return data


def find_blobtoolkit_id(assembly_id):
    query_url = f"https://blobtoolkit.genomehubs.org/api/v1/search/{assembly_id}"
    response = requests.get(query_url, timeout=60)
    if response.status_code != 200:
        raise RuntimeError(
            f"Error fetching BlobToolKit data: {response.status_code} {response.text}"
        )
    results = response.json()
    return next(
        (
            result.get("id")
            for result in results
            if result.get("latest") == result.get("revision")
        ),
        None,
    )


def select_representative_assemblies(data, seed=42):
    representatives = []
    random.seed(seed)
    for x_index, x_bin in enumerate(data):
        if not x_bin:
            continue
        for y_index, y_bin in enumerate(x_bin):
            if not y_bin:
                continue
            # select assembly at random using seed
            representative = random.choice(y_bin)
            representatives.append(
                {
                    **representative,
                    "x_bin": x_index,
                    "y_bin": y_index,
                    "blobtoolkit_id": find_blobtoolkit_id(
                        representative["assembly_id"]
                    ),
                }
            )
    return representatives


def draw_badges(representatives, directory="figure4"):
    os.makedirs(directory, exist_ok=True)
    for rep in representatives:
        if rep["blobtoolkit_id"] is None:
            print(f"Warning: No BlobToolKit ID found for assembly {rep['assembly_id']}")
            continue
        print(
            f"Drawing snail badge for assembly {rep['assembly_id']} (BlobToolKit ID: {rep['blobtoolkit_id']})"
        )
        cmd = [
            "blobtk",
            "plot",
            "-v",
            "snail",
            "-d",
            f"https://blobtoolkit.genomehubs.org/api/v1/dataset/id/{rep['blobtoolkit_id']}",
            "--badge",
            "--scale-function",
            "linear",
            "-o",
            f"{directory}/{rep['assembly_id']}_snail_badge.svg",
        ]
        subprocess.run(cmd)
        cmd[-1] = f"{directory}/{rep['assembly_id']}_snail_badge.yaml"
        subprocess.run(cmd)


def replace_svg_tag_with_group(svg_content, translate_x, translate_y):
    # Replace the entire <svg ...> opening tag with <g transform="...">
    return re.sub(
        r"<svg[^>]*?>",
        f'<g transform="translate({translate_x},{translate_y})">',
        svg_content,
        count=1,
    ).replace("</svg>", "</g>")


def make_badge_grid(representatives, directory="figure4"):
    os.makedirs(directory, exist_ok=True)
    grid_svg_path = f"{directory}/figure4_snail_badge_grid.svg"
    print(f"Making badge grid at {grid_svg_path}")
    with open("figure4_blank_grid_no_letters.svg", "r") as f:
        blank_grid_svg = f.read()
    badge_groups = [replace_svg_tag_with_group(blank_grid_svg, 0, 0)]
    table_rows = []
    for rep in representatives:
        badge_path = f"{directory}/{rep['assembly_id']}_snail_badge.svg"
        yaml_path = f"{directory}/{rep['assembly_id']}_snail_badge.yaml"
        if os.path.exists(badge_path):
            # read badge file and replace svg wrapper with group
            # translate according to bin position
            translate_x = rep["x_bin"] * 1000
            translate_y = (5 - rep["y_bin"]) * 1000 - 25
            with open(badge_path, "r") as f:
                badge_svg = f.read()
            badge_svg = replace_svg_tag_with_group(badge_svg, translate_x, translate_y)
            badge_groups.append(badge_svg)
            # read yaml file to get fields for table
            with open(yaml_path, "r") as f:
                yaml_content = yaml.safe_load(f)
            auN = yaml_content.get("auN")
            longest_scaffold = yaml_content.get("scaffolds")[0]
            n_proportion = yaml_content.get("n_proportion", 0)
            scaffold_n90 = yaml_content.get("binned_scaffold_lengths")[900]
            rauN = yaml_content.get("rauN")
            auNn = yaml_content.get("auNn")
            rauNn = yaml_content.get("rauNn")

            # table_rows.append(
            #     {
            #         "x_bin": rep["x_bin"],
            #         "y_bin": rep["y_bin"],
            #         **rep["fields"],
            #         "auN": auN,
            #         "longest_scaffold": longest_scaffold,
            #         "n_proportion": n_proportion,
            #         "rauN": rauN,
            #         "auNn": auNn,
            #         "rauNn": rauNn,
            #         "blobtoolkit_id": rep["blobtoolkit_id"],
            #     }
            # )
            table_rows.append(
                {
                    "accession": rep["fields"].get("assembly_id"),
                    "scientific name": rep["fields"].get("scientific_name"),
                    "assembly level": rep["fields"].get("assembly_level"),
                    "assembly span (Mbp)": int(rep["fields"].get("assembly_span", 0))
                    / 1_000_000,
                    "contig N50 (Mbp)": int(rep["fields"].get("contig_n50", 0))
                    / 1_000_000,
                    "scaffold N50 (Mbp)": int(rep["fields"].get("scaffold_n50", 0))
                    / 1_000_000,
                    "scaffold N90 (Mbp)": int(
                        scaffold_n90 if scaffold_n90 is not None else 0
                    )
                    / 1_000_000,
                    "contig bin": rep["y_bin"] + 3,
                    "scaffold bin": rep["x_bin"] + 4,
                    "longest_scaffold (Mbp)": longest_scaffold / 1_000_000,
                    "n_proportion": n_proportion,
                    "auN (Mbp)": auN / 1_000_000,
                    "relative auN": rauN,
                    "auN without Ns (Mbp)": auNn / 1_000_000,
                    "relative rauN without Ns": rauNn,
                    "BlobToolKit ID": rep["blobtoolkit_id"],
                }
            )

    # Write the badge grid SVG file
    with open(grid_svg_path, "w") as f:
        f.write(
            '<svg xmlns="http://www.w3.org/2000/svg" height="6503.168" width="6615.657"'
            ' viewbox="0 0 6615.657 6503.168" preserveAspectRatio="xMinYMin meet">\n'
        )
        f.write('<g transform="translate(609.988, 5.669)">\n')
        f.write("\n".join(badge_groups))
        f.write("</g>\n")
        f.write("</svg>\n")

    # Write the table of assemblies
    table_path = f"{directory}/figure4_snail_badge_table.tsv"
    with open(table_path, "w") as f:
        # Write header
        if table_rows:
            header = list(table_rows[0].keys())
            f.write("\t".join(header) + "\n")
            for row in table_rows:
                f.write("\t".join(str(row[h]) for h in header) + "\n")


def main():
    data = fetch_goat_data()
    representatives = select_representative_assemblies(data, seed=1031)
    # draw_badges(representatives, "figure4")
    make_badge_grid(representatives, "figure4")


if __name__ == "__main__":
    main()
