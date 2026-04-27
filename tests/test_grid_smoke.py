from pathlib import Path


def test_grid_smoke(tmp_path):
    # import installed blobtk Python extension
    import blobtk
    from blobtk import plot as plotmod

    data_dir = Path(__file__).parent / "fixtures" / "minimal_blobdir"
    out = tmp_path / "out.svg"

    options = {
        "blobdir": str(data_dir),
        "view": "blob",
        "shape": "grid",
        "window_size": "0.1",
        "x_field": "position",
        "y_field": "gc",
        "z_field": "length",
        "output": str(out),
    }

    # This should write an SVG file without requiring rasterisation
    plotmod.plot(**options)

    assert out.exists()
    assert out.stat().st_size > 0
