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
    /// Create a new Branch
    pub fn new(uid: BranchId, git_name: String) -> Self {
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
}

impl Dag {
    /// Create a new empty Dag
    pub fn new() -> Self {
        Dag {
            branches: HashMap::new(),
        }
    }
    
    /// Insert a branch into the DAG
    pub fn insert_branch(&mut self, branch: Branch) {
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
}
