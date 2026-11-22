use clap::Parser;
use local_text_worker::{scan_root, OfflineQueue, RootConfig};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Parser)]
#[command(name = "elembra-local-text-worker")]
struct Args {
    #[arg(long)]
    instance_id: Uuid,
    #[arg(long)]
    #[arg(required = true)]
    root: Vec<PathBuf>,
    #[arg(long)]
    queue: PathBuf,
    #[arg(long = "include")]
    include: Vec<String>,
    #[arg(long = "exclude")]
    exclude: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut queue = OfflineQueue::open(&args.queue)?;
    let mut queued = 0;
    let mut excluded = 0;
    for root in args.root {
        let scan = scan_root(&RootConfig {
            path: root,
            include: args.include.clone(),
            exclude: args.exclude.clone(),
        })?;
        for observation in &scan.observations {
            queue.enqueue(args.instance_id, observation)?;
        }
        queued += scan.observations.len();
        excluded += scan.excluded;
    }
    println!(
        "Queued {} eligible text observations; {} entries excluded",
        queued, excluded
    );
    Ok(())
}
