use std::process::Command;
use std::env;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use crate::dag::Dag;
use crate::serde::read_dag_from_file;

#[derive(Debug, Clone)]
pub enum TestCommand {
    /// Git command with arguments and expected success/failure
    Git { 
        args: Vec<String>, 
        should_succeed: bool 
    },
    /// Dagit command with arguments and expected success/failure
    Dagit { 
        args: Vec<String>, 
        should_succeed: bool 
    },
}

impl TestCommand {
    /// Create a git command that should succeed
    pub fn git_ok(args: &[&str]) -> Self {
        TestCommand::Git {
            args: args.iter().map(|s| s.to_string()).collect(),
            should_succeed: true,
        }
    }
    
    /// Create a git command that should fail
    pub fn git_fail(args: &[&str]) -> Self {
        TestCommand::Git {
            args: args.iter().map(|s| s.to_string()).collect(),
            should_succeed: false,
        }
    }
    
    /// Create a dagit command that should succeed
    pub fn dagit_ok(args: &[&str]) -> Self {
        TestCommand::Dagit {
            args: args.iter().map(|s| s.to_string()).collect(),
            should_succeed: true,
        }
    }
    
    /// Create a dagit command that should fail
    pub fn dagit_fail(args: &[&str]) -> Self {
        TestCommand::Dagit {
            args: args.iter().map(|s| s.to_string()).collect(),
            should_succeed: false,
        }
    }
}

pub struct FlowTest {
    pub commands: Vec<TestCommand>,
    pub expected_dag: Option<Dag>,
}

pub struct FlowTestWithOrigin {
    pub origin_commands: Vec<TestCommand>,  // Commands to run in origin repo
    pub clone_commands: Vec<TestCommand>,   // Commands to run in clone repo
    pub expected_dag: Option<Dag>,
}

impl FlowTest {
    pub fn new() -> Self {
        FlowTest {
            commands: Vec::new(),
            expected_dag: None,
        }
    }
    
    pub fn with_commands(mut self, commands: Vec<TestCommand>) -> Self {
        self.commands = commands;
        self
    }
    
    pub fn with_expected_dag(mut self, dag: Dag) -> Self {
        self.expected_dag = Some(dag);
        self
    }
    
    pub fn add_command(mut self, command: TestCommand) -> Self {
        self.commands.push(command);
        self
    }
}

impl FlowTestWithOrigin {
    pub fn new() -> Self {
        FlowTestWithOrigin {
            origin_commands: Vec::new(),
            clone_commands: Vec::new(),
            expected_dag: None,
        }
    }
    
    pub fn with_commands(mut self, commands: Vec<TestCommand>) -> Self {
        self.origin_commands = commands;
        self
    }
    
    pub fn with_clone_commands(mut self, commands: Vec<TestCommand>) -> Self {
        self.clone_commands = commands;
        self
    }
    
    pub fn with_expected_dag(mut self, dag: Dag) -> Self {
        self.expected_dag = Some(dag);
        self
    }
}

