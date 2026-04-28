use flate2::write;
use flate2::Compression;
use pyo3::prelude::*;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufWriter, Read, Write};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, IntoRawFd};
use std::path::{Path, PathBuf};

use crate::io as rust_io;

use pyo3::Py;
use pyo3::PyRefMut;

#[pyclass(unsendable)]
pub struct LineIter {
    reader: Option<Box<dyn BufRead>>,
    buf: String,
}

#[pymethods]
impl LineIter {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<LineIter>> {
        Ok(slf.into())
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<String>> {
        let mut reader_opt = slf.reader.take();
        let mut ret = None;
        if let Some(mut reader) = reader_opt {
            slf.buf.clear();
            match reader.read_line(&mut slf.buf) {
                Ok(0) => ret = None,
                Ok(_) => {
                    if slf.buf.ends_with('\n') {
                        slf.buf.pop();
                        if slf.buf.ends_with('\r') {
                            slf.buf.pop();
                        }
                    }
                    ret = Some(slf.buf.clone());
                }
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "{}",
                        e
                    )))
                }
            }
            slf.reader = Some(reader);
        }
        Ok(ret)
    }
}

#[pyclass(unsendable)]
pub struct CsvIter {
    rdr: Option<csv::Reader<Box<dyn BufRead>>>,
}

#[pymethods]
impl CsvIter {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<CsvIter>> {
        Ok(slf.into())
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<Vec<String>>> {
        let mut rdr_opt = slf.rdr.take();
        let mut ret: Option<Vec<String>> = None;
        if let Some(mut rdr) = rdr_opt {
            match rdr.records().next() {
                Some(Ok(rec)) => ret = Some(rec.iter().map(|s| s.to_string()).collect()),
                Some(Err(e)) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "{}",
                        e
                    )))
                }
                None => ret = None,
            }
            slf.rdr = Some(rdr);
        }
        Ok(ret)
    }
}

#[pyfunction]
pub fn read_list(file_path: Option<PathBuf>) -> PyResult<Vec<String>> {
    let set: HashSet<Vec<u8>> = rust_io::get_list(&file_path);
    let mut vecs: Vec<String> = set
        .into_iter()
        .map(|b| String::from_utf8(b).unwrap_or_default())
        .collect();
    vecs.sort();
    Ok(vecs)
}

#[pyfunction]
#[pyo3(signature = (entries, file_path=None))]
pub fn write_list(entries: Vec<String>, file_path: Option<PathBuf>) -> PyResult<()> {
    let mut set: HashSet<Vec<u8>> = HashSet::new();
    for e in entries {
        set.insert(e.into_bytes());
    }
    rust_io::write_list(&set, &file_path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    Ok(())
}

#[pyfunction]
pub fn open_lines(path: String) -> PyResult<Vec<String>> {
    let pb = PathBuf::from(path);
    let reader = rust_io::file_reader(pb)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    let mut lines: Vec<String> = Vec::new();
    let mut r = reader;
    let mut buf = String::new();
    loop {
        buf.clear();
        match r.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                if buf.ends_with('\n') {
                    buf.pop();
                    if buf.ends_with('\r') {
                        buf.pop();
                    }
                }
                lines.push(buf.clone());
            }
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                    "{}",
                    e
                )))
            }
        }
    }
    Ok(lines)
}

#[pyfunction]
pub fn open_lines_iter(py: Python, path: String) -> PyResult<Py<LineIter>> {
    let pb = PathBuf::from(path);
    let reader = rust_io::file_reader(pb)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    let iter = LineIter {
        reader: Some(reader),
        buf: String::new(),
    };
    Py::new(py, iter)
}

#[pyclass(unsendable)]
pub struct PyWriter {
    writer: Option<Box<dyn Write>>,
    file: Option<File>,
}

