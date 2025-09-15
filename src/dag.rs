use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchId(pub usize);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    /// Unique identifier for the branch
    pub uid: BranchId,
    /// UIDs of the parent branches
    pub parents: Vec<BranchId>,
    /// UIDs of the child branches
    pub children: Vec<BranchId>,
    /// Git branch name
    pub git_name: String,
}

impl Branch {
    /// Create a new Branch with a specific ID (used internally by DAG)
    pub(crate) fn with_id(uid: BranchId, git_name: String) -> Self {
        Branch {
            uid,
            parents: Vec::new(),
            children: Vec::new(),
            git_name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dag {
    /// Map from branch UID to Branch
    pub branches: HashMap<BranchId, Branch>,
    /// Next available branch ID (used for generating unique IDs)
    next_branch_id: usize,
}

impl Dag {
    /// Create a new empty Dag
    pub fn new() -> Self {
        Dag {
            branches: HashMap::new(),
            next_branch_id: 1,
        }
    }
    
    /// Create a new branch with an automatically generated unique ID
    pub fn create_branch(&mut self, git_name: String) -> BranchId {
        let branch_id = BranchId(self.next_branch_id);
        self.next_branch_id += 1;
        
        let branch = Branch::with_id(branch_id, git_name);
        self.branches.insert(branch_id, branch);
        
        branch_id
    }
    
    /// Insert a branch into the DAG (for when you already have a branch with an ID)
    pub fn insert_branch(&mut self, branch: Branch) {
        // Update next_branch_id to ensure we don't generate duplicate IDs
        self.next_branch_id = self.next_branch_id.max(branch.uid.0 + 1);
        self.branches.insert(branch.uid, branch);
    }
    
    /// Get a branch by its UID
    pub fn get_branch(&self, uid: &BranchId) -> Option<&Branch> {
        self.branches.get(uid)
    }
    
    /// Get a mutable reference to a branch by its UID
    pub fn get_branch_mut(&mut self, uid: &BranchId) -> Option<&mut Branch> {
        self.branches.get_mut(uid)
    }
    
    /// Remove a branch from the DAG
    pub fn remove_branch(&mut self, uid: &BranchId) -> Option<Branch> {
        self.branches.remove(uid)
    }
    
    /// Check if the DAG contains a branch with the given UID
    pub fn contains_branch(&self, uid: &BranchId) -> bool {
        self.branches.contains_key(uid)
    }
    
    /// Get the number of branches in the DAG
    pub fn len(&self) -> usize {
        self.branches.len()
    }
    
    /// Check if the DAG is empty
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty()
    }
    
    /// Find a branch by its git name
    pub fn find_branch_by_name(&self, git_name: &str) -> Option<&Branch> {
        self.branches.values().find(|branch| branch.git_name == git_name)
    }
    
    /// Get all git branch names that are currently tracked
    pub fn get_tracked_branch_names(&self) -> Vec<String> {
        self.branches.values().map(|branch| branch.git_name.clone()).collect()
    }
    
    /// Add a parent relationship (this also adds the corresponding child relationship)
    pub fn add_parent_child_relationship(&mut self, child_name: &str, parent_name: &str) -> Result<(), String> {
        // Find the child and parent branches
        let child_id = self.find_branch_by_name(child_name)
            .map(|branch| branch.uid)
            .ok_or_else(|| format!("Child branch '{}' not found in DAG", child_name))?;
            
        let parent_id = self.find_branch_by_name(parent_name)
            .map(|branch| branch.uid)
            .ok_or_else(|| format!("Parent branch '{}' not found in DAG", parent_name))?;
        
        // Add parent to child's parents list (if not already present)
        if let Some(child_branch) = self.branches.get_mut(&child_id) {
            if !child_branch.parents.contains(&parent_id) {
                child_branch.parents.push(parent_id);
            }
        }
        
        // Add child to parent's children list (if not already present)
        if let Some(parent_branch) = self.branches.get_mut(&parent_id) {
            if !parent_branch.children.contains(&child_id) {
                parent_branch.children.push(child_id);
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_id_generation() {
        let mut dag = Dag::new();
        
        // Create multiple branches and verify they get unique IDs
        let id1 = dag.create_branch("main".to_string());
        let id2 = dag.create_branch("feature".to_string());
        let id3 = dag.create_branch("bugfix".to_string());
        
        // All IDs should be different
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
        
        // IDs should be sequential starting from 1
        assert_eq!(id1.0, 1);
        assert_eq!(id2.0, 2);
        assert_eq!(id3.0, 3);
        
        // Verify branches are stored with correct IDs and names
        assert_eq!(dag.get_branch(&id1).unwrap().git_name, "main");
        assert_eq!(dag.get_branch(&id2).unwrap().git_name, "feature");
        assert_eq!(dag.get_branch(&id3).unwrap().git_name, "bugfix");
    }
    
    #[test]
    fn test_insert_branch_updates_counter() {
        let mut dag = Dag::new();
        
        // Insert a branch with a high ID
        let high_id_branch = Branch::with_id(BranchId(100), "external".to_string());
        dag.insert_branch(high_id_branch);
        
        // Next created branch should have ID 101, not 1
        let new_id = dag.create_branch("new_branch".to_string());
        assert_eq!(new_id.0, 101);
    }
    
    #[test]
    fn test_counter_persistence_through_serialization() {
        let mut original_dag = Dag::new();
        
        // Create some branches
        original_dag.create_branch("main".to_string());
        original_dag.create_branch("feature".to_string());
        
        // Serialize and deserialize
        let serialized = serde_json::to_string(&original_dag).expect("Failed to serialize");
        let mut restored_dag: Dag = serde_json::from_str(&serialized).expect("Failed to deserialize");
        
        // Create a new branch - should get ID 3, not 1
        let new_id = restored_dag.create_branch("new_after_restore".to_string());
        assert_eq!(new_id.0, 3);
        
        // Verify all branches exist
        assert_eq!(restored_dag.len(), 3);
        assert!(restored_dag.get_branch(&BranchId(1)).is_some());
        assert!(restored_dag.get_branch(&BranchId(2)).is_some());
        assert!(restored_dag.get_branch(&BranchId(3)).is_some());
    }
}

