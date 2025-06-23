use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

mod depth;
mod filter;
mod plot;
mod utils;

#[pymodule]
fn blobtk(py: Python<'_>, m: Bound<'_, PyModule>) -> PyResult<()> {
    let plot = PyModule::new(py, "plot")?;
    plot.add_function(wrap_pyfunction!(plot::blob, &plot)?)?;
    plot.add_function(wrap_pyfunction!(plot::cumulative, &plot)?)?;
    plot.add_function(wrap_pyfunction!(plot::legend, &plot)?)?;
    plot.add_function(wrap_pyfunction!(plot::plot, &plot)?)?;
    plot.add_function(wrap_pyfunction!(plot::snail, &plot)?)?;
    m.add_submodule(&plot)?;

    let filter = PyModule::new(py, "filter")?;
    filter.add_function(wrap_pyfunction!(filter::fastx, &filter)?)?;
    m.add_submodule(&filter)?;

    let depth = PyModule::new(py, "depth")?;
    depth.add_function(wrap_pyfunction!(depth::bam_to_bed, &depth)?)?;
    depth.add_function(wrap_pyfunction!(depth::bam_to_depth, &depth)?)?;
    m.add_submodule(&depth)?;

    Ok(())
}
