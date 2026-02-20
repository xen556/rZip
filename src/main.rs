use std::io::{self, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use clap::Parser;
mod comp;

fn ask_overwrite(path: &str) -> io::Result<bool> {
    print!("'{}' already exists. Overwrite? [y/N]: ", path);
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn validate_input_path(path: &str, expect_file: bool) -> io::Result<()> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Input path does not exist: '{}'", path),
        ));
    }
    if expect_file && !p.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Expected a file but got a directory: '{}'", path),
        ));
    }
    Ok(())
}

fn parse_profile(profile: &str) -> io::Result<i32> {
    match profile.to_lowercase().as_str() {
        "fastest" => Ok(1),
        "fast"    => Ok(5),
        "balanced"=> Ok(10),
        "high"    => Ok(15),
        "ultra"   => Ok(22),
        other => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Unknown compression profile: '{}'. Valid options: fastest, fast, balanced, high, ultra",
                other
            ),
        )),
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    if args.compress {
        validate_input_path(&args.input_path, false)?;

        let comp_lvl = parse_profile(&args.compress_profile)?;

        let output_path = args.output_path.unwrap_or_else(|| {
            let mut default = args.input_path.clone();
            default.push_str(".zst");
            default
        });

        // Ensure output has .zst extension so it can be detected later
        if !output_path.to_lowercase().ends_with(".zst") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Output file must have a .zst extension, got: '{}'", output_path),
            ));
        }

        if Path::new(&output_path).exists() {
            if !ask_overwrite(&output_path)? {
                println!("Compression cancelled.");
                return Ok(());
            }
        }

        // Ctrl+C handler — remove incomplete output file on interrupt
        let cleanup_path = Arc::new(Mutex::new(Some(output_path.clone())));
        let cleanup_path_ctrlc = cleanup_path.clone();
        ctrlc::set_handler(move || {
            if let Ok(guard) = cleanup_path_ctrlc.lock() {
                if let Some(ref path) = *guard {
                    let _ = std::fs::remove_file(path);
                    eprintln!("\nInterrupted. Incomplete file removed: {}", path);
                }
            }
            std::process::exit(1);
        }).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        comp::compress(&args.input_path, &output_path, comp_lvl)?;

        // Disarm cleanup — compression finished successfully
        if let Ok(mut guard) = cleanup_path.lock() {
            *guard = None;
        }

        println!("File compressed to: {}", output_path);
    }

    else if args.extract {
        validate_input_path(&args.input_path, true)?;

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
                        format!("Unsupported format: '{}'", other),
                    ));
                }
            }
        };

        if Path::new(&output_path).exists() {
            if !ask_overwrite(&output_path)? {
                println!("Extraction cancelled.");
                return Ok(());
            }
        }

        // Ctrl+C handler — remove incomplete output on interrupt
        let cleanup_path = Arc::new(Mutex::new(Some(output_path.clone())));
        let cleanup_path_ctrlc = cleanup_path.clone();
        ctrlc::set_handler(move || {
            if let Ok(guard) = cleanup_path_ctrlc.lock() {
                if let Some(ref path) = *guard {
                    let _ = std::fs::remove_file(path);
                    let _ = std::fs::remove_dir_all(path);
                    eprintln!("\nInterrupted. Incomplete output removed: {}", path);
                }
            }
            std::process::exit(1);
        }).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        comp::extract(&args.input_path, &output_path)?;

        // Disarm cleanup — extraction finished successfully
        if let Ok(mut guard) = cleanup_path.lock() {
            *guard = None;
        }

        println!("File extracted to: {}", output_path);
    }

    else {
        eprintln!("Error: specify either --compress or --extract.");
    }
    Ok(())
}

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(long, conflicts_with = "extract")]
    compress: bool,

    #[arg(long, conflicts_with = "compress")]
    extract: bool,

    input_path: String,

    #[arg(long)]
    output_path: Option<String>,

    #[arg(long, default_value = "balanced", help = "Compression profile [fastest, fast, balanced, high, ultra]")]
    compress_profile: String,
}