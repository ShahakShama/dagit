mod dag;
mod serde;
mod git;

use clap::{Parser, Subcommand};
use git::get_current_git_branch;

#[derive(Parser)]
#[command(name = "dagit")]
#[command(about = "A DAG-based git branch management tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Track a git branch in the DAG
    Track {
        /// Name of the branch to track (defaults to current branch)
        branch_name: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Track { branch_name } => {
            handle_track_command(branch_name.clone());
        }
    }
}

fn handle_track_command(branch_name: Option<String>) {
    // Get the branch name to track
    let branch_to_track = match branch_name {
        Some(name) => name,
        None => match get_current_git_branch() {
            Ok(current_branch) => current_branch,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };
    
    println!("Tracking branch: {}", branch_to_track);
    
    // TODO: Implement the actual tracking logic:
    // 1. Load existing DAG from .dagit/dag.json
    // 2. Create new Branch with unique ID
    // 3. Add branch to DAG
    // 4. Save updated DAG back to file
    // 5. Update git branch tracking if needed
    
    println!("Track command placeholder - implementation coming soon!");
}

