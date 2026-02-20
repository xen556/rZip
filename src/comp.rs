use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write, copy};
use zstd::stream::Encoder;
use zstd::stream::Decoder;
use tar::{Archive, Builder};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use zip::ZipArchive;
use std::process::{Stdio, Command};

const FORMAT_VERSION: u8 = 0x01;
const SUPPORTED_VERSIONS: &[u8] = &[0x01];

fn progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.white} {bytes}/{total_bytes} ({percent}%)")
            .unwrap()
    );
    pb
}

fn spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.white} {msg}")
            .unwrap()
    );
    pb
}

fn print_compression_stats(input_size: u64, output_path: &str) {
    if let Ok(meta) = fs::metadata(output_path) {
        let output_size = meta.len();
        let saved = input_size.saturating_sub(output_size);
        let ratio = if input_size > 0 {
            (saved as f64 / input_size as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "  Before: {:.2} MB  ->  After: {:.2} MB  ({:.1}% saved)",
            input_size as f64 / 1_048_576.0,
            output_size as f64 / 1_048_576.0,
            ratio,
        );
    }
}

// Checks that a path from the archive does not escape the output directory (Zip Slip)
fn safe_join(base: &Path, untrusted: &Path) -> io::Result<std::path::PathBuf> {
    let joined = base.join(untrusted);
    let canonical_base = base.canonicalize()?;
    // joined may not exist yet so canonicalize() would fail —
    // manually resolve the path by iterating components
    let mut resolved = canonical_base.clone();
    for component in untrusted.components() {
        match component {
            std::path::Component::Normal(c) => resolved.push(c),
            std::path::Component::CurDir => {}
            // '..' or absolute path — potential attack
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unsafe path in archive: {}", joined.display()),
                ));
            }
        }
    }
    Ok(resolved)
}

// Wrapper that tracks bytes read and updates a progress bar
struct ReadWithProgress<R: Read> {
    inner: R,
    pb: ProgressBar,
}

impl<R: Read> Read for ReadWithProgress<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.pb.inc(n as u64);
        Ok(n)
    }
}

pub fn compress(input_path: &str, output_path: &str, comp_lvl: i32) -> io::Result<()> {
    let metadata = fs::metadata(input_path)?;

    if metadata.is_file() {
        let input_size = metadata.len();
        let result = compress_file(input_path, output_path, comp_lvl, &metadata);
        if result.is_err() {
            let _ = fs::remove_file(output_path);
        } else {
            print_compression_stats(input_size, output_path);
        }
        return result;
    }

    let input_size = calc_dir_size(Path::new(input_path))?;
    let result = compress_dir(input_path, output_path, comp_lvl);
    if result.is_err() {
        let _ = fs::remove_file(output_path);
    } else {
        print_compression_stats(input_size, output_path);
    }
    result
}

fn compress_file(input_path: &str, output_path: &str, comp_lvl: i32, metadata: &fs::Metadata) -> io::Result<()> {
    let mut input = File::open(input_path)?;
    let mut output = File::create(output_path)?;
    output.write_all(&[b'R', b'Z', b'I', b'P', b'F', FORMAT_VERSION])?;
    let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
    let mut encoder = Encoder::new(writer, comp_lvl)?;
    encoder.include_checksum(true)?;
    encoder.multithread(num_cpus::get() as u32)?;
    let mut buffer = vec![0u8; 32 * 1024 * 1024];

    let pb = progress_bar(metadata.len());
    loop {
        let bytes_read = input.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        encoder.write_all(&buffer[..bytes_read])?;
        pb.inc(bytes_read as u64);
    }
    pb.finish();
    encoder.finish()?;
    Ok(())
}

