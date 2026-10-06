#!/usr/bin/env python3

import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
import seaborn as sns


def plot_extreme_collapse_distribution(
    bed_path: str,
    min_contig_len: int = 500_000,
    top_n_contigs: int = 20,
    collapse_threshold: float = 10.0,
    output_img: str = "extreme_collapse_distribution.png"
):
    # 1. Load BED/bedGraph data
    df = pd.read_csv(
        bed_path, 
        sep=r'\s+', 
        header=None, 
        names=['chrom', 'start', 'end', 'coverage']
    )
    
    # 2. Filter major contigs and compute 1x baseline depth
    contig_lengths = df.groupby('chrom')['end'].max().sort_values(ascending=False)
    major_contigs = contig_lengths[contig_lengths >= min_contig_len].index
    df_clean = df[df['chrom'].isin(major_contigs)].copy()
    
    cov_nonzero = df_clean[df_clean['coverage'] > 0]['coverage']
    counts, bin_edges = np.histogram(cov_nonzero, bins=200)
    baseline_1x = bin_edges[np.argmax(counts)]
    
    df_clean['multiplier'] = df_clean['coverage'] / baseline_1x
    
    # 3. Select top N longest contigs
    selected_contigs = contig_lengths.head(top_n_contigs).index
    df_top = df_clean[df_clean['chrom'].isin(selected_contigs)].copy()
    
    # 4. Set up two-panel figure
    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 7), gridspec_kw={'width_ratios': [1, 2]})
    
    # --- Panel A: Chromosome Track Overlay ---
    y_positions = {chrom: i for i, chrom in enumerate(reversed(selected_contigs))}
    
    for chrom in selected_contigs:
        y = y_positions[chrom]
        c_len_mb = contig_lengths[chrom] / 1e6
        
        # Draw gray chromosome backbone
        ax1.plot([0, c_len_mb], [y, y], color='lightgray', linewidth=6, zorder=1)
        
        # Overlay extreme collapse locations as red vertical ticks
        sub = df_top[(df_top['chrom'] == chrom) & (df_top['multiplier'] >= collapse_threshold)]
        if not sub.empty:
            x_mb = sub['start'] / 1e6
            ax1.scatter(x_mb, [y]*len(x_mb), color='crimson', s=15, marker='|', zorder=2)
            
    ax1.set_yticks(list(y_positions.values()))
    ax1.set_yticklabels(list(y_positions.keys()), fontsize=9)
    ax1.set_xlabel("Genomic Position (Mb)", fontsize=10, fontweight='bold')
    ax1.set_title(f"Genomic Map of Extreme Collapse (>10x) Across Top {len(selected_contigs)} Contigs", fontsize=11, fontweight='bold')
    ax1.grid(axis='x', linestyle='--', alpha=0.5)
    
    # --- Panel B: Relative Positional Density (0.0 = Start, 1.0 = End) ---
    df_clean['contig_len'] = df_clean['chrom'].map(contig_lengths)
    df_clean['rel_pos'] = (df_clean['start'] + df_clean['end']) / (2 * df_clean['contig_len'])
    
    extreme_pos = df_clean[df_clean['multiplier'] >= collapse_threshold]['rel_pos']
    
    sns.kdeplot(extreme_pos, ax=ax2, fill=True, color='crimson', alpha=0.4, bw_adjust=0.5)
    ax2.set_xlim(0, 1)
    ax2.set_xlabel("Relative Position (0 = Start, 1 = End)", fontsize=10, fontweight='bold')
    ax2.set_ylabel("Density", fontsize=10, fontweight='bold')
    ax2.set_title("Positional Bias\n(Centromeric vs Telomeric)", fontsize=11, fontweight='bold')
    
    plt.tight_layout()
    plt.savefig(output_img, dpi=300, bbox_inches='tight')
    print(f"Plot saved successfully as '{output_img}'.")

# Run on your file:
# plot_extreme_collapse_distribution("path_to_your_bedGraph.gz")


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Plot extreme coverage distribution for TE collapse detection.")
    parser.add_argument("coverage_bed", help="Input 1kb coverage BED file")
    parser.add_argument("output_img", help="Output image file path (e.g., PNG)")
    parser.add_argument("--min_contig_len", type=int, default=500_000, help="Minimum contig length to consider (default: 500,000 bp)")
    parser.add_argument("--top_n_contigs", type=int, default=20, help="Number of top longest contigs to display (default: 20)")
    parser.add_argument("--collapse_threshold", type=float, default=10.0, help="Coverage multiplier threshold for extreme collapse (default: 10.0)")
    args = parser.parse_args()

    plot_extreme_collapse_distribution(
        bed_path=args.coverage_bed,
        output_img=args.output_img,
        min_contig_len=args.min_contig_len,
        top_n_contigs=args.top_n_contigs,
        collapse_threshold=args.collapse_threshold
    )
