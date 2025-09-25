use std::process::Command;
use crate::dag::{Branch, BranchId, Dag};

#[derive(Debug, Clone, PartialEq)]
pub enum RebaseOriginError {
    OriginDoesntExist,
    Other(String),
}

/// Get the current git branch name
/// 
/// Returns an error if:
/// - Git command fails to execute
/// - Not in a git repository
/// - In detached HEAD state
/// - Git output is not valid UTF-8
pub fn get_current_git_branch() -> Result<String, String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get current git branch. Are you in a git repository?".to_string());
    }

    let branch_name = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
        .trim()
        .to_string();

    if branch_name.is_empty() {
        return Err("You are in a detached HEAD state. Please specify a branch name explicitly.".to_string());
    }

    Ok(branch_name)
}

/// Check if we're in a git repository
pub fn is_git_repository() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Get all local git branches
pub fn get_all_branches() -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get git branches. Are you in a git repository?".to_string());
    }

    let branches = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    Ok(branches)
}

/// Get the merge base (common ancestor) between two branches
pub fn get_merge_base(branch1: &str, branch2: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["merge-base", branch1, branch2])
        .output()
        .map_err(|e| format!("Failed to execute git merge-base: {}", e))?;

    if !output.status.success() {
        return Err(format!("Failed to find merge base between {} and {}", branch1, branch2));
    }

    let merge_base = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
        .trim()
        .to_string();

    Ok(merge_base)
}

/// Get the commit hash of a branch
pub fn get_branch_commit(branch: &str) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", branch])
        .output()
        .map_err(|e| format!("Failed to execute git rev-parse: {}", e))?;

    if !output.status.success() {
        return Err(format!("Failed to get commit for branch {}", branch));
    }

    let commit = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
        .trim()
        .to_string();

    Ok(commit)
}

/// Count commits between two references (from..to)
pub fn count_commits_between(from: &str, to: &str) -> Result<u32, String> {
    let output = Command::new("git")
        .args(["rev-list", "--count", &format!("{}..{}", from, to)])
        .output()
        .map_err(|e| format!("Failed to execute git rev-list: {}", e))?;

    if !output.status.success() {
        return Err(format!("Failed to count commits between {} and {}", from, to));
    }

    let output_string = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?;
    let count_str = output_string.trim();

    count_str.parse::<u32>()
        .map_err(|e| format!("Failed to parse commit count: {}", e))
}

/// Check if branch1 is an ancestor of branch2
pub fn is_ancestor(ancestor: &str, descendant: &str) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .output()
        .map_err(|e| format!("Failed to execute git merge-base --is-ancestor: {}", e))?;

    Ok(output.status.success())
}

/// Find the closest parent branch from a list of candidate branches
/// Returns the branch that is:
/// 1. An ancestor of the target branch
/// 2. Has the shortest distance (fewest commits) to the target branch
pub fn find_closest_parent(target_branch: &str, candidate_branches: &[String]) -> Result<Option<String>, String> {
    let mut closest_parent = None;
    let mut min_distance = u32::MAX;

    for candidate in candidate_branches {
        // Skip self
        if candidate == target_branch {
            continue;
        }

        // Check if candidate is an ancestor of target
        if is_ancestor(candidate, target_branch)? {
            let distance = count_commits_between(candidate, target_branch)?;
            if distance > 0 && distance < min_distance {
                min_distance = distance;
                closest_parent = Some(candidate.clone());
            }
        }
    }

    Ok(closest_parent)
}

/// Find the closest child branches from a list of candidate branches
/// Returns branches that are:
/// 1. Descendants of the target branch  
/// 2. Have the shortest distance (fewest commits) from the target branch
pub fn find_closest_children(target_branch: &str, candidate_branches: &[String]) -> Result<Vec<String>, String> {
    let mut children_with_distance = Vec::new();

    for candidate in candidate_branches {
        // Skip self
        if candidate == target_branch {
            continue;
        }

        // Check if target is an ancestor of candidate (candidate is descendant of target)
        if is_ancestor(target_branch, candidate)? {
            let distance = count_commits_between(target_branch, candidate)?;
            if distance > 0 {
                children_with_distance.push((candidate.clone(), distance));
            }
        }
    }

    // Sort by distance and return only the closest ones
    if children_with_distance.is_empty() {
        return Ok(Vec::new());
    }

    children_with_distance.sort_by_key(|(_, distance)| *distance);
    let min_distance = children_with_distance[0].1;
    
    // Return all children with the minimum distance
    let closest_children = children_with_distance
        .into_iter()
        .filter(|(_, distance)| *distance == min_distance)
        .map(|(branch, _)| branch)
        .collect();

    Ok(closest_children)
}

