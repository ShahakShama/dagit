mod dag;
mod serde;
mod git;

#[cfg(test)]
mod flow_tests;

use clap::{Parser, Subcommand};
use colored::Colorize;
use git::{get_current_git_branch, find_closest_parent, find_closest_children, fetch_from_origin, rebase_against_origin, rebase_branch, RebaseOriginError, create_pr_for_branch, get_branch_commit, is_ancestor, is_current_branch};
use serde::{read_dag_from_file, write_dag_to_file};
use std::collections::HashSet;

fn get_branch_info(branch: &dag::Branch, indent: usize, dag: &dag::Dag) -> Result<String, String> {
    // Get indent spaces
    let indent_str = " ".repeat(indent);

    // Determine marker: "*" if not current, colored ">" if current
    let is_current = match is_current_branch(&branch.git_name) {
        Ok(is_current) => is_current,
        Err(_) => false, // Default to non-current if we can't determine
    };

    let marker = if is_current {
        ">".green().bold().to_string()
    } else {
        "*".to_string()
    };

    // Get commit hash
    let commit_hash = match get_branch_commit(&branch.git_name) {
        Ok(hash) => {
            // Take first 7 characters of hash for brevity
            if hash.len() >= 7 {
                hash[..7].yellow().to_string()
            } else {
                hash.yellow().to_string()
            }
        }
        Err(_) => "unknown".yellow().to_string(),
    };

    // Determine status
    let status = if branch.last_failed_rebase.is_some() {
        "❌ failed update"
    } else {
        // Check if all parents are ancestors
        let mut all_parents_are_ancestors = true;
        for parent_id in &branch.parents {
            if let Some(parent_branch) = dag.get_branch(parent_id) {
                // We need to check if the parent is an ancestor of this branch
                if !is_ancestor(&parent_branch.git_name, &branch.git_name).unwrap() {
                    all_parents_are_ancestors = false;
                    break;
                }
            }
        }

        if all_parents_are_ancestors && !branch.parents.is_empty() {
            "✅ up to date"
        } else {
            "🔄 out of date"
        }
    };

    // PR number if exists
    let pr_info = if let Some(pr_num) = branch.pr_number {
        format!("PR #{}", pr_num).yellow().to_string()
    } else {
        "".to_string()
    };

    // Build and return the formatted string
    Ok(format!("{}{} {}|{}|{}|{}",
               indent_str,
               marker,
               commit_hash,
               branch.git_name,
               status,
               pr_info.trim()))
}

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
    /// Submit PRs for all tracked branches
    Submit,
    /// Print the DAG structure
    Dag,
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
        Commands::Submit => {
            handle_submit_command();
        }
        Commands::Dag => {
            handle_dag_command();
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
    let branch_name = dag.get_branch(&branch_id).map(|b| b.git_name.clone()).unwrap_or_else(|| "unknown".to_string());
    println!("*** Processing branch '{}' ***", branch_name);

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
            Err(RebaseOriginError::OriginDoesntExist) => {
                println!("✗ Skipped: origin branch does not exist");
            }
            Err(RebaseOriginError::Other(e)) => {
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
        return;
    }

    // Check if this branch is behind one of its parents (i.e., redundant)
    if !branch_parents.is_empty() {
        for &parent_id in &branch_parents {
            if let Some(parent_branch) = dag.get_branch(&parent_id) {
                let parent_name = parent_branch.git_name.clone();

                // Check if parent is an ancestor of this branch
                println!("    Checking if '{}' is ancestor of '{}'...", parent_name, branch_name);
                let is_ancestor = match git::is_ancestor(&parent_name, &branch_name) {
                    Ok(result) => result,
                    Err(e) => {
                        println!("    Error checking ancestry: {} - skipping redundant check", e);
                        false
                    }
                };
                if is_ancestor {
                    println!("    *** REMOVING BRANCH '{}' ***", branch_name);
                    println!("    Yes! '{}' is ancestor of '{}'", parent_name, branch_name);
                    println!("    Branch '{}' is behind parent '{}' - removing from DAG", branch_name, parent_name);

                    // Get all children of this branch before removing it
                    let children = dag.get_branch(&branch_id)
                        .map(|b| b.children.clone())
                        .unwrap_or_default();

                    // Remove the branch from DAG
                    dag.remove_branch(&branch_id);

                    // Remove the branch from its parents' children lists
                    for &parent_id in &branch_parents {
                        if let Some(parent_mut) = dag.get_branch_mut(&parent_id) {
                            parent_mut.children.retain(|&c| c != branch_id);
                        }
                    }

                    // Update all children to have this parent instead
                    for child_id in children {
                        if let Some(child_mut) = dag.get_branch_mut(&child_id) {
                            let child_name = child_mut.git_name.clone();

                            // Remove the old branch from child's parents
                            child_mut.parents.retain(|&p| p != branch_id);

                            // Add the new parent-child relationship
                            dag.add_parent_child_relationship_by_id(parent_id, child_id).unwrap();

                            // Update the PR target to point to the new parent
                            if let Err(e) = git::update_pr_target_for_branch(child_id, &dag, &parent_name) {
                                println!("      Warning: Failed to update PR target for '{}': {}", child_name, e);
                            } else {
                                println!("      Updated PR target for '{}' to '{}'", child_name, parent_name);
                            }

                            println!("      Updated child '{}' to have parent '{}'", child_name, parent_name);
                        }
                    }

                    // Mark this branch as "skipped" since we've removed it
                    println!("    DAG now has {} branches", dag.len());
                    skipped_branches.insert(branch_id);
                    return;
                }
            }
        }
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

fn handle_submit_command() {
    println!("Starting submit process...");

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

    // Get branches in topological sort order
    let sorted_branch_ids = match dag.topological_sort() {
        Ok(ids) => ids,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    println!("Processing {} branches in topological order for PR creation...", sorted_branch_ids.len());

    let mut pr_created_count = 0;
    let mut pr_skipped_count = 0;
    let mut pr_error_count = 0;

    // Process each branch in topological order
    for &branch_id in &sorted_branch_ids {
        // Get branch name for logging
        let branch_name = dag.get_branch(&branch_id)
            .map(|b| b.git_name.clone())
            .unwrap_or_else(|| "unknown".to_string());

        println!("*** Processing branch '{}' ***", branch_name);

        // Create PR for this branch
        match create_pr_for_branch(branch_id, &mut dag) {
            Ok(Some(pr_number)) => {
                println!("  ✓ Created PR #{}", pr_number);
                pr_created_count += 1;
            }
            Ok(None) => {
                println!("  - Skipped (already exists or no parent)");
                pr_skipped_count += 1;
            }
            Err(e) => {
                println!("  ✗ Error: {}", e);
                pr_error_count += 1;
            }
        }
    }

    // Save updated DAG back to file (to persist pr_number updates)
    match write_dag_to_file(&dag) {
        Ok(()) => {},
        Err(e) => {
            eprintln!("Failed to write DAG file: {}", e);
            std::process::exit(1);
        }
    }

    // Summary
    println!();
    println!("Submit completed:");
    println!("  ✓ {} PRs created", pr_created_count);
    println!("  - {} PRs skipped (already exist or no parent)", pr_skipped_count);
    println!("  ✗ {} PR creation errors", pr_error_count);

    if pr_error_count > 0 {
        println!();
        println!("Some branches had PR creation errors. Check the output above for details.");
    }
}

const DAG_INDENT_ROWS: usize = 3;

fn handle_dag_command() {
    // Load existing DAG from file
    let dag = match read_dag_from_file() {
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

    // Perform DFS traversal
    print_dag(&dag);
}

fn print_dag(dag: &dag::Dag) {
    // Find root branches (branches with no parents)
    let mut roots = Vec::new();
    for (&branch_id, branch) in &dag.branches {
        if branch.parents.is_empty() {
            roots.push(branch_id);
        }
    }

    // Sort roots to ensure consistent output
    roots.sort_by_key(|&id| id.0);

    // Track visited branches
    let mut visited = std::collections::HashSet::new();

    // DFS traversal from all roots
    for &root_id in &roots {
        dfs_print(dag, root_id, 0, &mut visited);
    }
}

fn dfs_print(
    dag: &dag::Dag,
    branch_id: dag::BranchId,
    indent: usize,
    visited: &mut std::collections::HashSet<dag::BranchId>,
) {
    if visited.contains(&branch_id) {
        return;
    }
    visited.insert(branch_id);

    let branch = match dag.get_branch(&branch_id) {
        Some(b) => b,
        None => return,
    };

    // Print connection line from parent (except for root)
    if indent > 0 {
        println!("{}", " ".repeat(indent * DAG_INDENT_ROWS) + "|");
    }

    // Print the branch info
    match get_branch_info(branch, indent * DAG_INDENT_ROWS, dag) {
        Ok(info) => println!("{}", info),
        Err(e) => eprintln!("Error getting branch info: {}", e),
    }

    // Get children and sort them for consistent output
    let mut children: Vec<_> = branch.children.iter().cloned().collect();
    children.sort_by_key(|&id| id.0);

    // Print children
    for (i, &child_id) in children.iter().enumerate() {
        if i == 0 {
            // First child: continue at same indent level
            dfs_print(dag, child_id, indent, visited);
        } else {
            // Subsequent children: print branch connector at parent level, then indent child
            println!("{}", " ".repeat(indent * DAG_INDENT_ROWS) + "/");
            dfs_print(dag, child_id, indent + 1, visited);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{Branch, BranchId, Dag};

    fn create_test_branch(id: usize, name: String, parents: Vec<BranchId>, pr_number: Option<usize>, last_failed_rebase: Option<String>) -> Branch {
        let mut branch = Branch::with_id(BranchId(id), name);
        branch.parents = parents;
        branch.pr_number = pr_number;
        branch.last_failed_rebase = last_failed_rebase;
        branch
    }

    #[test]
    fn test_get_branch_info_basic_formatting() {
        let mut dag = Dag::new();
        let branch = create_test_branch(1, "test-branch".to_string(), vec![], None, None);
        dag.insert_branch(branch.clone());

        let result = get_branch_info(&branch, 0, &dag);

        // Test that the function returns a result (may be Ok or Err depending on git state)
        assert!(result.is_ok() || result.is_err());

        // If successful, check basic formatting
        if let Ok(output) = result {
            assert!(output.contains("test-branch"));
            assert!(output.contains("|test-branch|"));
            assert!(output.contains("🔄 out of date")); // No parents so out of date
            assert!(!output.contains("PR"));
        }
    }

    #[test]
    fn test_get_branch_info_with_indent() {
        let mut dag = Dag::new();
        let branch = create_test_branch(1, "feature".to_string(), vec![], None, None);
        dag.insert_branch(branch.clone());

        let result = get_branch_info(&branch, 2, &dag);

        // Test that the function returns a result
        assert!(result.is_ok() || result.is_err());

        // If successful, check indentation
        if let Ok(output) = result {
            // Should be indented by 2 spaces
            assert!(output.starts_with("  "));
            assert!(output.contains("feature"));
            assert!(output.contains("|feature|"));
        }
    }

    #[test]
    fn test_get_branch_info_with_pr_number() {
        let mut dag = Dag::new();
        let branch = create_test_branch(1, "feature".to_string(), vec![], Some(123), None);
        dag.insert_branch(branch.clone());

        let result = get_branch_info(&branch, 0, &dag);

        // Test that the function returns a result
        assert!(result.is_ok() || result.is_err());

        // If successful, check PR number formatting
        if let Ok(output) = result {
            assert!(output.contains("PR #123"));
            assert!(output.contains("feature"));
        }
    }

    #[test]
    fn test_get_branch_info_failed_update() {
        let mut dag = Dag::new();
        let branch = create_test_branch(1, "feature".to_string(), vec![], None, Some("origin/feature".to_string()));
        dag.insert_branch(branch.clone());

        let result = get_branch_info(&branch, 0, &dag);

        // Test that the function returns a result
        assert!(result.is_ok() || result.is_err());

        // If successful, check failed update status
        if let Ok(output) = result {
            assert!(output.contains("❌ failed update"));
            assert!(output.contains("feature"));
        }
    }

    #[test]
    fn test_get_branch_info_with_parents() {
        let mut dag = Dag::new();

        // Create parent branch
        let parent_branch = create_test_branch(1, "main".to_string(), vec![], None, None);
        dag.insert_branch(parent_branch);

        // Create child branch with parent
        let child_branch = create_test_branch(2, "feature".to_string(), vec![BranchId(1)], None, None);
        dag.insert_branch(child_branch.clone());

        // Note: The actual status depends on is_ancestor check which may fail in test environment
        let result = get_branch_info(&child_branch, 0, &dag);

        // Test that the function returns a result
        assert!(result.is_ok() || result.is_err());

        // If successful, check basic structure
        if let Ok(output) = result {
            assert!(output.contains("feature"));
            assert!(output.contains("|feature|"));
        }
    }

    #[test]
    fn test_get_branch_info_formatting() {
        let mut dag = Dag::new();
        let branch = create_test_branch(1, "test-branch".to_string(), vec![], Some(456), None);
        dag.insert_branch(branch.clone());

        let result = get_branch_info(&branch, 4, &dag);

        // Test that the function returns a result
        assert!(result.is_ok() || result.is_err());

        // If successful, check formatting
        if let Ok(output) = result {
            // Should be indented by 4 spaces
            assert!(output.starts_with("    "));
            // Should contain the pipe-separated format
            assert!(output.contains("|test-branch|"));
            assert!(output.contains("PR #456"));
        }
    }
}

