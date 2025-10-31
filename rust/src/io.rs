extern crate atty;
use std::collections::HashSet;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Result, Write};
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use flate2::write;
use flate2::Compression;
use std::ffi::OsStr;

fn read_stdin() -> Vec<Vec<u8>> {
    let stdin = io::stdin();
    let mut list: Vec<Vec<u8>> = vec![];
    if atty::is(atty::Stream::Stdin) {
        eprintln!("No input on STDIN!");
        return list;
    }
    for line in stdin.lock().lines() {
        let line_as_vec = match line {
            Err(why) => panic!("couldn't read line: {}", why),
            Ok(l) => l.as_bytes().to_vec(),
        };
        list.push(line_as_vec)
    }
    list
}

pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename).expect("no such file");
    Ok(io::BufReader::new(file).lines())
}

fn read_file(file_path: &PathBuf) -> Vec<Vec<u8>> {
    let mut output: Vec<Vec<u8>> = vec![];
    if let Ok(lines) = read_lines(file_path) {
        for line in lines {
            let line_as_vec = match line {
                Err(why) => panic!("couldn't read line: {}", why),
                Ok(l) => l.as_bytes().to_vec(),
            };
            output.push(line_as_vec)
        }
    }
    output
}

pub fn get_list(file_path: &Option<PathBuf>) -> HashSet<Vec<u8>> {
    let list = match file_path {
        None => vec![],
        Some(p) if p == Path::new("-") => read_stdin(),
        Some(_) => read_file(file_path.as_ref().unwrap()),
    };
    HashSet::from_iter(list)
}

pub fn get_file_writer(file_path: &PathBuf, append: bool) -> Box<dyn Write> {
    if let Err(e) = create_dir_all(file_path.parent().unwrap()) {
        panic!(
            "couldn't create directory {}: {}",
            file_path.parent().unwrap().display(),
            e
        );
    }
    let file = if append {
        match OpenOptions::new().append(true).open(file_path) {
            Err(why) => panic!("couldn't open {}: {}", file_path.display(), why),
            Ok(file) => file,
        }
    } else {
        match File::create(file_path) {
            Err(why) => panic!("couldn't open {}: {}", file_path.display(), why),
            Ok(file) => file,
        }
    };

    let writer: Box<dyn Write> = if file_path.extension() == Some(OsStr::new("gz")) {
        Box::new(BufWriter::with_capacity(
            128 * 1024,
            write::GzEncoder::new(file, Compression::default()),
        ))
    } else {
        Box::new(BufWriter::with_capacity(128 * 1024, file))
    };
    writer
}

pub fn get_writer(file_path: &Option<PathBuf>) -> Box<dyn Write> {
    let writer: Box<dyn Write> = match file_path {
        Some(path) if path == Path::new("-") => Box::new(BufWriter::new(io::stdout().lock())),
        Some(path) => {
            create_dir_all(path.parent().unwrap()).unwrap();
            get_file_writer(path, false)
        }
        None => Box::new(BufWriter::new(io::stdout().lock())),
    };
    writer
}

pub fn get_append_writer(file_path: &Option<PathBuf>) -> Box<dyn Write> {
    let writer: Box<dyn Write> = match file_path {
        Some(path) if path == Path::new("-") => Box::new(BufWriter::new(io::stdout().lock())),
        Some(path) => {
            create_dir_all(path.parent().unwrap()).unwrap();
            get_file_writer(path, true)
        }
        None => Box::new(BufWriter::new(io::stdout().lock())),
    };
    writer
}

pub fn get_csv_writer(file_path: &Option<PathBuf>, delimiter: u8) -> csv::Writer<Box<dyn Write>> {
    let file_writer = get_writer(file_path);
    if delimiter == b'\t' {
        csv::WriterBuilder::new()
            .delimiter(b'\t')
            .from_writer(file_writer)
    } else {
        csv::WriterBuilder::new().from_writer(file_writer)
    }
}

/// Return a BufRead object for a given file path.
/// If the file path has a `.gz` extension, the file is decompressed on the fly.
pub fn local_file_reader(path: PathBuf) -> io::Result<Box<dyn BufRead>> {
    let file = File::open(&path)?;

    if path.extension() == Some(OsStr::new("gz")) {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Return a BufRead object for a given URL path.
/// The file will be fetched.
pub fn remote_file_reader(url: &str) -> io::Result<Box<dyn BufRead>> {
    // let response = reqwest::blocking::get(path)?;
    // let reader = response.bytes()?;
    // Ok(Box::new(BufReader::new(reader.as_ref()))

    let response = reqwest::blocking::get(url.to_string()).expect("Failed to fetch file");
    if response.status().is_success() {
        return Ok(Box::new(BufReader::new(response)));
    } else {
        let response = reqwest::blocking::get(url.to_string().replace(".gz", ""))
            .expect(format!("Failed to fetch file: {}", url).as_str());
        if response.status().is_success() {
            return Ok(Box::new(BufReader::new(response)));
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to fetch file: {}", response.status()),
            ));
        }
    }
}

