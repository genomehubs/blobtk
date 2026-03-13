#!/usr/bin/env python3

import argparse
import csv
import math
import os
import random
import re
import subprocess
import xml.etree.ElementTree as ET

import requests
import yaml


def order_of_magnitude(number):
    return math.floor(math.log(number, 10))


def fetch_goat_data_figure_4():
    query_url = (
        "https://goat.genomehubs.org/api/2025.04.21/search?"
        "query=scaffold_n50%20AND%20scaffold_n50%3E%3D10000%20AND%20assembly_span%20%3E%2010000000%20AND%20assembly_level%3Dscaffold%2Cchromosome%2Ccomplete%20genome%20AND%20busco_completeness%20%3E%2080%20AND%20contig_n50%3E1000"
        "&result=assembly"
        "&taxonomy=ncbi"
        "&size=10000"
        "&ranks=kingdom"
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
    taxid_index = None
    scaffold_n50_index = None
    contig_n50_index = None
    assembly_id_index = None
    kingdom_index = None
    for line in response.text.strip().split("\n"):
        if header is None:
            header = line.split("\t")
            taxid_index = header.index("taxon_id")
            scaffold_n50_index = header.index("scaffold_n50")
            contig_n50_index = header.index("contig_n50")
            assembly_id_index = header.index("assembly_id")
            kingdom_index = header.index("kingdom")
            continue
        fields = line.split("\t")
        taxon_id = fields[taxid_index]
        scaffold_n50 = int(fields[scaffold_n50_index])
        contig_n50 = int(fields[contig_n50_index])
        assembly_id = fields[assembly_id_index]
        kingdom = fields[kingdom_index]
        x_bin = order_of_magnitude(scaffold_n50) - 4
        y_bin = order_of_magnitude(contig_n50) - 3
        data[x_bin][y_bin].append(
            {
                "taxon_id": taxon_id,
                "assembly_id": assembly_id,
                "scaffold_n50": scaffold_n50,
                "contig_n50": contig_n50,
                "kingdom": kingdom,
                "fields": dict(zip(header, fields)),
            }
        )
    return data


def fetch_goat_assembly_data(taxon_id, assembly_span):
    query_url = (
        f"https://goat.genomehubs.org/api/2025.04.21/search?result=assembly"
        f"&query=tax_name%28{taxon_id}%29%20AND%20assembly_span%20%3D%20{assembly_span}"
        f"&taxonomy=ncbi"
        f"&fields=scaffold_n50%2Ccontig_n50%2Cassembly_span"
    )
    # fetch query_url with accept header tsv. use python module requests
    headers = {"Accept": "text/tab-separated-values"}
    response = requests.get(query_url, headers=headers, timeout=60)
    if response.status_code != 200:
        raise RuntimeError(
            f"Error fetching GoaT data: {response.status_code} {response.text}"
        )
    # Parse the TSV response and extract assembly_id, scaffold_n50, contig_n50, assembly_span into a list of dicts
    # return None if no results or more than one result
    data = []
    header = None
    for line in response.text.strip().split("\n"):
        if header is None:
            header = line.split("\t")
            continue
        fields = line.split("\t")
        data.append(dict(zip(header, fields)))
    return None if len(data) != 1 else data[0]


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


def draw_plot(blobdir, directory, filename, options=None):
    os.makedirs(directory, exist_ok=True)
    if options is None:
        options = []
    file_path = f"{directory}/{filename}"
    if os.path.exists(file_path):
        print(f"File {file_path} already exists, skipping plot generation")
        return
    cmd = [
        "blobtk",
        "snail",
        "-d",
        blobdir,
        "-o",
        file_path,
    ]
    if options is not None:
        cmd.extend(options)
    print(cmd)
    subprocess.run(cmd)


def draw_badges(representatives, directory="figure4"):
    for rep in representatives:
        if rep["blobtoolkit_id"] is None:
            print(f"Warning: No BlobToolKit ID found for assembly {rep['assembly_id']}")
            continue
        print(
            f"Drawing snail badge for assembly {rep['assembly_id']} (BlobToolKit ID: {rep['blobtoolkit_id']})"
        )
        blobdir = f"https://blobtoolkit.genomehubs.org/api/v1/dataset/id/{rep['blobtoolkit_id']}"
        filename = f"{rep['assembly_id']}_snail_badge.svg"
        options = [
            "--badge",
            "-o",
            f"{directory}/{rep['assembly_id']}_snail_badge.yaml",
        ]
        draw_plot(
            blobdir,
            directory,
            filename,
            options,
        )


def replace_svg_tag_with_group(svg_content, translate_x, translate_y):
    # Replace the entire <svg ...> opening tag with <g transform="...">
    return re.sub(
        r"<svg[^>]*?>",
        f'<g transform="translate({translate_x},{translate_y})">',
        svg_content,
        count=1,
    ).replace("</svg>", "</g>")


def create_blank_badge_grid_svg():
    grid_size = 6000
    cell_size = 1000

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{grid_size}" height="{grid_size}">',
        '<path d="M0 0v6000M6000 6000H0" style="fill:none;stroke:#000;stroke-width:12;stroke-linecap:round"/>',
    ]

    for i in range(1, 6):
        pos = i * cell_size
        lines.append(
            f'<path d="M0 {pos}h6000M{pos} 0v6000" '
            'style="fill:none;stroke:gray;stroke-width:4;stroke-linecap:round"/>'
        )

    for i, exp in enumerate(range(4, 10)):
        x = i * cell_size + 500
        lines.append(
            f'<text x="{x}" y="6250" text-anchor="middle" '
            'style="font-size:200px;font-family:sans-serif;fill:#000">10<tspan '
            'style="font-size:133px" dy="-80">'
            f"{exp}</tspan></text>"
        )

    for i, exp in enumerate(range(3, 9)):
        y = (5 - i) * cell_size + 550
        lines.append(
            f'<text x="-350" y="{y}" text-anchor="start" '
            'style="font-size:200px;font-family:sans-serif;fill:#000">10<tspan '
            'style="font-size:133px" dy="-80">'
            f"{exp}</tspan></text>"
        )

    lines.extend(
        (
            '<text x="3000" y="6450" text-anchor="middle" '
            'style="font-size:192px;font-family:sans-serif;fill:#000">Scaffold N50</text>',
            '<text x="-3000" y="-400" transform="rotate(-90)" text-anchor="middle" '
            'style="font-size:192px;font-family:sans-serif;fill:#000">Contig N50</text>',
            "</svg>",
        )
    )
    return "\n".join(lines)