#[pymethods]
impl PyWriter {
    fn write(&mut self, text: String) -> PyResult<usize> {
        let bytes = text.as_bytes();
        if let Some(w) = self.writer.as_mut() {
            w.write_all(bytes)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
            return Ok(bytes.len());
        }
        Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
            "writer closed",
        ))
    }

    fn write_bytes(&mut self, data: Vec<u8>) -> PyResult<usize> {
        if let Some(w) = self.writer.as_mut() {
            w.write_all(&data)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
            return Ok(data.len());
        }
        Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
            "writer closed",
        ))
    }

    fn writelines(&mut self, lines: Vec<String>) -> PyResult<()> {
        if let Some(w) = self.writer.as_mut() {
            for l in lines {
                w.write_all(l.as_bytes())
                    .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
            }
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
                "writer closed",
            ))
        }
    }

    fn flush(&mut self) -> PyResult<()> {
        if let Some(w) = self.writer.as_mut() {
            w.flush()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
            Ok(())
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
                "writer closed",
            ))
        }
    }

    fn close(&mut self) -> PyResult<()> {
        self.writer = None;
        Ok(())
    }

    fn fileno(&self) -> PyResult<Option<i32>> {
        #[cfg(unix)]
        {
            if let Some(f) = &self.file {
                return Ok(Some(f.as_raw_fd() as i32));
            }
            Ok(None)
        }
        #[cfg(not(unix))]
        {
            Ok(None)
        }
    }

    #[cfg(unix)]
    fn detach(&mut self) -> PyResult<i32> {
        if let Some(f) = self.file.take() {
            let raw = f.into_raw_fd();
            Ok(raw as i32)
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
                "no file to detach",
            ))
        }
    }

    #[cfg(not(unix))]
    fn detach(&mut self) -> PyResult<i32> {
        Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(
            "detach not supported on this platform",
        ))
    }

    fn __enter__(slf: PyRefMut<Self>) -> PyResult<Py<PyWriter>> {
        Ok(slf.into())
    }

    fn __exit__(
        mut slf: PyRefMut<Self>,
        _exc_type: Option<PyObject>,
        _exc_val: Option<PyObject>,
        _tb: Option<PyObject>,
    ) -> PyResult<()> {
        if let Some(mut w) = slf.writer.take() {
            let _ = w.flush();
        }
        Ok(())
    }
}

#[pyfunction]
pub fn open_writer(py: Python, file_path: Option<PathBuf>) -> PyResult<Py<PyWriter>> {
    // Construct a file-backed writer while keeping an owned File for detach()
    let mut file_for_detach: Option<File> = None;
    let w: Box<dyn Write> = match &file_path {
        Some(p) if p == Path::new("-") => Box::new(BufWriter::new(std::io::stdout().lock())),
        Some(p) => {
            if let Err(e) = std::fs::create_dir_all(p.parent().unwrap()) {
                return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                    "couldn't create directory {}: {}",
                    p.parent().unwrap().display(),
                    e
                )));
            }
            let file_main = match File::create(p) {
                Ok(f) => f,
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "couldn't open {}: {}",
                        p.display(),
                        e
                    )))
                }
            };
            let file_clone = match file_main.try_clone() {
                Ok(f) => f,
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "couldn't clone file {}: {}",
                        p.display(),
                        e
                    )))
                }
            };
            file_for_detach = Some(file_main);
            if p.extension() == Some(OsStr::new("gz")) {
                Box::new(BufWriter::with_capacity(
                    128 * 1024,
                    write::GzEncoder::new(file_clone, Compression::default()),
                ))
            } else {
                Box::new(BufWriter::with_capacity(128 * 1024, file_clone))
            }
        }
        None => Box::new(BufWriter::new(std::io::stdout().lock())),
    };
    let writer = PyWriter {
        writer: Some(w),
        file: file_for_detach,
    };
    Py::new(py, writer)
}

