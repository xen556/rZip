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
        let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
        let mut encoder = Encoder::new(writer, compress_level)?;
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
    output.write_all(b"RZIPD")?;
    let writer = BufWriter::with_capacity(32 * 1024 * 1024, output);
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
    let mut header = [0u8; 5];
    input_file.read_exact(&mut header)?;

    let reader = BufReader::with_capacity(32 * 1024 * 1024, input_file);
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

fn append_dir_with_progress(
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
        let relative_path = path.strip_prefix(input_root)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        header.set_path(relative_path)?;
        header.set_size(size);
        header.set_cksum();

        let mut reader = ReadWithProgress { file: &mut f, pb };
        tar_builder.append(&header, &mut reader)?;
    }
    Ok(())
}