def colour_by_kingdom(kingdom):
    kingdom_colors = {
        "Metazoa": "#440154",
        "Viridiplantae": "#35b779",
        "Fungi": "#31688e",
        "Other": "#999999",
    }
    return kingdom_colors.get(kingdom, kingdom_colors["Other"])


def load_representatives_from_badge_table(table_path):
    representatives = []
    with open(table_path, "r") as f:
        reader = csv.DictReader(f, delimiter="\t")
        for row in reader:
            assembly_id = row.get("accession")
            if not assembly_id:
                continue
            scaffold_bin = int(float(row.get("scaffold bin", "4")))
            contig_bin = int(float(row.get("contig bin", "3")))
            representatives.append(
                {
                    "assembly_id": assembly_id,
                    "x_bin": scaffold_bin - 4,
                    "y_bin": contig_bin - 3,
                    "blobtoolkit_id": row.get("BlobToolKit ID"),
                    "fields": {
                        "assembly_id": assembly_id,
                        "scientific_name": row.get("scientific name", ""),
                        "kingdom": row.get("kingdom", "Other"),
                        "assembly_level": row.get("assembly level", ""),
                        "assembly_span": str(
                            int(float(row.get("assembly span (Mbp)", "0")) * 1_000_000)
                        ),
                        "contig_n50": str(
                            int(float(row.get("contig N50 (Mbp)", "0")) * 1_000_000)
                        ),
                        "scaffold_n50": str(
                            int(float(row.get("scaffold N50 (Mbp)", "0")) * 1_000_000)
                        ),
                    },
                }
            )
    return representatives


def figure6_cell_overlay(rep, rauNn):
    stroke_width = 0
    fill_opacity = 0.3
    side = 1000
    grid_width = 12
    height = side - stroke_width - grid_width
    width = side - stroke_width - grid_width
    translate_x = rep["x_bin"] * 1000 + stroke_width / 2 + grid_width / 2
    translate_y = (5 - rep["y_bin"]) * 1000 + stroke_width / 2 + grid_width / 2
    stroke_color = colour_by_kingdom(rep["fields"].get("kingdom"))
    score_text = "NA" if rauNn is None else f"{rauNn:.2f}"
    return (
        f'<g transform="translate({translate_x},{translate_y})">'
        f'<rect x="0" y="0" width="{width}" height="{height}" '
        f'style="fill:#ffffff;fill-opacity:{fill_opacity * 2.5};stroke:none"/>'
        f'<rect x="40" y="{height - 86}" width="{width - 80}" height="80" rx="40" ry="40" '
        f'style="fill:{stroke_color};stroke:none;"/>'
        '<text x="500" y="555" text-anchor="middle" '
        f'style="font-size:220px;font-family:Arial,sans-serif;font-weight:normal;fill:#000;">'
        f"{score_text}</text>"
        "</g>"
    )


