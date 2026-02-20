use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write, copy};
use zstd::stream::Encoder;
use zstd::stream::Decoder;
use tar::{Archive, Builder};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

pub fn compress(input_path: &str, output_path: &str, compress_level: i32) -> io::Result<()> {

    if !(1..=22).contains(&compress_level) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid compression level (max 22)!"));
    }

    let metadata = fs::metadata(input_path)?;

    if metadata.is_file() {
        let mut input = File::open(input_path)?;
        let mut output = File::create(output_path)?;
        output.write_all(b"RZIPF")?;
        let writer = BufWriter::with_capacity(16 * 1024 * 1024, output);
        let mut encoder = Encoder::new(writer, compress_level)?;
        encoder.include_checksum(true)?;
        encoder.multithread(num_cpus::get() as u32)?;
        let mut buffer = vec![0u8; 16 * 1024 * 1024];

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
    output.write_all(b"RZIPD")?;
    let writer = BufWriter::with_capacity(16 * 1024 * 1024, output);
    let mut encoder = Encoder::new(writer, compress_level)?;
    encoder.include_checksum(true)?;
    encoder.multithread(num_cpus::get() as u32)?;

    let total_size = calc_dir_size(Path::new(input_path))?;
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40.white} {bytes}/{total_bytes} ({percent}%)")
            .unwrap()
    );

    {
        let mut tar_builder = Builder::new(&mut encoder);
        tar_builder.append_dir_all(input_path, input_path)?;
        tar_builder.finish()?;
    }

    encoder.finish()?;
    Ok(())
}

pub fn extract(input_path: &str, output_path: &str) -> io::Result<()> {
    let mut input_file = File::open(input_path)?;
    let mut header = [0u8; 5];
    input_file.read_exact(&mut header)?;

    let reader = BufReader::with_capacity(16 * 1024 * 1024, input_file);
    let decoder = Decoder::new(reader)?;

    match &header {
        b"RZIPF" => {
            let mut output = File::create(output_path)?;
            let mut decoder = decoder;
            copy(&mut decoder, &mut output)?;
        }
        b"RZIPD" => {
            let mut archive = Archive::new(decoder);
            archive.unpack(output_path)?;
        }
        _ => {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid archive format"));
        }
    }
    Ok(())
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