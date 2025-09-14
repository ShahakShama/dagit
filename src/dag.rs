use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
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
}