pub fn run_flow_test(test: FlowTest) -> Result<(), String> {
    let temp_dir = TempDir::new().map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let original_dir = env::current_dir().map_err(|e| format!("Failed to get current dir: {}", e))?;
    
    // Change to temp directory
    env::set_current_dir(temp_dir.path()).map_err(|e| format!("Failed to change to temp dir: {}", e))?;
    
    // Setup basic git repository
    setup_git_repo()?;
    
    // Get path to our dagit binary - build it first if needed
    let dagit_path = original_dir.join("target").join("debug").join("dagit");
    if !dagit_path.exists() {
        // Build the binary first
        env::set_current_dir(&original_dir).map_err(|e| format!("Failed to return to original dir: {}", e))?;
        let build_output = Command::new("cargo")
            .args(&["build", "--bin", "dagit"])
            .output()
            .map_err(|e| format!("Failed to build dagit: {}", e))?;
        
        if !build_output.status.success() {
            return Err(format!("Failed to build dagit binary: {}", String::from_utf8_lossy(&build_output.stderr)));
        }
        
        // Return to temp directory
        env::set_current_dir(temp_dir.path()).map_err(|e| format!("Failed to change back to temp dir: {}", e))?;
    }
    
    // Execute each command
    for (i, command) in test.commands.iter().enumerate() {
        let result = match command {
            TestCommand::Git { args, should_succeed } => {
                execute_git_command(args, *should_succeed, i)
            }
            TestCommand::Dagit { args, should_succeed } => {
                execute_dagit_command(&dagit_path, args, *should_succeed, i)
            }
        };
        
        if let Err(e) = result {
            // Always restore directory before returning error
            let _ = env::set_current_dir(&original_dir);
            return Err(e);
        }
    }
    
    // Check expected DAG if provided
    if let Some(expected_dag) = test.expected_dag {
        verify_dag_state(expected_dag)?;
    }
    
    // Restore original directory
    env::set_current_dir(&original_dir).map_err(|e| format!("Failed to restore directory: {}", e))?;
    
    Ok(())
}

pub fn run_flow_test_with_origin(test: FlowTestWithOrigin) -> Result<(), String> {
    let origin_dir = TempDir::new().map_err(|e| format!("Failed to create origin temp dir: {}", e))?;
    let clone_dir = TempDir::new().map_err(|e| format!("Failed to create clone temp dir: {}", e))?;
    let original_dir = env::current_dir().map_err(|e| format!("Failed to get current dir: {}", e))?;
    
    // === Setup Origin Repository ===
    env::set_current_dir(origin_dir.path()).map_err(|e| format!("Failed to change to origin dir: {}", e))?;
    
    // Setup basic git repository in origin
    setup_git_repo()?;
    
    // Execute origin commands
    for (i, command) in test.origin_commands.iter().enumerate() {
        let result = match command {
            TestCommand::Git { args, should_succeed } => {
                execute_git_command(args, *should_succeed, i)
            }
            TestCommand::Dagit { .. } => {
                return Err("Dagit commands not supported in origin repository setup".to_string());
            }
        };
        
        if let Err(e) = result {
            let _ = env::set_current_dir(&original_dir);
            return Err(format!("Origin repo setup failed: {}", e));
        }
    }
    
    let origin_path = origin_dir.path().to_path_buf();
    
    // === Setup Clone Repository ===
    env::set_current_dir(clone_dir.path()).map_err(|e| format!("Failed to change to clone dir: {}", e))?;
    
    // Clone from origin
    run_command("git", &["clone", origin_path.to_str().unwrap(), "."], true, "clone")?;
    
    // Configure clone repo
    run_command("git", &["config", "--local", "user.name", "Test User"], true, "clone setup")?;
    run_command("git", &["config", "--local", "user.email", "test@example.com"], true, "clone setup")?;
    
    // Get path to dagit binary
    let dagit_path = original_dir.join("target").join("debug").join("dagit");
    if !dagit_path.exists() {
        // Build the binary first
        env::set_current_dir(&original_dir).map_err(|e| format!("Failed to return to original dir: {}", e))?;
        let build_output = Command::new("cargo")
            .args(&["build", "--bin", "dagit"])
            .output()
            .map_err(|e| format!("Failed to build dagit: {}", e))?;
        
        if !build_output.status.success() {
            return Err(format!("Failed to build dagit binary: {}", String::from_utf8_lossy(&build_output.stderr)));
        }
        
        // Return to clone directory
        env::set_current_dir(clone_dir.path()).map_err(|e| format!("Failed to change back to clone dir: {}", e))?;
    }
    
    // Execute clone commands
    for (i, command) in test.clone_commands.iter().enumerate() {
        let result = match command {
            TestCommand::Git { args, should_succeed } => {
                execute_git_command(args, *should_succeed, i)
            }
            TestCommand::Dagit { args, should_succeed } => {
                execute_dagit_command(&dagit_path, args, *should_succeed, i)
            }
        };
        
        if let Err(e) = result {
            let _ = env::set_current_dir(&original_dir);
            return Err(format!("Clone repo command failed: {}", e));
        }
    }
    
    // Check expected DAG if provided
    if let Some(expected_dag) = test.expected_dag {
        verify_dag_state(expected_dag)?;
    }
    
    // Restore original directory
    env::set_current_dir(&original_dir).map_err(|e| format!("Failed to restore directory: {}", e))?;
    
    Ok(())
}

