use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use crate::dag::Dag;
use crate::git;

#[derive(Error, Debug)]
pub enum SerdeError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Git error: {0}")]
    Git(String),
}

/// Get the path to the DAG file, relative to the git repository root
/// This ensures the DAG file is shared across all worktrees
fn get_dag_file_path() -> Result<PathBuf, SerdeError> {
    let repo_root = git::get_git_repo_root().map_err(SerdeError::Git)?;
    let dag_path = Path::new(&repo_root).join(".dagit").join("dag.json");
    Ok(dag_path)
}

/// Read a DAG from the file at .dagit/dag.json in the git repository root
/// Returns an empty DAG if the file doesn't exist or can't be read
pub fn read_dag_from_file() -> Result<Dag, SerdeError> {
    let path = get_dag_file_path()?;

    if !path.exists() {
        // Return empty DAG if file doesn't exist
        return Ok(Dag::new());
    }

    let content = fs::read_to_string(path)?;

    if content.trim().is_empty() {
        // Return empty DAG if file is empty
        return Ok(Dag::new());
    }

    let dag: Dag = serde_json::from_str(&content)?;

    Ok(dag)
}

/// Write a DAG to the file at .dagit/dag.json in the git repository root
/// Creates the .dagit directory if it doesn't exist
/// Overwrites any existing content in the file
pub fn write_dag_to_file(dag: &Dag) -> Result<(), SerdeError> {
    let path = get_dag_file_path()?;

    // Create the .dagit directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Serialize the DAG to JSON with pretty formatting
    let json = serde_json::to_string_pretty(dag)?;

    // Write to file, creating it if it doesn't exist or overwriting if it does
    let mut file = fs::File::create(path)?;
    file.write_all(json.as_bytes())?;
    file.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::Dag;
    use std::fs;
    use std::env;
    use std::path::Path;
    
    /// Helper function to create isolated test functions that work in a temp directory with git initialized
    fn with_temp_dir<F>(test_fn: F)
    where
        F: FnOnce() -> ()
    {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Change to temp directory for the test
        env::set_current_dir(temp_dir.path()).expect("Failed to change to temp dir");

        // Initialize git repository for the test
        std::process::Command::new("git")
            .args(["init"])
            .output()
            .expect("Failed to init git repo");

        // Set basic git config for the test repo
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("Failed to set git user name");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .output()
            .expect("Failed to set git user email");

        // Create and commit a file to avoid issues with empty repo
        fs::write("test.txt", "test content")
            .expect("Failed to create test file");

        std::process::Command::new("git")
            .args(["add", "test.txt"])
            .output()
            .expect("Failed to add test file");

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .output()
            .expect("Failed to commit test file");

        // Run the test function
        test_fn();

        // Always restore original directory
        env::set_current_dir(&original_dir).expect("Failed to restore directory");
    }
    
    #[test]
    #[serial_test::serial]
    fn test_write_and_read_dag() {
        with_temp_dir(|| {
            // Create a test DAG
            let mut dag = Dag::new();
            dag.create_branch("main".to_string());
            dag.create_branch("feature".to_string());
            
            // Write to file
            write_dag_to_file(&dag).expect("Failed to write DAG");
            
            // Verify file was created
            let dag_path = get_dag_file_path().expect("Failed to get DAG file path");
            assert!(dag_path.exists(), "DAG file should exist after write");
            
            // Read from file
            let read_dag = read_dag_from_file().expect("Failed to read DAG");
            
            // Verify the DAGs are equal
            assert_eq!(dag, read_dag);
            
            // Clean up is automatic when temp dir is dropped
        });
    }
    
    #[test]
    #[serial_test::serial]
    fn test_read_nonexistent_file() {
        with_temp_dir(|| {
            // In a fresh temp directory, the file shouldn't exist
            let dag_path = get_dag_file_path().expect("Failed to get DAG file path");
            assert!(!dag_path.exists(), "DAG file should not exist initially");

            // Should return empty DAG
            let dag = read_dag_from_file().expect("Should return empty DAG");
            assert!(dag.is_empty());
        });
    }
    
    #[test]
    #[serial_test::serial]
    fn test_read_empty_file() {
        with_temp_dir(|| {
            // Create empty .dagit directory and file
            let dag_path = get_dag_file_path().expect("Failed to get DAG file path");
            if let Some(parent) = dag_path.parent() {
                fs::create_dir_all(parent).expect("Failed to create .dagit directory");
            }
            fs::write(&dag_path, "").expect("Failed to create empty file");

            // Should return empty DAG
            let dag = read_dag_from_file().expect("Should return empty DAG for empty file");
            assert!(dag.is_empty());
        });
    }
}
