mod dag;
mod serde;
mod git;

#[cfg(test)]
mod flow_tests;

use clap::{Parser, Subcommand};
use git::get_current_git_branch;
use serde::{read_dag_from_file, write_dag_to_file};

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
    
    // Load existing DAG from file (or create new one if file doesn't exist)
    let mut dag = match read_dag_from_file() {
        Ok(dag) => {
            dag
        }
        Err(e) => {
            eprintln!("Failed to read DAG file: {}", e);
            std::process::exit(1);
        }
    };
    
    // Check if branch already exists
    for (_, branch) in &dag.branches {
        if branch.git_name == branch_to_track {
            println!("Branch '{}' is already being tracked", branch_to_track);
            return;
        }
    }
    
    // Create new branch with unique ID
    let branch_id = dag.create_branch(branch_to_track.clone());
    println!("Tracking branch {}", branch_to_track);
    
    // Save updated DAG back to file
    match write_dag_to_file(&dag) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Failed to write DAG file: {}", e);
            std::process::exit(1);
        }
    }
}

