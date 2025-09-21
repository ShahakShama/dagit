mod dag;
mod serde;
mod git;

#[cfg(test)]
mod flow_tests;

use clap::{Parser, Subcommand};
use git::{get_current_git_branch, find_closest_parent, find_closest_children, fetch_from_origin, rebase_against_origin, rebase_branch};
use serde::{read_dag_from_file, write_dag_to_file};
use std::collections::HashSet;

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
    /// Update all tracked branches by rebasing against origin and parents
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

fn update_branch(
    dag: &mut dag::Dag,
    branch_id: dag::BranchId,
    failed_branches: &mut HashSet<dag::BranchId>,
    skipped_branches: &mut HashSet<dag::BranchId>,
) {
    // Get branch info first to avoid borrowing conflicts
    let (branch_name, branch_parents, should_skip) = {
        let branch = match dag.get_branch(&branch_id) {
            Some(b) => b,
            None => return, // Should not happen
        };

        let branch_name = branch.git_name.clone();
        let branch_parents = branch.parents.clone();

        // Check if this branch should be skipped due to parent failure
        let should_skip = branch.parents.iter().any(|parent_id| failed_branches.contains(parent_id));

        (branch_name, branch_parents, should_skip)
    };

    if should_skip {
        println!("  Skipping '{}' (parent branch failed rebase)", branch_name);
        skipped_branches.insert(branch_id);
        return;
    }

    println!("  Processing branch: {}", branch_name);

    // Get mutable reference to the branch for rebasing
    let mut branch_failed = false;

    // Step 1: Rebase against origin
    if let Some(branch_mut) = dag.get_branch_mut(&branch_id) {
        print!("    Rebasing against origin... ");
        match rebase_against_origin(branch_mut) {
            Ok(()) => println!("✓ Success"),
            Err(e) => {
                println!("✗ Failed: {}", e);
                branch_failed = true;
            }
        }
    }

    // Step 2: Rebase against first parent (if no failure so far and has parents)
    if !branch_failed && !branch_parents.is_empty() {
        if branch_parents.len() > 1 {
            todo!("Handle multiple parents - for now only supporting single parent");
        }

        let first_parent_id = branch_parents[0];
        let parent_name = {
            if let Some(parent_branch) = dag.get_branch(&first_parent_id) {
                parent_branch.git_name.clone()
            } else {
                eprintln!("    Error: Parent branch not found in DAG");
                return;
            }
        };

        if let Some(branch_mut) = dag.get_branch_mut(&branch_id) {
            print!("    Rebasing against parent '{}'... ", parent_name);

            match rebase_branch(branch_mut, &parent_name) {
                Ok(()) => println!("✓ Success"),
                Err(e) => {
                    println!("✗ Failed: {}", e);
                    branch_failed = true;
                }
            }
        }
    } else if !branch_failed {
        println!("    No parent to rebase against");
    }

    // If any rebase failed, mark this branch as failed
    if branch_failed {
        failed_branches.insert(branch_id);
        println!("    Branch '{}' failed - its children will be skipped", branch_name);
    }
}

fn handle_update_command() {
    println!("Starting update process...");
    
    // Load existing DAG from file
    let mut dag = match read_dag_from_file() {
        Ok(dag) => dag,
        Err(e) => {
            eprintln!("Failed to read DAG file: {}", e);
            std::process::exit(1);
        }
    };
    
    if dag.is_empty() {
        println!("No branches are being tracked. Use 'dagit track' to add branches first.");
        return;
    }
    
    // Fetch latest changes from origin
    println!("Fetching latest changes from origin...");
    if let Err(e) = fetch_from_origin() {
        eprintln!("Error: Failed to fetch from origin: {}", e);
        std::process::exit(1);
    }
    
    // Get branches in topological sort order
    let sorted_branch_ids = match dag.topological_sort() {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };
    
    // Track branches that failed rebase (and their children should be skipped)
    let mut failed_branches: HashSet<dag::BranchId> = HashSet::new();
    let mut skipped_branches: HashSet<dag::BranchId> = HashSet::new();
    
    println!("Processing {} branches in topological order...", sorted_branch_ids.len());
    
    // Process each branch in topological order
    for &branch_id in &sorted_branch_ids {
        update_branch(&mut dag, branch_id, &mut failed_branches, &mut skipped_branches);
    }
    
    // Save updated DAG back to file (to persist any last_failed_rebase updates)
    match write_dag_to_file(&dag) {
        Ok(()) => {},
        Err(e) => {
            eprintln!("Failed to write DAG file: {}", e);
            std::process::exit(1);
        }
    }
    
    // Summary
    let total_branches = sorted_branch_ids.len();
    let failed_count = failed_branches.len();
    let skipped_count = skipped_branches.len();
    let success_count = total_branches - failed_count - skipped_count;
    
    println!();
    println!("Update completed:");
    println!("  ✓ {} branches successfully updated", success_count);
    println!("  ✗ {} branches failed", failed_count);
    println!("  - {} branches skipped (due to parent failures)", skipped_count);
    
    if failed_count > 0 || skipped_count > 0 {
        println!();
        println!("Some branches had issues. Check the output above for details.");
    }
}

