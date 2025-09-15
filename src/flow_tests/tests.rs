use super::utils::{FlowTest, TestCommand, run_flow_test};
use crate::dag::Dag;

#[test]
#[serial_test::serial]
fn test_track_new_branch_with_auto_detection() {
    // Build expected DAG - should have main branch (ID 1) detected automatically
    let mut expected_dag = Dag::new();
    expected_dag.create_branch("main".to_string());
    
    let test = FlowTest::new()
        .with_commands(vec![
            // We're already on main branch from setup, so dagit track should detect it
            TestCommand::dagit_ok(&["track"]),
        ])
        .with_expected_dag(expected_dag);
    
    run_flow_test(test).expect("Flow test should succeed");
}

#[test]
#[serial_test::serial]
fn test_track_new_given_branch() {
    // Build expected DAG - should have feature-branch (ID 1)
    let mut expected_dag = Dag::new();
    expected_dag.create_branch("feature-branch".to_string());
    
    let test = FlowTest::new()
        .with_commands(vec![
            // Create and switch to a new branch
            TestCommand::git_ok(&["checkout", "-b", "feature-branch"]),
            // Track it explicitly
            TestCommand::dagit_ok(&["track", "feature-branch"]),
        ])
        .with_expected_dag(expected_dag);
    
    run_flow_test(test).expect("Flow test should succeed");
}

#[test]
#[serial_test::serial]
fn test_track_multiple_branches() {
    // Build expected DAG - should have main (ID 1) and feature (ID 2)
    let mut expected_dag = Dag::new();
    expected_dag.create_branch("main".to_string());
    expected_dag.create_branch("feature".to_string());
    
    let test = FlowTest::new()
        .with_commands(vec![
            // Track main branch (current)
            TestCommand::dagit_ok(&["track"]),
            // Create feature branch
            TestCommand::git_ok(&["checkout", "-b", "feature"]),
            // Track feature branch
            TestCommand::dagit_ok(&["track", "feature"]),
        ])
        .with_expected_dag(expected_dag);
    
    run_flow_test(test).expect("Flow test should succeed");
}

#[test]
#[serial_test::serial]
fn test_track_duplicate_branch_fails_gracefully() {
    // Build expected DAG - should have only main branch (ID 1) since duplicate is ignored
    let mut expected_dag = Dag::new();
    expected_dag.create_branch("main".to_string());
    
    let test = FlowTest::new()
        .with_commands(vec![
            // Track main branch first time (should succeed)
            TestCommand::dagit_ok(&["track"]),
            // Try to track main branch again (should succeed but show it's already tracked)
            TestCommand::dagit_ok(&["track"]),
        ])
        .with_expected_dag(expected_dag);
    
    run_flow_test(test).expect("Flow test should succeed - duplicate tracking should be handled gracefully");
}