#[pyfunction]
pub fn open_append_writer(py: Python, file_path: Option<PathBuf>) -> PyResult<Py<PyWriter>> {
    // Construct append writer while keeping a File for detach()
    let mut file_for_detach: Option<File> = None;
    let w: Box<dyn Write> = match &file_path {
        Some(p) if p == Path::new("-") => Box::new(BufWriter::new(std::io::stdout().lock())),
        Some(p) => {
            if let Err(e) = std::fs::create_dir_all(p.parent().unwrap()) {
                return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                    "couldn't create directory {}: {}",
                    p.parent().unwrap().display(),
                    e
                )));
            }
            let file_main = match OpenOptions::new().append(true).create(true).open(p) {
                Ok(f) => f,
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "couldn't open {}: {}",
                        p.display(),
                        e
                    )))
                }
            };
            let file_clone = match file_main.try_clone() {
                Ok(f) => f,
                Err(e) => {
                    return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                        "couldn't clone file {}: {}",
                        p.display(),
                        e
                    )))
                }
            };
            file_for_detach = Some(file_main);
            if p.extension() == Some(OsStr::new("gz")) {
                Box::new(BufWriter::with_capacity(
                    128 * 1024,
                    write::GzEncoder::new(file_clone, Compression::default()),
                ))
            } else {
                Box::new(BufWriter::with_capacity(128 * 1024, file_clone))
            }
        }
        None => Box::new(BufWriter::new(std::io::stdout().lock())),
    };
    let writer = PyWriter {
        writer: Some(w),
        file: file_for_detach,
    };
    Py::new(py, writer)
}

#[pyfunction]
#[pyo3(signature = (text, file_path=None))]
pub fn write_text(text: String, file_path: Option<PathBuf>) -> PyResult<()> {
    let mut w = rust_io::get_writer(&file_path);
    w.write_all(text.as_bytes())
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (text, file_path=None))]
pub fn append_text(text: String, file_path: Option<PathBuf>) -> PyResult<()> {
    let mut w = rust_io::get_append_writer(&file_path);
    w.write_all(text.as_bytes())
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    Ok(())
}

#[pyfunction]
#[pyo3(signature = (file_path=None, delimiter='\t', has_headers=true, comment_char=None, skip_lines=0, flexible=true))]
pub fn read_csv(
    file_path: Option<PathBuf>,
    delimiter: char,
    has_headers: bool,
    comment_char: Option<char>,
    skip_lines: usize,
    flexible: bool,
) -> PyResult<Vec<Vec<String>>> {
    let delim_byte = delimiter as u8;
    let comment = comment_char.map(|c| c as u8);
    let rdr = rust_io::get_csv_reader(
        &file_path,
        delim_byte,
        has_headers,
        comment,
        skip_lines,
        flexible,
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    let mut rows: Vec<Vec<String>> = Vec::new();
    for result in rdr.into_records() {
        match result {
            Ok(rec) => rows.push(rec.iter().map(|s| s.to_string()).collect()),
            Err(e) => {
                return Err(PyErr::new::<pyo3::exceptions::PyIOError, _>(format!(
                    "{}",
                    e
                )))
            }
        }
    }
    Ok(rows)
}

#[pyfunction]
#[pyo3(signature = (rows, file_path=None, delimiter='\t', headers=None))]
pub fn write_csv(
    rows: Vec<Vec<String>>,
    file_path: Option<PathBuf>,
    delimiter: char,
    headers: Option<Vec<String>>,
) -> PyResult<()> {
    let delim_byte = delimiter as u8;
    let mut wtr = rust_io::get_csv_writer(&file_path, delim_byte);
    if let Some(h) = headers {
        wtr.write_record(h.iter())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    }
    for row in rows {
        wtr.write_record(row.iter())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    }
    wtr.flush()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    Ok(())
}

#[pyfunction]
pub fn csv_record_iter(
    py: Python,
    file_path: Option<PathBuf>,
    delimiter: char,
    has_headers: bool,
    comment_char: Option<char>,
    skip_lines: usize,
    flexible: bool,
) -> PyResult<Py<CsvIter>> {
    let delim_byte = delimiter as u8;
    let comment = comment_char.map(|c| c as u8);
    let rdr = rust_io::get_csv_reader(
        &file_path,
        delim_byte,
        has_headers,
        comment,
        skip_lines,
        flexible,
    )
    .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(format!("{}", e)))?;
    let iter = CsvIter { rdr: Some(rdr) };
    Py::new(py, iter)
}

// Note: functions are registered from the parent `python.rs` module.
