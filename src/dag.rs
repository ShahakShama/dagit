use std::collections::{HashMap, HashSet, VecDeque};
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
    /// Last failed rebase attempt (target branch name)
    pub last_failed_rebase: Option<String>,
}

impl Branch {
    /// Create a new Branch with a specific ID (used internally by DAG)
    pub(crate) fn with_id(uid: BranchId, git_name: String) -> Self {
        Branch {
            uid,
            parents: Vec::new(),
            children: Vec::new(),
            git_name,
            last_failed_rebase: None,
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

    /// Add a parent relationship by branch IDs (this also adds the corresponding child relationship)
    pub fn add_parent_child_relationship_by_id(&mut self, child_id: BranchId, parent_id: BranchId) -> Result<(), String> {
        // Verify both branches exist
        if !self.branches.contains_key(&child_id) {
            return Err(format!("Child branch with ID {} not found in DAG", child_id.0));
        }
        if !self.branches.contains_key(&parent_id) {
            return Err(format!("Parent branch with ID {} not found in DAG", parent_id.0));
        }
        
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
    
    /// Get branches in topological sort order (parents before children)
    /// Returns an error if there are cycles in the DAG
    pub fn topological_sort(&self) -> Result<Vec<BranchId>, String> {
        let mut in_degree: HashMap<BranchId, usize> = HashMap::new();
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        
        // Initialize in-degree count for all branches
        for (branch_id, branch) in &self.branches {
            in_degree.insert(*branch_id, branch.parents.len());
            if branch.parents.is_empty() {
                queue.push_back(*branch_id);
            }
        }
        
        // Process branches with no incoming edges
        while let Some(current_id) = queue.pop_front() {
            result.push(current_id);
            
            // For each child of current branch
            if let Some(current_branch) = self.branches.get(&current_id) {
                for &child_id in &current_branch.children {
                    if let Some(degree) = in_degree.get_mut(&child_id) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(child_id);
                        }
                    }
                }
            }
        }
        
        // Check for cycles
        if result.len() != self.branches.len() {
            return Err("Cycle detected in DAG - topological sort not possible".to_string());
        }
        
        Ok(result)
    }
    
    /// Get all recursive children of a branch (including the branch itself)
    pub fn get_recursive_children(&self, branch_id: BranchId) -> HashSet<BranchId> {
        let mut visited = HashSet::new();
        let mut stack = vec![branch_id];
        
        while let Some(current_id) = stack.pop() {
            if visited.insert(current_id) {
                if let Some(branch) = self.branches.get(&current_id) {
                    for &child_id in &branch.children {
                        stack.push(child_id);
                    }
                }
            }
        }
        
        visited
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

    #[test]
    fn test_topological_sort_empty_dag() {
        let dag = Dag::new();
        let result = dag.topological_sort().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_topological_sort_single_branch() {
        let mut dag = Dag::new();
        let id = dag.create_branch("main".to_string());
        
        let result = dag.topological_sort().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], id);
    }

    #[test]
    fn test_topological_sort_linear_chain() {
        let mut dag = Dag::new();
        
        // Create a linear chain: main -> feature -> bugfix
        let main_id = dag.create_branch("main".to_string());
        let feature_id = dag.create_branch("feature".to_string());
        let bugfix_id = dag.create_branch("bugfix".to_string());
        
        // Add relationships
        dag.add_parent_child_relationship("feature", "main").unwrap();
        dag.add_parent_child_relationship("bugfix", "feature").unwrap();
        
        let result = dag.topological_sort().unwrap();
        assert_eq!(result.len(), 3);
        
        // Parents should come before children
        let main_pos = result.iter().position(|&id| id == main_id).unwrap();
        let feature_pos = result.iter().position(|&id| id == feature_id).unwrap();
        let bugfix_pos = result.iter().position(|&id| id == bugfix_id).unwrap();
        
        assert!(main_pos < feature_pos);
        assert!(feature_pos < bugfix_pos);
    }

    #[test]
    fn test_topological_sort_complex_dag() {
        let mut dag = Dag::new();
        
        // Create a more complex DAG:
        //     main
        //    /    \
        // feat1   feat2
        //    \    /
        //    merge
        let main_id = dag.create_branch("main".to_string());
        let feat1_id = dag.create_branch("feat1".to_string());
        let feat2_id = dag.create_branch("feat2".to_string());
        let merge_id = dag.create_branch("merge".to_string());
        
        // Add relationships
        dag.add_parent_child_relationship("feat1", "main").unwrap();
        dag.add_parent_child_relationship("feat2", "main").unwrap();
        dag.add_parent_child_relationship("merge", "feat1").unwrap();
        dag.add_parent_child_relationship("merge", "feat2").unwrap();
        
        let result = dag.topological_sort().unwrap();
        assert_eq!(result.len(), 4);
        
        // Check ordering constraints
        let main_pos = result.iter().position(|&id| id == main_id).unwrap();
        let feat1_pos = result.iter().position(|&id| id == feat1_id).unwrap();
        let feat2_pos = result.iter().position(|&id| id == feat2_id).unwrap();
        let merge_pos = result.iter().position(|&id| id == merge_id).unwrap();
        
        // main should come before both feat1 and feat2
        assert!(main_pos < feat1_pos);
        assert!(main_pos < feat2_pos);
        
        // both feat1 and feat2 should come before merge
        assert!(feat1_pos < merge_pos);
        assert!(feat2_pos < merge_pos);
    }

    #[test]
    fn test_topological_sort_cycle_detection() {
        let mut dag = Dag::new();
        
        let a_id = dag.create_branch("a".to_string());
        let _b_id = dag.create_branch("b".to_string());
        let c_id = dag.create_branch("c".to_string());
        
        // Create a cycle: a -> b -> c -> a
        dag.add_parent_child_relationship("b", "a").unwrap();
        dag.add_parent_child_relationship("c", "b").unwrap();
        
        // Manually create the cycle by adding the back edge (this bypasses normal validation)
        if let Some(branch_a) = dag.get_branch_mut(&a_id) {
            branch_a.parents.push(c_id);
        }
        if let Some(branch_c) = dag.get_branch_mut(&c_id) {
            branch_c.children.push(a_id);
        }
        
        let result = dag.topological_sort();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cycle detected"));
    }

    #[test]
    fn test_get_recursive_children_no_children() {
        let mut dag = Dag::new();
        let id = dag.create_branch("main".to_string());
        
        let children = dag.get_recursive_children(id);
        assert_eq!(children.len(), 1);
        assert!(children.contains(&id)); // Should include itself
    }

    #[test]
    fn test_get_recursive_children_direct_children_only() {
        let mut dag = Dag::new();
        
        let main_id = dag.create_branch("main".to_string());
        let feat1_id = dag.create_branch("feat1".to_string());
        let feat2_id = dag.create_branch("feat2".to_string());
        
        dag.add_parent_child_relationship("feat1", "main").unwrap();
        dag.add_parent_child_relationship("feat2", "main").unwrap();
        
        let children = dag.get_recursive_children(main_id);
        assert_eq!(children.len(), 3);
        assert!(children.contains(&main_id));
        assert!(children.contains(&feat1_id));
        assert!(children.contains(&feat2_id));
    }

    #[test]
    fn test_get_recursive_children_deep_hierarchy() {
        let mut dag = Dag::new();
        
        // Create: main -> feat1 -> feat2 -> feat3
        let main_id = dag.create_branch("main".to_string());
        let feat1_id = dag.create_branch("feat1".to_string());
        let feat2_id = dag.create_branch("feat2".to_string());
        let feat3_id = dag.create_branch("feat3".to_string());
        
        dag.add_parent_child_relationship("feat1", "main").unwrap();
        dag.add_parent_child_relationship("feat2", "feat1").unwrap();
        dag.add_parent_child_relationship("feat3", "feat2").unwrap();
        
        // Test from main - should include all
        let children_from_main = dag.get_recursive_children(main_id);
        assert_eq!(children_from_main.len(), 4);
        assert!(children_from_main.contains(&main_id));
        assert!(children_from_main.contains(&feat1_id));
        assert!(children_from_main.contains(&feat2_id));
        assert!(children_from_main.contains(&feat3_id));
        
        // Test from feat1 - should include feat1, feat2, feat3 but not main
        let children_from_feat1 = dag.get_recursive_children(feat1_id);
        assert_eq!(children_from_feat1.len(), 3);
        assert!(children_from_feat1.contains(&feat1_id));
        assert!(children_from_feat1.contains(&feat2_id));
        assert!(children_from_feat1.contains(&feat3_id));
        assert!(!children_from_feat1.contains(&main_id));
        
        // Test from feat3 - should only include itself
        let children_from_feat3 = dag.get_recursive_children(feat3_id);
        assert_eq!(children_from_feat3.len(), 1);
        assert!(children_from_feat3.contains(&feat3_id));
    }

    #[test]
    fn test_get_recursive_children_complex_tree() {
        let mut dag = Dag::new();
        
        // Create a tree structure:
        //       main
        //      /    \
        //   feat1   feat2
        //   /  \      |
        // sub1 sub2  sub3
        //       |
        //     sub4
        
        let main_id = dag.create_branch("main".to_string());
        let feat1_id = dag.create_branch("feat1".to_string());
        let feat2_id = dag.create_branch("feat2".to_string());
        let sub1_id = dag.create_branch("sub1".to_string());
        let sub2_id = dag.create_branch("sub2".to_string());
        let sub3_id = dag.create_branch("sub3".to_string());
        let sub4_id = dag.create_branch("sub4".to_string());
        
        dag.add_parent_child_relationship("feat1", "main").unwrap();
        dag.add_parent_child_relationship("feat2", "main").unwrap();
        dag.add_parent_child_relationship("sub1", "feat1").unwrap();
        dag.add_parent_child_relationship("sub2", "feat1").unwrap();
        dag.add_parent_child_relationship("sub3", "feat2").unwrap();
        dag.add_parent_child_relationship("sub4", "sub2").unwrap();
        
        // Test from main - should include all branches
        let children_from_main = dag.get_recursive_children(main_id);
        assert_eq!(children_from_main.len(), 7);
        
        // Test from feat1 - should include feat1, sub1, sub2, sub4
        let children_from_feat1 = dag.get_recursive_children(feat1_id);
        assert_eq!(children_from_feat1.len(), 4);
        assert!(children_from_feat1.contains(&feat1_id));
        assert!(children_from_feat1.contains(&sub1_id));
        assert!(children_from_feat1.contains(&sub2_id));
        assert!(children_from_feat1.contains(&sub4_id));
        assert!(!children_from_feat1.contains(&feat2_id));
        assert!(!children_from_feat1.contains(&sub3_id));
        
        // Test from feat2 - should include feat2 and sub3
        let children_from_feat2 = dag.get_recursive_children(feat2_id);
        assert_eq!(children_from_feat2.len(), 2);
        assert!(children_from_feat2.contains(&feat2_id));
        assert!(children_from_feat2.contains(&sub3_id));
    }
}