def make_badge_grid(
    representatives,
    directory="figure4",
    figure_name="figure4",
    badge_directory=None,
    dpi=150,
):
    os.makedirs(directory, exist_ok=True)
    if badge_directory is None:
        badge_directory = directory
    grid_svg_path = f"{directory}/{figure_name}_snail_badge_grid.svg"
    grid_png_path = f"{directory}/{figure_name}_snail_badge_grid.png"
    table_path = f"{directory}/{figure_name}_snail_badge_table.tsv"
    add_figure6_overlay = figure_name == "figure6"
    print(f"Making badge grid at {grid_svg_path}")
    blank_grid_svg = create_blank_badge_grid_svg()
    badge_groups = [replace_svg_tag_with_group(blank_grid_svg, 0, 0)]
    table_rows = []
    for rep in representatives:
        badge_path = f"{badge_directory}/{rep['assembly_id']}_snail_badge.svg"
        yaml_path = f"{badge_directory}/{rep['assembly_id']}_snail_badge.yaml"
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

            if add_figure6_overlay:
                badge_groups.append(figure6_cell_overlay(rep, rauNn))

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
                    "kingdom": rep["fields"].get("kingdom"),
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

    convert_svg_to_png(
        svg_path=grid_svg_path,
        png_path=grid_png_path,
        px_width=round(6615.657 * dpi / 96),
        px_height=round(6503.168 * dpi / 96),
    )

    # Write the table of assemblies
    with open(table_path, "w") as f:
        # Write header
        if table_rows:
            header = list(table_rows[0].keys())
            f.write("\t".join(header) + "\n")
            for row in table_rows:
                f.write("\t".join(str(row[h]) for h in header) + "\n")

    return grid_svg_path, grid_png_path, table_path


def _load_svg_children_and_viewbox(svg_path):
    tree = ET.parse(svg_path)
    root = tree.getroot()
    view_box = root.get("viewBox") or root.get("viewbox")
    children = [ET.tostring(child, encoding="unicode") for child in root]
    return view_box, children


def _viewbox_dims(viewbox, default_width, default_height):
    if viewbox is None:
        return default_width, default_height
    parts = viewbox.split()
    if len(parts) != 4:
        return default_width, default_height
    try:
        return float(parts[2]), float(parts[3])
    except ValueError:
        return default_width, default_height


def make_figure4_panel_b(
    directory,
    badge_directory,
    accession="GCA_003033685.1",
    source_box=(300, 65, 400, 200),
):
    badge_path = f"{badge_directory}/{accession}_snail_badge.svg"
    if not os.path.exists(badge_path):
        raise FileNotFoundError(
            f"Missing badge for panel B: {badge_path}. Generate badges first."
        )

    _, badge_children = _load_svg_children_and_viewbox(badge_path)
    panel_b_svg_path = f"{directory}/figure4_panelB.svg"

    panel_width = 6615.657
    panel_height = 1550
    small_x = 150
    small_y = 320
    small_size = 900
    zoom_x = 1600
    zoom_y = 220
    zoom_w = 3000
    zoom_h = 1000
    source_box_stroke = 10
    connector_stroke = 8
    zoom_frame_stroke = 10
    zoom_aspect = "xMidYMid meet"

    source_x, source_y, source_w, source_h = source_box
    source_rect_x = small_x + source_x / 1000 * small_size
    source_rect_y = small_y + source_y / 1000 * small_size
    source_rect_w = source_w / 1000 * small_size
    source_rect_h = source_h / 1000 * small_size

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{panel_width}" height="{panel_height}" viewBox="0 0 {panel_width} {panel_height}">',
        '<rect x="0" y="0" width="100%" height="100%" fill="white"/>',
        f'<svg x="{small_x}" y="{small_y}" width="{small_size}" height="{small_size}" viewBox="0 0 1000 1000">',
        *badge_children,
        "</svg>",
        f'<rect x="{source_rect_x}" y="{source_rect_y}" width="{source_rect_w}" height="{source_rect_h}" fill="none" stroke="#333" stroke-width="{source_box_stroke}"/>',
        f'<line x1="{source_rect_x + source_rect_w}" y1="{source_rect_y}" x2="{zoom_x}" y2="{zoom_y}" stroke="#333" stroke-width="{connector_stroke}"/>',
        f'<line x1="{source_rect_x + source_rect_w}" y1="{source_rect_y + source_rect_h}" x2="{zoom_x}" y2="{zoom_y + zoom_h}" stroke="#333" stroke-width="{connector_stroke}"/>',
        f'<rect x="{zoom_x}" y="{zoom_y}" width="{zoom_w}" height="{zoom_h}" fill="white" stroke="#333" stroke-width="{zoom_frame_stroke}"/>',
        f'<svg x="{zoom_x}" y="{zoom_y}" width="{zoom_w}" height="{zoom_h}" viewBox="{source_x} {source_y} {source_w} {source_h}" preserveAspectRatio="{zoom_aspect}">',
        *badge_children,
        "</svg>",
        "</svg>",
    ]

    with open(panel_b_svg_path, "w") as f:
        f.write("\n".join(lines) + "\n")

    return panel_b_svg_path


