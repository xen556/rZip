use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use zstd::stream::Encoder;
use zstd::stream::Decoder;
use tar::{Builder, Archive};
use std::io::copy;

pub fn compress(input_path: &str, output_path: &str, compress_level: i32) -> io::Result<()> {

    if compress_level < 1 || compress_level > 22 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid compression level (max 22)!"));
    }

    let metadata = fs::metadata(input_path)?;

    if metadata.is_file() {
        let mut input = File::open(input_path)?;
        let mut output = File::create(output_path)?;
        output.write_all(b"RZIPF")?;
        let mut encoder = Encoder::new(output, compress_level)?;
        encoder.multithread(num_cpus::get() as u32)?;
        copy(&mut input, &mut encoder)?;
        encoder.finish()?;
        return Ok(());
    }

    let mut output = File::create(output_path)?;
    output.write_all(b"RZIPD")?;
    let writer = BufWriter::with_capacity(16 * 1024 * 1024, output);
    let mut encoder = Encoder::new(writer, compress_level)?;
    encoder.multithread(num_cpus::get() as u32)?;

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