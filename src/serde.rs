use std::fs;
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;
use crate::dag::Dag;

const DAG_FILE_PATH: &str = ".dagit/dag.json";

#[derive(Error, Debug)]
pub enum SerdeError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Read a DAG from the file at .dagit/dag.json
/// Returns an empty DAG if the file doesn't exist or can't be read
pub fn read_dag_from_file() -> Result<Dag, SerdeError> {
    let path = Path::new(DAG_FILE_PATH);
    
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

/// Write a DAG to the file at .dagit/dag.json
/// Creates the .dagit directory if it doesn't exist
/// Overwrites any existing content in the file
pub fn write_dag_to_file(dag: &Dag) -> Result<(), SerdeError> {
    let path = Path::new(DAG_FILE_PATH);
    
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
    use crate::dag::{Branch, BranchId};
    use std::fs;
    
    #[test]
    fn test_write_and_read_dag() {
        // Create a test DAG
        let mut dag = Dag::new();
        let branch1 = Branch::new(BranchId(1), "main".to_string());
        let branch2 = Branch::new(BranchId(2), "feature".to_string());
        
        dag.insert_branch(branch1);
        dag.insert_branch(branch2);
        
        // Write to file
        write_dag_to_file(&dag).expect("Failed to write DAG");
        
        // Read from file
        let read_dag = read_dag_from_file().expect("Failed to read DAG");
        
        // Verify the DAGs are equal
        assert_eq!(dag, read_dag);
        
        // Clean up
        let _ = fs::remove_file(DAG_FILE_PATH);
        let _ = fs::remove_dir(".dagit");
    }
    
    #[test]
    fn test_read_nonexistent_file() {
        // Ensure file doesn't exist
        let _ = fs::remove_file(DAG_FILE_PATH);
        
        // Should return empty DAG
        let dag = read_dag_from_file().expect("Should return empty DAG");
        assert!(dag.is_empty());
    }
}