def make_figure4_full(directory, grid_svg_path, panel_b_svg_path, dpi=150):
    panel_a_viewbox, panel_a_children = _load_svg_children_and_viewbox(grid_svg_path)
    panel_b_viewbox, panel_b_children = _load_svg_children_and_viewbox(panel_b_svg_path)

    panel_a_width, panel_a_height = _viewbox_dims(panel_a_viewbox, 6615.657, 6503.168)
    _, panel_b_height = _viewbox_dims(panel_b_viewbox, 6615.657, 1550)

    canvas_width = 6900
    left_margin = 250
    top_margin = 40
    label_height = 120
    label_size = 360
    label_offset_x = 120
    label_font = "Arial, sans-serif"
    inter_panel_gap = 10
    panel_a_y = top_margin + label_height
    panel_b_y = panel_a_y + panel_a_height + inter_panel_gap + label_height / 2
    canvas_height = panel_b_y + panel_b_height - label_height

    figure4_svg_path = f"{directory}/figure4.svg"
    figure4_png_path = f"{directory}/figure4.png"

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{canvas_width}" height="{canvas_height}" viewBox="0 0 {canvas_width} {canvas_height}">',
        '<rect x="0" y="0" width="100%" height="100%" fill="white"/>',
        f'<text x="{label_offset_x}" y="{top_margin + label_size}" font-family="{label_font}" font-size="{label_size}" font-weight="normal" fill="black">A</text>',
        f'<svg x="{left_margin}" y="{panel_a_y}" width="{panel_a_width}" height="{panel_a_height}" viewBox="{panel_a_viewbox or f"0 0 {panel_a_width} {panel_a_height}"}">',
        *panel_a_children,
        "</svg>",
        f'<text x="{label_offset_x}" y="{panel_a_y + panel_a_height + inter_panel_gap + label_size}" font-family="{label_font}" font-size="{label_size}" font-weight="normal" fill="black">B</text>',
        f'<svg x="{left_margin + 500}" y="{panel_b_y}" width="{panel_a_width}" height="{panel_b_height}" viewBox="{panel_b_viewbox or f"0 0 {panel_a_width} {panel_b_height}"}">',
        *panel_b_children,
        "</svg>",
        "</svg>",
    ]

    with open(figure4_svg_path, "w") as f:
        f.write("\n".join(lines) + "\n")

    convert_svg_to_png(
        svg_path=figure4_svg_path,
        png_path=figure4_png_path,
        px_width=round(canvas_width * dpi / 96),
        px_height=round(canvas_height * dpi / 96),
    )