fn setup_git_repo() -> Result<(), String> {
    // Initialize git repo with main as default branch
    run_command("git", &["init", "--initial-branch=main"], true, "setup")?;
    
    // Set basic git config (local to this repo)
    run_command("git", &["config", "--local", "user.name", "Test User"], true, "setup")?;
    run_command("git", &["config", "--local", "user.email", "test@example.com"], true, "setup")?;
    
    // Create initial commit only if no commits exist
    let status_output = Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .current_dir(env::current_dir().unwrap())
        .output()
        .map_err(|e| format!("Failed to check git status: {}", e))?;
    
    if !status_output.status.success() {
        // No commits exist, create initial commit
        fs::write("README.md", "# Test Repository").map_err(|e| format!("Failed to create README: {}", e))?;
        run_command("git", &["add", "README.md"], true, "setup")?;
        run_command("git", &["commit", "-m", "Initial commit"], true, "setup")?;
    }
    
    Ok(())
}

/// Get the current git branch name in the current directory
fn get_current_branch_name() -> Result<String, String> {
    let output = Command::new("git")
        .args(&["branch", "--show-current"])
        .current_dir(env::current_dir().unwrap())
        .output()
        .map_err(|e| format!("Failed to get current branch: {}", e))?;
    
    if !output.status.success() {
        return Err(format!("Git command failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    
    let branch_name = String::from_utf8(output.stdout)
        .map_err(|e| format!("Invalid UTF-8 in git output: {}", e))?
        .trim()
        .to_string();
    
    if branch_name.is_empty() {
        return Err("No current branch detected".to_string());
    }
    
    Ok(branch_name)
}

fn execute_git_command(args: &[String], should_succeed: bool, command_index: usize) -> Result<(), String> {
    let args_str: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_command("git", &args_str, should_succeed, &format!("git command {}", command_index))
}

fn execute_dagit_command(dagit_path: &Path, args: &[String], should_succeed: bool, command_index: usize) -> Result<(), String> {
    let output = Command::new(dagit_path)
        .args(args)
        .current_dir(env::current_dir().unwrap())
        .output()
        .map_err(|e| format!("Failed to execute dagit command {}: {}", command_index, e))?;
    
    let success = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Debug: always print stdout for now
    if !stdout.is_empty() {
        println!("Dagit command {} output: {}", command_index, stdout);
    }

    if success != should_succeed {
        return Err(format!(
            "Dagit command {} expected success={}, got success={}\nCommand: dagit {}\nStdout: {}\nStderr: {}",
            command_index, should_succeed, success, args.join(" "), stdout, stderr
        ));
    }

    Ok(())
}

fn run_command(program: &str, args: &[&str], should_succeed: bool, context: &str) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(env::current_dir().unwrap())
        .output()
        .map_err(|e| format!("Failed to execute {} command ({}): {}", program, context, e))?;
    
    let success = output.status.success();
    
    if success != should_succeed {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{} command ({}) expected success={}, got success={}\nCommand: {} {}\nStdout: {}\nStderr: {}",
            program, context, should_succeed, success, program, args.join(" "), stdout, stderr
        ));
    }
    
    Ok(())
}

