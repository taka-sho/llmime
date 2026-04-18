use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "llmime", about = "LLM-powered Japanese IME")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Convert {
        reading: String,
        #[arg(short = 'n', long, default_value = "5")]
        top_k: usize,
        #[arg(short, long, env = "LLMIME_MODEL")]
        model: Option<std::path::PathBuf>,
        #[arg(short, long, env = "LLMIME_DICT")]
        dict: Option<std::path::PathBuf>,
        #[arg(short, long, default_value = "plain")]
        format: String,
    },
    Version,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Version => {
            println!("llmime {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Convert {
            reading,
            top_k,
            model,
            dict,
            format,
        } => {
            eprintln!("[llmime] converting: {reading} (top_k={top_k})");
            eprintln!("[llmime] model={:?}, dict={:?}", model, dict);
            println!("TODO: N-gram scoring not yet implemented (P1-T4 pending)");
            let _ = format;
            let _ = top_k;
        }
    }
    Ok(())
}

