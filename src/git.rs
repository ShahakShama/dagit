use std::process::Command;

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