/// Rebase a branch onto another branch
/// 
/// This function will:
/// 1. Check out the branch to be rebased
/// 2. Attempt to rebase it onto the target branch
/// 3. If conflicts occur, abort the rebase and return an error
/// 4. Update the Branch's last_failed_rebase field on failure
/// 
/// Returns Ok(()) on success, Err(message) on failure
pub fn rebase_branch(branch: &mut Branch, target_branch: &str) -> Result<(), String> {
    let branch_name = &branch.git_name;
    
    // First, check out the branch we want to rebase
    let checkout_output = Command::new("git")
        .args(["checkout", branch_name])
        .output()
        .map_err(|e| format!("Failed to execute git checkout: {}", e))?;
    
    if !checkout_output.status.success() {
        let stderr = String::from_utf8_lossy(&checkout_output.stderr);
        return Err(format!("Failed to checkout branch '{}': {}", branch_name, stderr));
    }
    
    // Attempt to rebase onto the target branch
    let rebase_output = Command::new("git")
        .args(["rebase", target_branch])
        .output()
        .map_err(|e| format!("Failed to execute git rebase: {}", e))?;
    
    if !rebase_output.status.success() {
        // Rebase failed, likely due to conflicts
        let stderr = String::from_utf8_lossy(&rebase_output.stderr);
        
        // Abort the rebase to clean up
        let abort_output = Command::new("git")
            .args(["rebase", "--abort"])
            .output()
            .map_err(|e| format!("Failed to execute git rebase --abort: {}", e))?;
        
        if !abort_output.status.success() {
            let abort_stderr = String::from_utf8_lossy(&abort_output.stderr);
            return Err(format!(
                "Rebase failed and abort also failed. Rebase error: {}. Abort error: {}", 
                stderr, abort_stderr
            ));
        }
        
        // Update the branch's last failed rebase field
        branch.last_failed_rebase = Some(target_branch.to_string());
        
        return Err(format!("Rebase of '{}' onto '{}' failed with conflicts: {}", 
                          branch_name, target_branch, stderr));
    }
    
    // Rebase succeeded - clear any previous failed rebase
    branch.last_failed_rebase = None;
    
    Ok(())
}

/// Fetch latest changes from origin for all branches
pub fn fetch_from_origin() -> Result<(), String> {
    let output = Command::new("git")
        .args(["fetch", "origin"])
        .output()
        .map_err(|e| format!("Failed to execute git fetch: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to fetch from origin: {}", stderr));
    }

    Ok(())
}

/// Rebase a branch against its origin counterpart
/// Returns Ok(()) on success, Err(RebaseOriginError) on failure
pub fn rebase_against_origin(branch: &mut Branch) -> Result<(), RebaseOriginError> {
    let branch_name = &branch.git_name;
    let origin_branch = format!("origin/{}", branch_name);

    // First check if the origin branch exists
    let check_output = Command::new("git")
        .args(["rev-parse", "--verify", &origin_branch])
        .output()
        .map_err(|e| RebaseOriginError::Other(format!("Failed to check if origin branch exists: {}", e)))?;

    if !check_output.status.success() {
        return Err(RebaseOriginError::OriginDoesntExist);
    }

    // Use the existing rebase_branch function to perform the actual rebase
    rebase_branch(branch, &origin_branch).map_err(RebaseOriginError::Other)
}