pub fn ssh_file_reader(path: &str) -> io::Result<Box<dyn BufRead>> {
    // Remove protocol from path
    let path = path.replace("ssh://", "");
    // Split the path into host and file
    let parts: Vec<&str> = path.split(':').collect();
    if parts.len() != 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid SSH path format. Expected ssh://host:path",
        ));
    }
    let host = parts[0];
    let path = parts[1];
    // Use SSH to read the file
    let command = if path.ends_with(".gz") {
        format!(
            "ssh {} 'if [ -f {} ]; then cat {}; else cat {}; fi'",
            host,
            path,
            path,
            path.trim_end_matches(".gz")
        )
    } else {
        format!("ssh {} cat {}", host, path)
    };

    let process = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start SSH command");

    let stdout = process
        .stdout
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "Failed to capture stdout"))?;
    let mut buffer = [0u8; 2];
    let mut stdout_reader = BufReader::new(stdout);
    match io::Read::read_exact(&mut stdout_reader, &mut buffer) {
        Ok(_) => {
            let is_gzipped = buffer == [0x1F, 0x8B];
            let stdout = io::Read::chain(std::io::Cursor::new(buffer), stdout_reader);
            if is_gzipped {
                Ok(Box::new(BufReader::new(GzDecoder::new(stdout))))
            } else {
                Ok(Box::new(BufReader::new(stdout)))
            }
        }
        Err(e) => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            format!("Failed to read from SSH output: {}", e),
        )),
    }
}

/// Return a BufRead object for a given file path.
/// If the path is a URL the file will be fetched.
pub fn file_reader(path: PathBuf) -> io::Result<Box<dyn BufRead>> {
    if path.to_string_lossy().starts_with("http") {
        return remote_file_reader(&path.to_string_lossy());
    } else if path.to_string_lossy().starts_with("ssh") {
        return ssh_file_reader(&path.to_string_lossy());
    }

    let file = File::open(&path);

    if path.extension() == Some(OsStr::new("gz")) {
        match file {
            Ok(f) => {
                // Check gzip magic bytes
                let mut magic = [0u8; 2];
                let mut f_clone = f.try_clone()?;
                use std::io::Read;
                f_clone.read_exact(&mut magic)?;
                if magic != [0x1F, 0x8B] {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "File {} has .gz extension but is not a valid gzip file (magic bytes: {:x?})",
                            path.display(),
                            magic
                        ),
                    ));
                }
                // Re-open for actual reading
                let file = File::open(&path)?;
                Ok(Box::new(BufReader::new(GzDecoder::new(file))))
            }
            Err(_) => {
                // Try unzipped file
                let mut unzipped_path = path.clone();
                unzipped_path.set_extension("");
                let file = File::open(&unzipped_path)?;
                Ok(Box::new(BufReader::new(file)))
            }
        }
    } else {
        let file = file?;
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Return an empty Box<dyn BufRead>.
/// This is useful when we want to read from stdin.
pub fn get_empty_reader() -> Box<dyn BufRead> {
    Box::new(BufReader::new(io::empty()))
}

/// Return a csv::Reader object for a given file path.
/// If the file path has a `.gz` extension, the file is decompressed on the fly.
pub fn get_csv_reader(
    file_path: &Option<PathBuf>,
    delimiter: u8,
    has_headers: bool,
    comment_char: Option<u8>,
    skip_lines: usize,
    flexible: bool,
) -> csv::Reader<Box<dyn BufRead>> {
    dbg!(&file_path);
    let file_reader =
        file_reader(file_path.as_ref().unwrap().clone()).expect("Failed to read file");
    // Skip the first `skip_lines` lines
    let mut file_reader = Box::new(file_reader);
    for _ in 0..skip_lines {
        let mut line = String::new();
        file_reader.read_line(&mut line).unwrap();
    }

    csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(has_headers)
        .comment(comment_char)
        .flexible(flexible) // Allow incomplete rows
        .from_reader(file_reader)
}

pub fn write_list(entries: &HashSet<Vec<u8>>, file_path: &Option<PathBuf>) -> Result<()> {
    let mut writer = get_writer(file_path);
    for line in entries.iter() {
        writeln!(&mut writer, "{}", String::from_utf8(line.to_vec()).unwrap())?;
    }
    Ok(())
}

pub fn append_to_path(p: &PathBuf, s: &str) -> PathBuf {
    let mut p = p.clone().into_os_string();
    p.push(s);
    p.into()
}