fn compress_dir(input_path: &str, output_path: &str, comp_lvl: i32) -> io::Result<()> {
    let mut output = File::create(output_path)?;
    output.write_all(&[b'R', b'Z', b'I', b'P', b'D', FORMAT_VERSION])?;
    let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
    let mut encoder = Encoder::new(writer, comp_lvl)?;
    encoder.include_checksum(true)?;
    encoder.multithread(num_cpus::get() as u32)?;

    let total_size = calc_dir_size(Path::new(input_path))?;
    let pb = progress_bar(total_size);

    let input_root = Path::new(input_path);
    let mut tar_builder = Builder::new(encoder);
    append_dir_with_progress(&mut tar_builder, input_root, input_root, &pb)?;
    let encoder = tar_builder.into_inner()?;
    encoder.finish()?;
    pb.finish();
    Ok(())
}

pub fn extract(input_path: &str, output_path: &str) -> io::Result<()> {
    let mut input_file = File::open(input_path)?;

    let mut header = [0u8; 6];
    input_file.read_exact(&mut header)?;

    match detect_format(&header)? {
        Some(ArchiveFormat::RzipFile) => {
            let file_size = input_file.metadata()?.len();
            let pb = progress_bar(file_size);
            let reader = BufReader::with_capacity(32 * 1024 * 1024, input_file);
            let tracked = ReadWithProgress { inner: reader, pb: pb.clone() };
            let mut decoder = Decoder::new(tracked)?;
            let mut output = File::create(output_path)?;
            let result = copy(&mut decoder, &mut output);
            if result.is_err() {
                let _ = fs::remove_file(output_path);
            }
            pb.finish();
            result.map(|_| ())
        }

        Some(ArchiveFormat::RzipDir) => {
            let file_size = input_file.metadata()?.len();
            let pb = progress_bar(file_size);
            let reader = BufReader::with_capacity(32 * 1024 * 1024, input_file);
            let tracked = ReadWithProgress { inner: reader, pb: pb.clone() };
            let decoder = Decoder::new(tracked)?;
            let mut archive = Archive::new(decoder);
            let result = archive.unpack(output_path);
            if result.is_err() {
                let _ = fs::remove_dir_all(output_path);
            }
            pb.finish();
            result
        }

        Some(ArchiveFormat::Zip) => {
            extract_zip(input_path, output_path)
        }

        Some(ArchiveFormat::Rar) => {
            extract_rar(input_path, output_path)
        }

        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unknown or unsupported archive format",
        )),
    }
}

pub fn calc_dir_size(path: &Path) -> io::Result<u64> {
    let mut size = 0;
    // Use symlink_metadata so we don't follow symlinks — avoids infinite loops
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            size += calc_dir_size(&entry.path())?;
        }
    } else if meta.is_file() {
        size += meta.len();
    }
    // symlinks are skipped intentionally
    Ok(size)
}

pub fn append_dir_with_progress(
    tar_builder: &mut Builder<Encoder<BufWriter<File>>>,
    path: &Path,
    input_root: &Path,
    pb: &ProgressBar,
) -> io::Result<()> {
    // Use symlink_metadata so we don't follow symlinks — avoids infinite loops
    let meta = fs::symlink_metadata(path)?;

    if meta.is_dir() {
        // Add an explicit directory entry so empty dirs are preserved
        let relative_path = path.strip_prefix(input_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        if relative_path != Path::new("") {
            let mut dir_header = tar::Header::new_gnu();
            dir_header.set_size(0);
            dir_header.set_entry_type(tar::EntryType::Directory);

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                dir_header.set_mode(meta.mode());
            }
            #[cfg(not(unix))]
            {
                dir_header.set_mode(0o755);
            }

            if let Ok(mtime) = meta.modified() {
                if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
                    dir_header.set_mtime(duration.as_secs());
                }
            }

            dir_header.set_cksum();
            tar_builder.append_data(&mut dir_header, relative_path, &mut io::empty())?;
        }

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            append_dir_with_progress(tar_builder, &entry.path(), input_root, pb)?;
        }
    } else if meta.is_file() {
        let mut f = File::open(path)?;

        let mut header = tar::Header::new_gnu();
        header.set_size(meta.len());

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            header.set_mode(meta.mode());
        }
        #[cfg(not(unix))]
        {
            header.set_mode(0o644);
        }

        if let Ok(mtime) = meta.modified() {
            if let Ok(duration) = mtime.duration_since(std::time::UNIX_EPOCH) {
                header.set_mtime(duration.as_secs());
            }
        }

        header.set_cksum();

        let relative_path = path.strip_prefix(input_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        struct ReadWithProgressRef<'a> {
            file: &'a mut File,
            pb: &'a ProgressBar,
        }
        impl<'a> Read for ReadWithProgressRef<'a> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.file.read(buf)?;
                self.pb.inc(n as u64);
                Ok(n)
            }
        }
        let mut reader = ReadWithProgressRef { file: &mut f, pb };
        tar_builder.append_data(&mut header, relative_path, &mut reader)?;
    }
    // symlinks are skipped intentionally
    Ok(())
}