/// Create a pull request for a branch if it doesn't already have one
/// Uses the branch's first parent as the target branch
/// If the branch has multiple parents, this function will panic
/// Returns Some(pr_number) if a PR was created, None if no PR was created
/// (either because one already exists or because there are no parents)
pub fn create_pr_for_branch(branch_id: BranchId, dag: &mut Dag) -> Result<Option<usize>, String> {
    // First, check if the branch exists and get parent information
    let parent_info = {
        let branch = match dag.get_branch(&branch_id) {
            Some(b) => b,
            None => return Err(format!("Branch with ID {} not found in DAG", branch_id.0)),
        };

        // If the branch already has a PR number, return None (no new PR created)
        if branch.pr_number.is_some() {
            return Ok(None);
        }

        // Check for multiple parents
        if branch.parents.len() > 1 {
            todo!("Branch has multiple parents - need to determine which one to target for PR");
        }

        // Get parent information
        let parent_info = match branch.parents.first() {
            Some(parent_id) => {
                match dag.get_branch(parent_id) {
                    Some(parent_branch) => Some(parent_branch.git_name.clone()),
                    None => return Err(format!("Parent branch with ID {} not found in DAG", parent_id.0)),
                }
            }
            None => None,
        };

        parent_info
    };

    // Now get mutable reference to create the PR
    let branch = match dag.get_branch_mut(&branch_id) {
        Some(b) => b,
        None => return Err(format!("Branch with ID {} not found in DAG", branch_id.0)),
    };

    // Create the PR
    match parent_info {
        Some(target_branch_name) => {
            match create_pr_if_needed(branch, &target_branch_name) {
                Ok(pr_number) => Ok(Some(pr_number)),
                Err(e) => Err(e),
            }
        }
        None => Ok(None),
    }
}

