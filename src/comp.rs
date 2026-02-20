use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write, copy};
use zstd::stream::Encoder;
use zstd::stream::Decoder;
use tar::{Archive, Builder};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use zip::ZipArchive;
use std::process::{Stdio, Command};

pub fn compress(input_path: &str, output_path: &str, comp_lvl: i32) -> io::Result<()> {

    let metadata = fs::metadata(input_path)?;

    if metadata.is_file() {
        let mut input = File::open(input_path)?;
        let mut output = File::create(output_path)?;
        output.write_all(b"RZIPF\x01")?;
        let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
        let mut encoder = Encoder::new(writer, comp_lvl)?;
        encoder.include_checksum(true)?;
        encoder.multithread(num_cpus::get() as u32)?;
        let mut buffer = vec![0u8; 32 * 1024 * 1024];

        let pb = ProgressBar::new(metadata.len());
        pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.white} {bytes}/{total_bytes} ({percent}%)")
            .unwrap()
        );
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
        return Ok(());
    }

    let mut output = File::create(output_path)?;
    output.write_all(b"RZIPD\x01")?;
    let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
    let mut encoder = Encoder::new(writer, comp_lvl)?;
    encoder.include_checksum(true)?;
    encoder.multithread(num_cpus::get() as u32)?;

    let total_size = calc_dir_size(Path::new(input_path))?;
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.white} {bytes}/{total_bytes} ({percent}%)")
            .unwrap()
    );

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

    let mut header = [0u8; 8];
    input_file.read_exact(&mut header)?;

    match detect_format(&header) {
        Some(ArchiveFormat::RzipFile) => {
            let reader = BufReader::with_capacity(32 * 1024 * 1024, input_file);
            let mut decoder = Decoder::new(reader)?;
            let mut output = File::create(output_path)?;
            copy(&mut decoder, &mut output)?;
            Ok(())
        }

        Some(ArchiveFormat::RzipDir) => {
            let reader = BufReader::with_capacity(32 * 1024 * 1024, input_file);
            let decoder = Decoder::new(reader)?;
            let mut archive = Archive::new(decoder);
            archive.unpack(output_path)?;
            Ok(())
        }

        Some(ArchiveFormat::Zip) => {
            extract_zip(input_path, output_path)
        }

        Some(ArchiveFormat::Rar) => {
            extract_rar(input_path, output_path)
        }

        None => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Unknown archive format",
        )),
    }
}

pub fn calc_dir_size(path: &Path) -> io::Result<u64> {
    let mut size = 0;    
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            size += calc_dir_size(&path)?;
        }
    } else if path.is_file() {
        size += fs::metadata(path)?.len();
    }
    Ok(size)
}

pub fn append_dir_with_progress(
    tar_builder: &mut Builder<Encoder<BufWriter<File>>>,
    path: &Path,
    input_root: &Path,
    pb: &ProgressBar,
) -> io::Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            append_dir_with_progress(tar_builder, &entry.path(), input_root, pb)?;
        }
    } else if path.is_file() {
        let mut f = File::open(path)?;
        let size = fs::metadata(path)?.len();

        let mut header = tar::Header::new_gnu();
        header.set_size(size);
        header.set_cksum();

        let relative_path = path.strip_prefix(input_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        struct ReadWithProgress<'a> {
            file: &'a mut File,
            pb: &'a ProgressBar,
        }
        impl<'a> Read for ReadWithProgress<'a> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let n = self.file.read(buf)?;
                self.pb.inc(n as u64);
                Ok(n)
            }
        }
        let mut reader = ReadWithProgress { file: &mut f, pb };

        tar_builder.append_data(&mut header, relative_path, &mut reader)?;
    }
    Ok(())
}

pub fn extract_zip(input_path: &str, output_path: &str) -> io::Result<()> {
    let file = File::open(input_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = std::path::Path::new(output_path).join(file.mangled_name());

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        }
        else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
    }
    Ok(())
}

pub fn extract_rar(input_path: &str, output_path: &str) -> io::Result<()> {
    if Path::new(output_path).exists() {
        let mut entries = fs::read_dir(output_path)?;
        if entries.next().is_some() {
            println!("Output directory is not empty. Overwrite? (y/n)");

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;

            if input.trim().to_lowercase() != "y" {
                return Ok(());
            }
        }
    }
    fs::create_dir_all(output_path)?;

    let status = Command::new("unrar")
        .arg("x")
        .arg("-o+")
        .arg("-idq") 
        .arg(input_path)
        .arg(output_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("unrar failed with status: {}", status),
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

fn detect_format(header: &[u8]) -> Option<ArchiveFormat> {
    if &header[..5] == b"RZIPF\x01" {
        return Some(ArchiveFormat::RzipFile);
    }

    if &header[..5] == b"RZIPD\x01" {
        return Some(ArchiveFormat::RzipDir);
    }

    if &header[..4] == b"PK\x03\x04"
        || &header[..4] == b"PK\x05\x06"
        || &header[..4] == b"PK\x07\x08"
    {
        return Some(ArchiveFormat::Zip);
    }

    if header.starts_with(b"Rar!\x1A\x07") {
        return Some(ArchiveFormat::Rar);
    }
    None
}