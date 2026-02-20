use std::io;
use clap::Parser;
mod comp;

fn main() -> io::Result<()> {
    let args = Args::parse();
    let comp_lvl = match args.compress_profile.to_lowercase().as_str() {
        "fastest" => 1,
        "fast" => 5,
        "balanced" => 10,
        "high" => 15,
        "ultra" => 22,
        other => return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Unknown profile: {}", other),
        )),
    };

    if args.compress {
        let output_path = args.output_path.unwrap_or_else(|| {
            let mut default = args.input_path.clone();
            default.push_str(".zst");
            default
        });
        comp::compress(&args.input_path, &output_path, comp_lvl)?;
        println!("File compressed to: {}", output_path);
    }

else if args.extract {
    use std::path::Path;

    let output_path = if let Some(path) = args.output_path {
        path
    } else {
        let path = Path::new(&args.input_path);

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "File has no extension"))?;

        match ext.to_lowercase().as_str() {
            "zst" | "zip" | "rar" => {
                path.with_extension("")
                    .to_string_lossy()
                    .to_string()
            }
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Unsupported format: {}", other),
                ));
            }
        }
    };

    comp::extract(&args.input_path, &output_path)?;
    println!("File extracted to: {}", output_path);
}

    else {
        println!("Invalid flag!");
    }
    Ok(())
}

#[derive(Parser)]
struct Args {
    #[arg(long, conflicts_with = "extract")]
    compress: bool,

    #[arg(long, conflicts_with = "compress")]
    extract: bool,

    input_path: String,

    #[arg(long)]
    output_path: Option<String>,

    #[arg(long, default_value = "balanced")]
    compress_profile: String,
}

pub enum OverwriteMode {
    Ask,
    Always,
    Never,
}