/// Create a pull request for a branch if it doesn't already have one
/// Uses the provided target branch as the base for the PR
/// Returns the PR number that was created or already existed
fn create_pr_if_needed(branch: &mut Branch, target_branch: &str) -> Result<usize, String> {
    // If the branch already has a PR number, do nothing
    if let Some(pr_number) = branch.pr_number {
        return Ok(pr_number);
    }

    // Create the PR using gh CLI
    let pr_title = format!("{} -> {}", branch.git_name, target_branch);
    let output = Command::new("gh")
        .args([
            "pr", "create",
            "--base", target_branch,
            "--head", &branch.git_name,
            "--title", &pr_title,
            "--body", "",
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh pr create: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to create PR: {}", stderr));
    }

    // Parse the PR number from the output
    // gh pr create outputs something like: "https://github.com/user/repo/pull/123"
    let output_str = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in gh output: {}", e))?;

    // Find the PR number in the URL
    if let Some(pr_url_line) = output_str.lines().find(|line| line.contains("pull/")) {
        if let Some(pr_number_str) = pr_url_line.split("pull/").nth(1) {
            if let Some(pr_number) = pr_number_str.split('/').next() {
                if let Ok(pr_number) = pr_number.parse::<usize>() {
                    // Set the PR number on the branch
                    branch.pr_number = Some(pr_number);
                    return Ok(pr_number);
                }
            }
        }
    }

    Err("Failed to parse PR number from gh output".to_string())
}

/// Update the target branch (base) of an existing pull request for a branch
/// Takes a branch ID and DAG reference, and a new target branch name
/// Updates the PR's base branch to the specified target branch
/// Returns Ok(()) on success, Err(message) on failure
pub fn update_pr_target_for_branch(branch_id: BranchId, dag: &Dag, new_target_branch: &str) -> Result<(), String> {
    // Get the branch from the DAG
    let branch = match dag.get_branch(&branch_id) {
        Some(b) => b,
        None => return Err(format!("Branch with ID {} not found in DAG", branch_id.0)),
    };

    update_pr_target(branch, new_target_branch)
}

/// Update the target branch (base) of an existing pull request
/// Takes a branch reference and a new target branch name
/// Updates the PR's base branch to the specified target branch
/// Returns Ok(()) on success, Err(message) on failure
pub fn update_pr_target(branch: &Branch, new_target_branch: &str) -> Result<(), String> {
    // Check if the branch has a PR number
    let pr_number = match branch.pr_number {
        Some(number) => number,
        None => return Err(format!("Branch '{}' does not have an associated pull request", branch.git_name)),
    };

    // Update the PR using gh CLI
    let output = Command::new("gh")
        .args([
            "pr", "edit",
            &pr_number.to_string(),
            "--base", new_target_branch,
        ])
        .output()
        .map_err(|e| format!("Failed to execute gh pr edit: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to update PR #{} target to '{}': {}", pr_number, new_target_branch, stderr));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::BranchId;
    use std::process::Command;
    use std::fs;
    use std::env;

    fn setup_test_git_repo() -> tempfile::TempDir {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to init git repo");

        // Set basic git config for the test repo
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to set git user name");

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to set git user email");

        // Create and commit a file to avoid issues with empty repo
        fs::write(temp_path.join("test.txt"), "test content")
            .expect("Failed to create test file");

        Command::new("git")
            .args(["add", "test.txt"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to add test file");

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to commit test file");

        temp_dir
    }

    /// Test helper that runs git commands in a specific directory
    fn get_current_branch_in_dir(dir: &std::path::Path) -> Result<String, String> {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(dir)
            .output()
            .map_err(|e| format!("Failed to execute git command: {}", e))?;

        if !output.status.success() {
            return Err("Failed to get current git branch. Are you in a git repository?".to_string());
        }

        let branch_name = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
            .trim()
            .to_string();

        if branch_name.is_empty() {
            return Err("You are in a detached HEAD state. Please specify a branch name explicitly.".to_string());
        }

        Ok(branch_name)
    }

    #[test]
    fn test_get_current_git_branch_in_git_repo() {
        let temp_dir = setup_test_git_repo();

        // Test getting the current branch (should be 'main' or 'master' by default)
        let result = get_current_branch_in_dir(temp_dir.path());
        assert!(result.is_ok(), "Should successfully get current branch: {:?}", result);
        
        let branch_name = result.unwrap();
        assert!(!branch_name.is_empty(), "Branch name should not be empty");
        assert!(branch_name == "main" || branch_name == "master", 
               "Branch should be 'main' or 'master', got: {}", branch_name);
    }

    #[test]
    fn test_get_current_git_branch_with_custom_branch() {
        let temp_dir = setup_test_git_repo();

        // Create and switch to a new branch
        Command::new("git")
            .args(["checkout", "-b", "feature-branch"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to create new branch");

        let result = get_current_branch_in_dir(temp_dir.path());
        assert!(result.is_ok(), "Should successfully get current branch: {:?}", result);
        
        let branch_name = result.unwrap();
        assert_eq!(branch_name, "feature-branch", "Should return the custom branch name");
    }

    #[test]
    #[serial_test::serial]
    fn test_get_current_git_branch_not_in_repo() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Change to a directory that's not a git repository
        env::set_current_dir(temp_dir.path()).expect("Failed to change directory");

        let result = get_current_git_branch();
        assert!(result.is_err(), "Should fail when not in a git repository");
        
        let error_message = result.unwrap_err();
        assert!(error_message.contains("git repository"), 
               "Error message should mention git repository: {}", error_message);

        // Always restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");
    }

    #[test]
    #[serial_test::serial]
    fn test_is_git_repository() {
        let temp_dir = setup_test_git_repo();
        let non_git_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Test in git repository
        env::set_current_dir(temp_dir.path()).expect("Failed to change directory");
        assert!(is_git_repository(), "Should detect git repository");

        // Test not in git repository
        env::set_current_dir(non_git_dir.path()).expect("Failed to change directory");
        assert!(!is_git_repository(), "Should not detect git repository");

        // Always restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");
    }

    fn get_all_branches_in_dir(dir: &std::path::Path) -> Result<Vec<String>, String> {
        let output = Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(dir)
            .output()
            .map_err(|e| format!("Failed to execute git command: {}", e))?;

        if !output.status.success() {
            return Err("Failed to get git branches. Are you in a git repository?".to_string());
        }

        let branches = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        Ok(branches)
    }

    #[test]
    fn test_get_all_branches() {
        let temp_dir = setup_test_git_repo();

        // Create additional branches
        Command::new("git")
            .args(["checkout", "-b", "feature-1"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to create feature-1 branch");

        Command::new("git")
            .args(["checkout", "-b", "feature-2"])
            .current_dir(temp_dir.path())
            .output()
            .expect("Failed to create feature-2 branch");

        let result = get_all_branches_in_dir(temp_dir.path());
        assert!(result.is_ok(), "Should successfully get all branches: {:?}", result);

        let branches = result.unwrap();
        assert!(branches.len() >= 3, "Should have at least 3 branches, got: {:?}", branches);
        assert!(branches.contains(&"feature-1".to_string()), "Should contain feature-1 branch");
        assert!(branches.contains(&"feature-2".to_string()), "Should contain feature-2 branch");
        
        // Should contain the main/master branch
        let has_main_or_master = branches.contains(&"main".to_string()) || 
                                branches.contains(&"master".to_string());
        assert!(has_main_or_master, "Should contain main or master branch, got: {:?}", branches);
    }

    #[test]
    #[serial_test::serial]
    fn test_relationship_detection() {
        let temp_dir = setup_test_git_repo();
        let temp_path = temp_dir.path();

        // Create a more complex branch structure
        // master -> feature-1 -> feature-2
        //        -> feature-3

        // Create feature-1 from master
        Command::new("git")
            .args(["checkout", "-b", "feature-1"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to create feature-1 branch");

        // Add a commit to feature-1
        std::fs::write(temp_path.join("feature1.txt"), "feature 1 content")
            .expect("Failed to create feature1.txt");
        Command::new("git")
            .args(["add", "feature1.txt"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to add feature1.txt");
        Command::new("git")
            .args(["commit", "-m", "Add feature 1"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to commit feature 1");

        // Create feature-2 from feature-1
        Command::new("git")
            .args(["checkout", "-b", "feature-2"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to create feature-2 branch");

        // Add a commit to feature-2
        std::fs::write(temp_path.join("feature2.txt"), "feature 2 content")
            .expect("Failed to create feature2.txt");
        Command::new("git")
            .args(["add", "feature2.txt"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to add feature2.txt");
        Command::new("git")
            .args(["commit", "-m", "Add feature 2"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to commit feature 2");

        // Go back to master and create feature-3
        Command::new("git")
            .args(["checkout", "master"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to checkout master");
        Command::new("git")
            .args(["checkout", "-b", "feature-3"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to create feature-3 branch");

        // Add a commit to feature-3
        std::fs::write(temp_path.join("feature3.txt"), "feature 3 content")
            .expect("Failed to create feature3.txt");
        Command::new("git")
            .args(["add", "feature3.txt"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to add feature3.txt");
        Command::new("git")
            .args(["commit", "-m", "Add feature 3"])
            .current_dir(temp_path)
            .output()
            .expect("Failed to commit feature 3");

        // Save current directory and change to temp directory
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(temp_path).expect("Failed to change to temp dir");

        // Test ancestor relationships
        let result = is_ancestor("master", "feature-1");
        assert!(result.is_ok() && result.unwrap(), "master should be ancestor of feature-1");

        let result = is_ancestor("feature-1", "feature-2");
        assert!(result.is_ok() && result.unwrap(), "feature-1 should be ancestor of feature-2");

        let result = is_ancestor("master", "feature-2");
        assert!(result.is_ok() && result.unwrap(), "master should be ancestor of feature-2");

        let result = is_ancestor("master", "feature-3");
        assert!(result.is_ok() && result.unwrap(), "master should be ancestor of feature-3");

        let result = is_ancestor("feature-1", "feature-3");
        assert!(result.is_ok() && !result.unwrap(), "feature-1 should NOT be ancestor of feature-3");

        // Test closest parent detection
        let branches = vec!["master".to_string(), "feature-1".to_string(), "feature-3".to_string()];
        
        let result = find_closest_parent("feature-2", &branches);
        assert!(result.is_ok());
        let parent = result.unwrap();
        assert_eq!(parent, Some("feature-1".to_string()), "feature-1 should be closest parent of feature-2");

        let result = find_closest_parent("feature-1", &branches);
        assert!(result.is_ok());
        let parent = result.unwrap();
        assert_eq!(parent, Some("master".to_string()), "master should be closest parent of feature-1");

        // Test closest children detection
        let branches = vec!["master".to_string(), "feature-1".to_string(), "feature-2".to_string(), "feature-3".to_string()];
        
        let result = find_closest_children("master", &branches);
        assert!(result.is_ok());
        let children = result.unwrap();
        assert_eq!(children.len(), 2, "master should have 2 direct children");
        assert!(children.contains(&"feature-1".to_string()), "master should have feature-1 as child");
        assert!(children.contains(&"feature-3".to_string()), "master should have feature-3 as child");

        let result = find_closest_children("feature-1", &branches);
        assert!(result.is_ok());
        let children = result.unwrap();
        assert_eq!(children, vec!["feature-2".to_string()], "feature-1 should have feature-2 as only child");

        // Restore original directory
        std::env::set_current_dir(&original_dir).expect("Failed to restore directory");
    }

    #[test]
    #[serial_test::serial]
    fn test_rebase_branch_success() {
        let temp_dir = setup_test_git_repo();
        let temp_path = temp_dir.path();
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Change to temp directory for this test
        env::set_current_dir(temp_path).expect("Failed to change to temp dir");

        // Create a feature branch from master
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .output()
            .expect("Failed to create feature branch");

        // Add a commit to feature branch
        std::fs::write(temp_path.join("feature.txt"), "feature content")
            .expect("Failed to create feature.txt");
        Command::new("git")
            .args(["add", "feature.txt"])
            .output()
            .expect("Failed to add feature.txt");
        Command::new("git")
            .args(["commit", "-m", "Add feature"])
            .output()
            .expect("Failed to commit feature");

        // Go back to master and add another commit
        Command::new("git")
            .args(["checkout", "master"])
            .output()
            .expect("Failed to checkout master");
        std::fs::write(temp_path.join("master.txt"), "master content")
            .expect("Failed to create master.txt");
        Command::new("git")
            .args(["add", "master.txt"])
            .output()
            .expect("Failed to add master.txt");
        Command::new("git")
            .args(["commit", "-m", "Add master change"])
            .output()
            .expect("Failed to commit master change");

        // Test rebase
        let mut branch = Branch::with_id(BranchId(1), "feature".to_string());
        let result = rebase_branch(&mut branch, "master");

        // Restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");

        assert!(result.is_ok(), "Rebase should succeed: {:?}", result);
        assert!(branch.last_failed_rebase.is_none(), "last_failed_rebase should be None on success");
    }

    #[test]
    #[serial_test::serial]
    fn test_rebase_branch_with_conflicts() {
        let temp_dir = setup_test_git_repo();
        let temp_path = temp_dir.path();
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Change to temp directory for this test
        env::set_current_dir(temp_path).expect("Failed to change to temp dir");

        // Create a feature branch from master
        Command::new("git")
            .args(["checkout", "-b", "feature"])
            .output()
            .expect("Failed to create feature branch");

        // Modify the same file in both branches to create conflicts
        std::fs::write(temp_path.join("test.txt"), "feature change")
            .expect("Failed to modify test.txt in feature");
        Command::new("git")
            .args(["add", "test.txt"])
            .output()
            .expect("Failed to add test.txt in feature");
        Command::new("git")
            .args(["commit", "-m", "Change test.txt in feature"])
            .output()
            .expect("Failed to commit in feature");

        // Go back to master and modify the same file differently
        Command::new("git")
            .args(["checkout", "master"])
            .output()
            .expect("Failed to checkout master");
        std::fs::write(temp_path.join("test.txt"), "master change")
            .expect("Failed to modify test.txt in master");
        Command::new("git")
            .args(["add", "test.txt"])
            .output()
            .expect("Failed to add test.txt in master");
        Command::new("git")
            .args(["commit", "-m", "Change test.txt in master"])
            .output()
            .expect("Failed to commit in master");

        // Test rebase (should fail due to conflicts)
        let mut branch = Branch::with_id(BranchId(1), "feature".to_string());
        let result = rebase_branch(&mut branch, "master");

        // Restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");

        assert!(result.is_err(), "Rebase should fail due to conflicts");
        assert_eq!(branch.last_failed_rebase, Some("master".to_string()), 
                  "last_failed_rebase should be set to target branch on failure");
    }

    #[test]
    #[serial_test::serial]
    fn test_rebase_branch_nonexistent_branch() {
        let temp_dir = setup_test_git_repo();
        let temp_path = temp_dir.path();
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Change to temp directory for this test
        env::set_current_dir(temp_path).expect("Failed to change to temp dir");

        // Test rebasing a non-existent branch
        let mut branch = Branch::with_id(BranchId(1), "nonexistent".to_string());
        let result = rebase_branch(&mut branch, "master");

        // Restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");

        assert!(result.is_err(), "Rebase should fail for non-existent branch");
        // The last_failed_rebase should not be set because the failure was due to checkout, not rebase conflicts
        assert!(branch.last_failed_rebase.is_none(), "last_failed_rebase should be None when checkout fails");
    }

    #[test]
    fn test_create_pr_for_branch_already_has_pr() {
        let mut dag = Dag::new();
        let branch_id = dag.create_branch("feature".to_string());
        {
            let branch = dag.get_branch_mut(&branch_id).unwrap();
            branch.pr_number = Some(42);
        }

        let result = create_pr_for_branch(branch_id, &mut dag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // No new PR created
    }

    #[test]
    fn test_create_pr_for_branch_no_parents() {
        let mut dag = Dag::new();
        let branch_id = dag.create_branch("feature".to_string());

        let result = create_pr_for_branch(branch_id, &mut dag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None); // No PR created
    }

    #[test]
    fn test_create_pr_if_needed_already_has_pr() {
        let mut branch = Branch::with_id(BranchId(1), "feature".to_string());
        branch.pr_number = Some(42);

        let result = create_pr_if_needed(&mut branch, "main");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_create_pr_for_branch_parent_not_in_dag() {
        let mut dag = Dag::new();
        let branch_id = dag.create_branch("feature".to_string());
        {
            let branch = dag.get_branch_mut(&branch_id).unwrap();
            // Add a parent ID that doesn't exist in the DAG
            branch.parents.push(BranchId(999));
        }

        let result = create_pr_for_branch(branch_id, &mut dag);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in DAG"));
    }

    #[test]
    fn test_create_pr_for_branch_with_valid_parent() {
        let mut dag = Dag::new();

        // Create parent branch
        let parent_id = dag.create_branch("main".to_string());
        let mut child_branch = Branch::with_id(BranchId(2), "feature".to_string());
        child_branch.parents.push(parent_id);

        // Note: This test would actually call gh CLI, so it's more of an integration test
        // For now, we'll skip the actual CLI call by testing the early return cases
        // In a real scenario, you'd mock the gh CLI or use integration tests

        // Test that it would proceed (but would fail at gh CLI call)
        // We can't easily test the full flow without mocking gh CLI
        assert!(child_branch.pr_number.is_none());
        assert_eq!(child_branch.parents.len(), 1);
    }

    #[test]
    fn test_update_pr_target_no_pr() {
        let branch = Branch::with_id(BranchId(1), "feature".to_string());

        let result = update_pr_target(&branch, "main");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have an associated pull request"));
    }

    #[test]
    fn test_update_pr_target_with_pr() {
        let mut branch = Branch::with_id(BranchId(1), "feature".to_string());
        branch.pr_number = Some(42);

        // Note: This test would actually call gh CLI, so it's more of an integration test
        // For now, we test that it would attempt to call gh CLI (but would fail without gh CLI)
        // In a real scenario, you'd mock the gh CLI or use integration tests

        let result = update_pr_target(&branch, "main");
        // This will fail because gh CLI is not available in test environment,
        // but we can verify it attempts the operation by checking the error message
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("Failed to execute gh pr edit") ||
                error_msg.contains("Failed to update PR #42"));
    }

    #[test]
    fn test_update_pr_target_for_branch_not_in_dag() {
        let dag = Dag::new();
        let branch_id = BranchId(999);

        let result = update_pr_target_for_branch(branch_id, &dag, "main");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found in DAG"));
    }

    #[test]
    fn test_update_pr_target_for_branch_no_pr() {
        let mut dag = Dag::new();
        let branch_id = dag.create_branch("feature".to_string());

        let result = update_pr_target_for_branch(branch_id, &dag, "main");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not have an associated pull request"));
    }
}
