use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use zstd::stream::Encoder;
use zstd::stream::Decoder;

pub fn compress(input_path: &str, output_path: &str, compress_level: i32) -> io::Result<()> {

    if compress_level < 1 || compress_level > 22 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid compression level (max 22)!"));
    }

    let input_file = File::open(input_path)?;
    let mut reader = BufReader::with_capacity(4 * 1024 * 1024, input_file);
    let output_file = File::create(output_path)?;
    let mut writer = BufWriter::with_capacity(4 * 1024 * 1024, output_file);

    let mut encoder = Encoder::new(&mut writer, compress_level)?;
    encoder.multithread(num_cpus::get() as u32)?;
    io::copy(&mut reader, &mut encoder)?;
    encoder.finish()?;

    Ok(())
}

pub fn extract(input_path: &str, output_path: &str) -> io::Result<()> {
    let input_file = File::open(input_path)?;
    let reader = BufReader::with_capacity(4 * 1024 * 1024, input_file);
    let output_file = File::create(output_path)?;
    let mut writer = BufWriter::with_capacity(4 * 1024 * 1024, output_file);

    let mut decoder = Decoder::new(reader)?;
    io::copy(&mut decoder, &mut writer)?;

    Ok(())
}