pub fn extract_zip(input_path: &str, output_path: &str) -> io::Result<()> {
    let output_base = Path::new(output_path);

    // Create output dir before canonicalize in safe_join requires it to exist
    fs::create_dir_all(output_base)?;

    let file = File::open(input_path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut total_size: u64 = 0;
    for i in 0..archive.len() {
        if let Ok(f) = archive.by_index(i) {
            total_size += f.size();
        }
    }

    let pb = progress_bar(total_size);

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // Zip Slip: validate every path before writing
        let outpath = safe_join(output_base, &file.mangled_name())?;

        if file.is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                fs::create_dir_all(p)?;
            }
            let mut outfile = File::create(&outpath)?;
            let size = file.size();
            io::copy(&mut file, &mut outfile)?;
            pb.inc(size);

            // Restore Unix permissions if available
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                }
            }
        }
    }
    pb.finish();
    Ok(())
}

pub fn extract_rar(input_path: &str, output_path: &str) -> io::Result<()> {
    // Check if unrar is installed before doing anything
    which_unrar()?;

    fs::create_dir_all(output_path)?;

    let pb = spinner();
    pb.set_message("Extracting RAR...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let status = Command::new("unrar")
        .arg("x")
        .arg("-o+")
        .arg("-idq")
        .arg(input_path)
        .arg(output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    pb.finish_with_message("Done.");

    if !status.success() {
        let _ = fs::remove_dir_all(output_path);
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unrar failed with status: {}", status),
        ));
    }
    Ok(())
}

fn which_unrar() -> io::Result<()> {
    let result = Command::new("unrar")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    if result.is_err() {
        let install_hint = if cfg!(target_os = "windows") {
            "Download from: https://www.rarlab.com/download.htm"
        } else if cfg!(target_os = "macos") {
            "Install with: brew install rar"
        } else {
            "Install with: sudo apt install unrar"
        };
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("unrar is not installed. {}", install_hint),
        ));
    }
    Ok(())
}

enum ArchiveFormat {
    RzipFile,
    RzipDir,
    Zip,
    Rar,
}

fn check_version(version: u8) -> io::Result<()> {
    if !SUPPORTED_VERSIONS.contains(&version) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Unsupported archive version: 0x{:02X}. This build supports versions: {}",
                version,
                SUPPORTED_VERSIONS.iter().map(|v| format!("0x{:02X}", v)).collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    Ok(())
}

fn detect_format(header: &[u8]) -> io::Result<Option<ArchiveFormat>> {
    if header.len() < 6 {
        return Ok(None);
    }

    if &header[..5] == b"RZIPF" {
        check_version(header[5])?;
        return Ok(Some(ArchiveFormat::RzipFile));
    }

    if &header[..5] == b"RZIPD" {
        check_version(header[5])?;
        return Ok(Some(ArchiveFormat::RzipDir));
    }

    if &header[..4] == b"PK\x03\x04"
        || &header[..4] == b"PK\x05\x06"
        || &header[..4] == b"PK\x07\x08"
    {
        return Ok(Some(ArchiveFormat::Zip));
    }

    if header.starts_with(b"Rar!\x1A\x07") {
        return Ok(Some(ArchiveFormat::Rar));
    }

    Ok(None)
}