def extract_features(infile, outfile, include=None, exclude=None, viewbox=None):
    import copy

    # Parse the SVG file as XML
    try:
        tree = ET.parse(infile)
        root = tree.getroot()
    except ET.ParseError:
        raise ET.ParseError(f"Error parsing SVG file: {infile}")

    ET.register_namespace("", "http://www.w3.org/2000/svg")
    svg_ns = "http://www.w3.org/2000/svg"
    group_tag = f"{{{svg_ns}}}g"

    # Extract viewBox if not provided
    if viewbox is None:
        viewbox = root.get("viewBox")
        if not viewbox:
            raise ValueError(
                "ViewBox not found in SVG file and not provided as argument"
            )

    def filter_children(element, in_scope):
        """
        Recursively filter children of element and return a list of kept child elements.

        in_scope: True means we are inside an already-accepted group, so all children
                  are kept by default (unless excluded).

        Rules:
        - If a child's id is in exclude: drop it, but if include is set still recurse
          into it to surface any included descendants at this level.
        - If in_scope, or the child's id is in include, or include is None: keep the
          child (as a filtered copy whose own children obey the same rules).
        - Otherwise (include is set, not in scope, id not listed): traverse into the
          child without keeping the child itself, looking for included descendants.
        """
        result = []
        for child in element:
            if child.tag != group_tag:
                # Non-group nodes: keep when in scope (or no include filter)
                if in_scope or include is None:
                    result.append(copy.deepcopy(child))
                continue

            child_id = child.get("id")
            in_inc = include is not None and child_id in include
            in_exc = exclude is not None and child_id in exclude

            if in_exc and not in_inc:
                # Excluded group – drop it, but surface any included descendants
                if include is not None:
                    result.extend(filter_children(child, False))
            elif in_scope or in_inc or include is None:
                # Keep this group – build a filtered copy
                new_child = copy.copy(child)
                new_child[:] = filter_children(child, True)
                result.append(new_child)
            else:
                # Not in scope and not listed – traverse looking for includes
                result.extend(filter_children(child, False))

        return result

    # Build the top-level output
    if include is None:
        # No include filter: output a filtered copy of the root SVG element
        new_root = copy.copy(root)
        new_root[:] = filter_children(root, True)
        top_groups = [ET.tostring(new_root, encoding="unicode")]
    else:
        # Include filter: collect matching groups (possibly from deep in the tree)
        top_groups = [
            ET.tostring(elem, encoding="unicode")
            for elem in filter_children(root, False)
        ]

    # Write new SVG file with included groups and viewbox
    with open(outfile, "w") as f:
        f.write(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{viewbox}">\n')
        f.write("\n".join(top_groups))
        f.write("\n</svg>\n")


def convert_svg_to_png(svg_path, png_path, px_width, px_height):
    converters = [
        (
            "rsvg-convert",
            lambda: subprocess.run(
                [
                    "rsvg-convert",
                    "-w",
                    str(px_width),
                    "-h",
                    str(px_height),
                    "-o",
                    png_path,
                    svg_path,
                ],
                check=True,
            ),
        ),
        (
            "sips",
            lambda: subprocess.run(
                [
                    "sips",
                    "-s",
                    "format",
                    "png",
                    "-Z",
                    str(max(px_width, px_height)),
                    svg_path,
                    "--out",
                    png_path,
                ],
                check=True,
            ),
        ),
    ]

    for converter_name, converter in converters:
        try:
            converter()
            print(f"Wrote {png_path} using {converter_name}")
            return
        except Exception:
            continue

    raise RuntimeError(
        f"Failed to convert SVG to PNG for {svg_path}. Install rsvg-convert or use macOS sips."
    )


def assemble_figure(
    panels,
    outfile,
    cols=2,
    panel_width=500,
    panel_height=500,
    padding=0,
    label_height=36,
    label_size=36,
    label_font="Arial, sans-serif",
    label_offset_x=8,
    dpi=300,
):
    """
    Assemble multiple SVG panel files into a single grid figure.

    Parameters
    ----------
    panels       : list of dicts, each with:
                     'file'    - path to an SVG panel file (required)
                     'viewbox' - override the source viewBox (optional)
    outfile      : output SVG path
    cols         : number of columns in the grid
    panel_width  : width of each panel cell (output SVG units)
    panel_height : total height of each panel cell including label area
    padding      : space between cells and around the figure edge
    label_height : pixels reserved above the content area for the panel label
    label_size   : font size for panel labels
    label_font   : font-family for panel labels
    label_offset_x : horizontal offset of label from cell left edge
    dpi            : target resolution for PNG output. SVG units are treated as
                     96 dpi, so the output is scaled by dpi/96 (e.g. 300 dpi
                     gives a 3.125x linear scale).

    Each panel SVG is embedded as a nested <svg> with its original viewBox and
    preserveAspectRatio="xMidYMid meet" so content is automatically centred and
    scaled to fill the content area regardless of its natural dimensions.
    """
    ET.register_namespace("", "http://www.w3.org/2000/svg")

    rows = math.ceil(len(panels) / cols)
    total_width = cols * panel_width + (cols - 1) * padding
    total_height = rows * panel_height + (rows - 1) * padding
    content_height = panel_height - label_height
    # Pixel dimensions for PNG output (SVG units are 96 dpi by convention)
    px_width = round(total_width * dpi / 96)
    px_height = round(total_height * dpi / 96)

    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg"'
        f' width="{total_width}" height="{total_height}"'
        f' viewBox="0 0 {total_width} {total_height}">'
    ]

    for i, panel in enumerate(panels):
        label = chr(ord("A") + i)
        row = i // cols
        col = i % cols
        x = col * (panel_width + padding)
        y = row * (panel_height + padding)

        # Resolve the viewBox – read from source file if not overridden
        vb = panel.get("viewbox")
        if vb is None:
            try:
                src_tree = ET.parse(panel["file"])
                vb = src_tree.getroot().get(
                    "viewBox", f"0 0 {panel_width} {content_height}"
                )
            except Exception:
                vb = f"0 0 {panel_width} {content_height}"

        lines.append(f'  <g transform="translate({x},{y})">')

        # Nested SVG occupies the content area below the label
        lines.append(
            f"    <svg"
            f' x="0" y="{label_height}"'
            f' width="{panel_width}" height="{content_height}"'
            f' viewBox="{vb}"'
            f' preserveAspectRatio="xMidYMid meet">'
        )
        try:
            src_tree = ET.parse(panel["file"])
            lines.extend(
                "      " + ET.tostring(child, encoding="unicode")
                for child in src_tree.getroot()
            )
        except Exception as e:
            print(f"Warning: could not read {panel['file']}: {e}")
        lines.append("    </svg>")

        lines.extend(
            (
                f'    <text x="{label_offset_x}" y="{label_size * 0.85}" font-family="{label_font}" font-size="{label_size}" font-weight="normal" fill="black">{label}</text>',
                "  </g>",
            )
        )
    lines.append("</svg>")

    svg_content = "\n".join(lines) + "\n"
    os.makedirs(os.path.dirname(os.path.abspath(outfile)), exist_ok=True)

    if outfile.endswith(".svg"):
        with open(outfile, "w") as f:
            f.write(svg_content)
        print(f"Wrote {outfile}")
    elif outfile.endswith(".png"):
        svg_path = f"{outfile[:-4]}.svg"
        with open(svg_path, "w") as f:
            f.write(svg_content)

        convert_svg_to_png(svg_path, outfile, px_width, px_height)