fn verify_dag_state(expected_dag: Dag) -> Result<(), String> {
    let actual_dag = read_dag_from_file()
        .map_err(|e| format!("Failed to read DAG for verification: {}", e))?;
    
    if actual_dag.len() != expected_dag.len() {
        return Err(format!(
            "DAG length mismatch: expected {}, got {}",
            expected_dag.len(),
            actual_dag.len()
        ));
    }
    
    // Check that all expected branches exist with correct names and dependencies
    for (expected_id, expected_branch) in &expected_dag.branches {
        match actual_dag.branches.get(expected_id) {
            Some(actual_branch) => {
                // Check branch name
                if actual_branch.git_name != expected_branch.git_name {
                    return Err(format!(
                        "Branch {} name mismatch: expected '{}', got '{}'",
                        expected_id.0, expected_branch.git_name, actual_branch.git_name
                    ));
                }
                
                // Check parent relationships
                if actual_branch.parents.len() != expected_branch.parents.len() {
                    return Err(format!(
                        "Branch {} ('{}') parent count mismatch: expected {}, got {}",
                        expected_id.0, expected_branch.git_name, 
                        expected_branch.parents.len(), actual_branch.parents.len()
                    ));
                }
                
                for expected_parent in &expected_branch.parents {
                    if !actual_branch.parents.contains(expected_parent) {
                        let parent_name = expected_dag.branches.get(expected_parent)
                            .map(|b| b.git_name.clone())
                            .unwrap_or_else(|| format!("ID {}", expected_parent.0));
                        return Err(format!(
                            "Branch {} ('{}') missing expected parent: {} ('{}')",
                            expected_id.0, expected_branch.git_name,
                            expected_parent.0, parent_name
                        ));
                    }
                }
                
                // Check child relationships
                if actual_branch.children.len() != expected_branch.children.len() {
                    return Err(format!(
                        "Branch {} ('{}') child count mismatch: expected {}, got {}",
                        expected_id.0, expected_branch.git_name, 
                        expected_branch.children.len(), actual_branch.children.len()
                    ));
                }
                
                for expected_child in &expected_branch.children {
                    if !actual_branch.children.contains(expected_child) {
                        let child_name = expected_dag.branches.get(expected_child)
                            .map(|b| b.git_name.clone())
                            .unwrap_or_else(|| format!("ID {}", expected_child.0));
                        return Err(format!(
                            "Branch {} ('{}') missing expected child: {} ('{}')",
                            expected_id.0, expected_branch.git_name,
                            expected_child.0, child_name
                        ));
                    }
                }
            }
            None => {
                return Err(format!(
                    "Expected branch {} ('{}') not found in actual DAG",
                    expected_id.0, expected_branch.git_name
                ));
            }
        }
    }
    
    // Verify parent-child relationship symmetry in actual DAG
    for (branch_id, branch) in &actual_dag.branches {
        // For each parent, verify this branch is in parent's children list
        for parent_id in &branch.parents {
            if let Some(parent_branch) = actual_dag.branches.get(parent_id) {
                if !parent_branch.children.contains(branch_id) {
                    return Err(format!(
                        "Asymmetric parent-child relationship: Branch {} ('{}') has parent {} ('{}'), but parent doesn't have it as child",
                        branch_id.0, branch.git_name, parent_id.0, parent_branch.git_name
                    ));
                }
            } else {
                return Err(format!(
                    "Branch {} ('{}') references non-existent parent {}",
                    branch_id.0, branch.git_name, parent_id.0
                ));
            }
        }
        
        // For each child, verify this branch is in child's parents list
        for child_id in &branch.children {
            if let Some(child_branch) = actual_dag.branches.get(child_id) {
                if !child_branch.parents.contains(branch_id) {
                    return Err(format!(
                        "Asymmetric parent-child relationship: Branch {} ('{}') has child {} ('{}'), but child doesn't have it as parent",
                        branch_id.0, branch.git_name, child_id.0, child_branch.git_name
                    ));
                }
            } else {
                return Err(format!(
                    "Branch {} ('{}') references non-existent child {}",
                    branch_id.0, branch.git_name, child_id.0
                ));
            }
        }
    }
    
    Ok(())
}
