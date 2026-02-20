use std::io;

use clap::Parser;
mod comp;

fn main() -> io::Result<()> {
    let args = Args::parse();

    if args.compress {
        let output_path = args.output_path.unwrap_or_else(|| {
            let mut default = args.input_path.clone();
            default.push_str(".zst");
            default
        });
        comp::compress(&args.input_path, &output_path, args.compress_level)?;
        println!("File compressed to: {}", output_path);
    }

    else if args.extract {
        let output_path = args.output_path.unwrap_or_else(|| {
            let mut default = args.input_path.clone();
            if default.ends_with(".zst") {
                default.truncate(default.len() - 4);
            } else if default.ends_with(".zip") {
                default.truncate(default.len() - 4);
            }
            default
        });
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
    #[arg(long)]
    compress: bool,

    #[arg(long)]
    extract: bool,

    input_path: String,

    #[arg(long)]
    output_path: Option<String>,

    #[arg(long, default_value_t = 5)]
    compress_level: i32,
}