def save_figure7_data(directory, filename):
    source_files = [
        f"{directory}/7A.yaml",
        f"{directory}/7B.yaml",
        f"{directory}/7C.yaml",
    ]

    def _extract_assembly_id(path_or_url):
        if not path_or_url:
            return None
        match = re.search(r"(GC[AF]_\d+\.\d+)", str(path_or_url))
        if match:
            return match[1]
        base = os.path.basename(str(path_or_url)).replace(".gz", "")
        return os.path.splitext(base)[0] if base else None

    def _extract_data_source_id(parameters, key, fallback=None):
        source = (parameters or {}).get(key) or {}
        candidate = source.get("blobdir") or source.get("fasta")
        return _extract_assembly_id(candidate) or fallback

    def _extract_snail_scores(data):
        snail = data.get("snail")
        if isinstance(snail, dict):
            return {
                "score": snail.get("score", snail.get("base", data.get("rauNn"))),
                "reference": snail.get("reference"),
                "g": snail.get("g", data.get("snail_g", data.get("rauNn-g"))),
                "gs": snail.get("gs", data.get("snail_gs", data.get("rauNn-gs"))),
                "ag": snail.get("ag", data.get("snail_ag", data.get("rauNn-ag"))),
                "ags": snail.get("ags", data.get("snail_ags", data.get("rauNn-ags"))),
            }
        return {
            "score": data.get("rauNn", snail),
            "reference": None,
            "g": data.get("snail_g", data.get("rauNn-g")),
            "gs": data.get("snail_gs", data.get("rauNn-gs")),
            "ag": data.get("snail_ag", data.get("rauNn-ag")),
            "ags": data.get("snail_ags", data.get("rauNn-ags")),
        }

    rows = []
    for source_file in source_files:
        if not os.path.exists(source_file):
            continue
        with open(source_file, "r") as f:
            data = yaml.safe_load(f)
        parameters = data.get("parameters") or {}
        scores = _extract_snail_scores(data)

        scaffolds = data.get("scaffolds") or []
        longest_scaffold = scaffolds[0] if scaffolds else None

        row = {
            "asm_id": _extract_data_source_id(parameters, "source", data.get("id")),
            "asm_span": data.get("assembly"),
            "asm_long": longest_scaffold,
            "ref_id": _extract_data_source_id(parameters, "reference"),
            "ref_span": parameters.get("max_span"),
            "ref_long": parameters.get("max_scaffold"),
            "snail_ref": scores.get("reference"),
            "snail": scores.get("score"),
            "snail_g": scores.get("g"),
            "snail_gs": scores.get("gs"),
            "snail_ag": scores.get("ag"),
            "snail_ags": scores.get("ags"),
        }
        rows.append(row)

    if rows:
        table_path = f"{directory}/{filename}"
        with open(table_path, "w") as f:
            writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()), delimiter="\t")
            writer.writeheader()
            writer.writerows(rows)
        print(f"Wrote {table_path}")


