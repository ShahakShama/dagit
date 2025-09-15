mod dag;
mod serde;
mod git;

#[cfg(test)]
mod flow_tests;

use clap::{Parser, Subcommand};
use git::{get_current_git_branch, find_closest_parent, find_closest_children};
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
    /// Update command placeholder
    Update,
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Track { branch_name } => {
            handle_track_command(branch_name.clone());
        }
        Commands::Update => {
            handle_update_command();
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
    let _branch_id = dag.create_branch(branch_to_track.clone());
    println!("Tracking branch {}", branch_to_track);
    
    // Auto-detect parent and child relationships
    let tracked_branches = dag.get_tracked_branch_names();
    
    // Find the closest parent
    match find_closest_parent(&branch_to_track, &tracked_branches) {
        Ok(Some(parent_name)) => {
            match dag.add_parent_child_relationship(&branch_to_track, &parent_name) {
                Ok(()) => println!("  → Detected parent: {}", parent_name),
                Err(e) => eprintln!("Warning: Failed to add parent relationship: {}", e),
            }
        }
        Ok(None) => println!("  → No parent detected"),
        Err(e) => eprintln!("Warning: Failed to detect parent: {}", e),
    }
    
    // Find the closest children
    match find_closest_children(&branch_to_track, &tracked_branches) {
        Ok(children) => {
            if children.is_empty() {
                println!("  → No children detected");
            } else {
                for child_name in &children {
                    match dag.add_parent_child_relationship(child_name, &branch_to_track) {
                        Ok(()) => println!("  → Detected child: {}", child_name),
                        Err(e) => eprintln!("Warning: Failed to add child relationship: {}", e),
                    }
                }
            }
        }
        Err(e) => eprintln!("Warning: Failed to detect children: {}", e),
    }
    
    // Save updated DAG back to file
    match write_dag_to_file(&dag) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("Failed to write DAG file: {}", e);
            std::process::exit(1);
        }
    }
}

fn handle_update_command() {
    // TODO: Implement update command functionality
    println!("Update command called - implementation pending");
}

