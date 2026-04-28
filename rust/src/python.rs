use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

mod depth;
mod filter;
mod io;
mod plot;
mod utils;

#[pymodule]
fn _blobtk(py: Python<'_>, m: Bound<'_, PyModule>) -> PyResult<()> {
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

    let io = PyModule::new(py, "io")?;
    io.add_function(wrap_pyfunction!(io::read_list, &io)?)?;
    io.add_function(wrap_pyfunction!(io::write_list, &io)?)?;
    io.add_function(wrap_pyfunction!(io::open_lines, &io)?)?;
    io.add_function(wrap_pyfunction!(io::open_writer, &io)?)?;
    io.add_function(wrap_pyfunction!(io::open_append_writer, &io)?)?;
    io.add_function(wrap_pyfunction!(io::write_text, &io)?)?;
    io.add_function(wrap_pyfunction!(io::append_text, &io)?)?;
    io.add_function(wrap_pyfunction!(io::read_csv, &io)?)?;
    io.add_function(wrap_pyfunction!(io::write_csv, &io)?)?;
    io.add_function(wrap_pyfunction!(io::open_lines_iter, &io)?)?;
    io.add_function(wrap_pyfunction!(io::csv_record_iter, &io)?)?;
    io.add_function(wrap_pyfunction!(io::read_fastx, &io)?)?;
    io.add_function(wrap_pyfunction!(io::fastx_record_iter, &io)?)?;
    m.add_submodule(&io)?;

    Ok(())
}