def figure1(directory: str):
    if directory is None:
        directory = "figure1"
    print("Drawing snail plot for Figure 1")
    blobdir = "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_949316315.1"
    filename = "1.svg"
    draw_plot(
        blobdir,
        directory,
        filename,
    )
    draw_plot(
        "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_949316315.1",
        "figure7",
        "7A.svg",
        [
            "--show-numbers",
            "--assembly-name",
            "GCA_949316315.1",
            "--reference",
            "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/964/188/535/GCA_964188535.1_C57BL_6J_T2T_v1/GCA_964188535.1_C57BL_6J_T2T_v1_genomic.fna.gz",
            "--reference-name",
            "GCA_964188535.1 T2T",
            "--show-score",
            "--score-type",
            "g",
            "--output",
            "7A.yaml",
        ],
    )
    viewbox = "46 56 907 937"
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1A.svg",
        viewbox=viewbox,
        include=["plot_translate_group", "angle_axis_group"],
        exclude=["plot_features_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1B.svg",
        viewbox=viewbox,
        include=["plot_translate_group", "angle_axis_group", "gc_composition_group"],
        exclude=["plot_features_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1C.svg",
        viewbox=viewbox,
        include=["plot_translate_group", "angle_axis_group", "scaffold_count_group"],
        exclude=["plot_features_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1D.svg",
        viewbox=viewbox,
        include=[
            "plot_translate_group",
            "angle_axis_group",
            "length_axis_group",
            "scaffold_length_group",
        ],
        exclude=["plot_features_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1E.svg",
        viewbox=viewbox,
        include=[
            "plot_translate_group",
            "angle_axis_group",
            "length_axis_group",
            "scaffold_overlay_group",
        ],
        exclude=["plot_features_group", "n50_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1F.svg",
        viewbox=viewbox,
        include=[
            "plot_translate_group",
            "angle_axis_group",
            "length_axis_group",
            "n50_group",
        ],
        exclude=["plot_features_group", "n90_group"],
    )
    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1G.svg",
        viewbox=viewbox,
        include=[
            "plot_translate_group",
            "angle_axis_group",
            "length_axis_group",
            "n90_group",
        ],
        exclude=["plot_features_group"],
    )

    extract_features(
        "figure7/7A.svg",
        f"{directory}/1H.svg",
        viewbox=viewbox,
        include=[
            "plot_translate_group",
            "angle_axis_group",
            "length_axis_group",
            "ref_length_fill_group",
            "ref_length_outline_group",
        ],
        exclude=["plot_features_group"],
    )

    extract_features(
        f"{directory}/{filename}",
        f"{directory}/1I.svg",
        viewbox="835 93.5 150 153",
        include=[
            "busco_plot_group",
        ],
        exclude=[],
    )

    assemble_figure(
        cols=3,
        label_height=12,
        label_size=54,
        padding=20,
        panels=[
            {"file": f"{directory}/1A.svg"},
            {"file": f"{directory}/1B.svg"},
            {"file": f"{directory}/1C.svg"},
            {"file": f"{directory}/1D.svg"},
            {"file": f"{directory}/1E.svg"},
            {"file": f"{directory}/1F.svg"},
            {"file": f"{directory}/1G.svg"},
            {"file": f"{directory}/1H.svg"},
            {"file": f"{directory}/1I.svg", "viewbox": "785 43.5 250 253"},
        ],
        outfile=f"{directory}/figure1.png",
    )


def figure2(directory: str):
    if directory is None:
        directory = "figure2"
    print("Drawing snail plots for Figure 2")
    blobdir = "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_949316315.1"
    draw_plot(
        blobdir,
        directory,
        "2A.svg",
    )

    blobdir = "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_000185125.1"
    draw_plot(
        blobdir,
        directory,
        "2B.svg",
    )

    viewbox = "0 0 1000 1000"
    assemble_figure(
        cols=2,
        label_height=36,
        label_size=36,
        panel_height=536,
        label_offset_x=0,
        panels=[
            {"file": f"{directory}/2A.svg", "viewbox": viewbox},
            {"file": f"{directory}/2B.svg", "viewbox": viewbox},
        ],
        outfile=f"{directory}/figure2.png",
    )

    # filename = "2C.png"
    # options = ["--max-span", "2770968735"]
    # draw_plot(blobdir, directory, filename, options)

    # filename = "2D.png"
    # options = ["--max-span", "2770968735", "--max-scaffold", "200127270"]
    # draw_plot(blobdir, directory, filename, options)


def figure3(directory: str):
    if directory is None:
        directory = "figure3"
    print("Drawing snail plots for Figure 3")
    blobdir = "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_000185125.1"
    draw_plot(
        blobdir,
        directory,
        "3A.svg",
        ["--show-numbers", "--scale-function", "sqrt"],
    )
    draw_plot(
        blobdir,
        directory,
        "3B.svg",
        ["--show-numbers", "--scale-function", "sqrt", "--max-span", "2770968735"],
    )
    draw_plot(
        blobdir,
        directory,
        "3C.svg",
        [
            "--show-numbers",
            "--scale-function",
            "sqrt",
            "--max-span",
            "2770968735",
            "--max-scaffold",
            "200127270",
        ],
    )
    viewbox = "0 0 1000 1000"
    assemble_figure(
        cols=3,
        label_height=54,
        label_size=54,
        panel_height=554,
        label_offset_x=0,
        panels=[
            {"file": f"{directory}/3A.svg", "viewbox": viewbox},
            {"file": f"{directory}/3B.svg", "viewbox": viewbox},
            {"file": f"{directory}/3C.svg", "viewbox": viewbox},
        ],
        outfile=f"{directory}/figure3.png",
    )


def figure4(directory: str):
    if directory is None:
        directory = "figure4"
    data = fetch_goat_data_figure_4()
    representatives = select_representative_assemblies(data, seed=1031)
    draw_badges(representatives, directory)
    grid_svg_path, _, _ = make_badge_grid(
        representatives,
        directory,
        figure_name="figure4",
        dpi=150,
    )
    panel_b_svg_path = make_figure4_panel_b(directory, directory)
    make_figure4_full(directory, grid_svg_path, panel_b_svg_path, dpi=150)


def figure5():
    goat_url = "https://goat.genomehubs.org/search?query=assembly_id%3DGCA_900322205.1%2CGCA_003016195.1%2CGCA_001632505.1%2CGCA_000261425.2%2CGCA_002222395.1%2CGCA_020883555.1%2CGCA_001661245.1%2CGCA_000204055.1%2CGCA_964340765.1%2CGCA_013467465.1%2CGCA_014337955.1%2CGCA_018257905.1%2CGCA_964340405.1%2CGCA_000001215.4%2CGCA_003033685.1%2CGCA_013339765.2%2CGCA_963691655.1%2CGCA_949316315.1%2CGCA_019009955.1%2CGCA_964205295.1%2CGCA_963693085.1&result=assembly&includeEstimates=true&taxonomy=ncbi&report=tree&collapseMonotypic=true&treeStyle=ring&treeThreshold=2000&pointSize=15&y=assembly_span&cat=kingdom&hideSourceColors=true&size=10"
    print("Figure 5 is based on the tree report at the following GoaT URL:")
    print(goat_url)
    print(
        "The final figure was generated by exporting the tree from the GoaT web interface and annotating it in Inkscape to ensure all GCA accessions were visible."
    )


def figure6(directory: str):
    if directory is None:
        directory = "figure6"
    source_directory = "figure4"
    source_table = f"{source_directory}/figure4_snail_badge_table.tsv"

    if os.path.exists(source_table):
        print(f"Loading representatives from cached table: {source_table}")
        representatives = load_representatives_from_badge_table(source_table)
    else:
        print("Cached figure4 table not found, fetching GoaT data")
        data = fetch_goat_data_figure_4()
        representatives = select_representative_assemblies(data, seed=1031)

    make_badge_grid(
        representatives,
        directory,
        figure_name="figure6",
        badge_directory=source_directory,
        dpi=150,
    )


def figure7(directory: str):
    if directory is None:
        directory = "figure7"
    print("Drawing snail plots for Figure 7")
    draw_plot(
        "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/GCA_949316315.1",
        directory,
        "7A.svg",
        [
            "--show-numbers",
            "--assembly-name",
            "GCA_949316315.1",
            "--reference",
            "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/964/188/535/GCA_964188535.1_C57BL_6J_T2T_v1/GCA_964188535.1_C57BL_6J_T2T_v1_genomic.fna.gz",
            "--reference-name",
            "GCA_964188535.1 T2T",
            "--show-score",
            "--score-type",
            "g",
            "--output",
            f"{directory}/7A.yaml",
        ],
    )
    draw_plot(
        "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/DLUB01.1",
        directory,
        "7B.svg",
        [
            "--show-numbers",
            "--assembly-name",
            "GCA_003033685.1",
            "--reference",
            "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/055/504/915/GCA_055504915.1_China_Antique-T2T/GCA_055504915.1_China_Antique-T2T_genomic.fna.gz",
            "--reference-name",
            "GCA_055504915.1 T2T",
            "--show-score",
            "--score-type",
            "g",
            "--output",
            f"{directory}/7B.yaml",
        ],
    )
    draw_plot(
        "https://blobtoolkit.genomehubs.org/api/v1/dataset/id/MQPG01",
        directory,
        "7C.svg",
        [
            "--show-numbers",
            "--assembly-name",
            "GCA_003016195.1",
            "--reference",
            "https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/000/292/605/GCA_000292605.2_PoP131/GCA_000292605.2_PoP131_genomic.fna.gz",
            "--reference-name",
            "GCA_000292605.2 T2T",
            "--show-score",
            "--score-type",
            "g",
            "--output",
            f"{directory}/7C.yaml",
        ],
    )
    viewbox = "0 0 1000 1000"
    assemble_figure(
        cols=3,
        label_height=54,
        label_size=54,
        panel_height=554,
        label_offset_x=0,
        panels=[
            {"file": f"{directory}/7A.svg", "viewbox": viewbox},
            {"file": f"{directory}/7B.svg", "viewbox": viewbox},
            {"file": f"{directory}/7C.svg", "viewbox": viewbox},
        ],
        outfile=f"{directory}/figure7.png",
    )
    save_figure7_data(
        directory,
        "figure7_plot_data.tsv",
    )


# Nelumbo nucifera
# T2T GCA_055504915.1 https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/055/504/915/GCA_055504915.1_China_Antique-T2T/GCA_055504915.1_China_Antique-T2T_genomic.fna.gz

# Pyricularia oryzae
# T2T GCA_000292605.2 https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/000/292/605/GCA_000292605.2_PoP131/GCA_000292605.2_PoP131_genomic.fna.gz

# Mus musculus
# T2T GCA_964188535.1 Mus musculus https://ftp.ncbi.nlm.nih.gov/genomes/all/GCA/964/188/535/GCA_964188535.1_C57BL_6J_T2T_v1/GCA_964188535.1_C57BL_6J_T2T_v1_genomic.fna.gz


def parse_args():
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(description="Prepare figures for snail plot paper")
    parser.add_argument("-f", "--figure", help="Figure number", type=int, required=True)
    parser.add_argument(
        "-o", "--output", help="Path to the output directory.", type=str
    )
    return parser.parse_args()


def main():
    args = parse_args()
    if args.figure == 1:
        figure1(args.output)
    elif args.figure == 2:
        figure2(args.output)
    elif args.figure == 3:
        figure3(args.output)
    elif args.figure == 4:
        figure4(args.output)
    elif args.figure == 5:
        figure5()
    elif args.figure == 6:
        figure6(args.output)
    elif args.figure == 7:
        figure7(args.output)
    else:
        print(f"ERROR: {args.figure} is not a valid figure number")
        exit(1)


if __name__ == "__main__":
    main()
