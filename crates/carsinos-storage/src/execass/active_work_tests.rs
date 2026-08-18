use super::active_work::{load_active_work_snapshot, load_active_work_status};
use rusqlite::Connection;

#[test]
fn active_work_counts_only_explicit_nonterminal_states_and_never_action_text() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE execass_delegations(delegation_id TEXT NOT NULL, phase TEXT NOT NULL, normalized_original_intent TEXT NOT NULL);
        CREATE TABLE execass_continuations(continuation_id TEXT NOT NULL, status TEXT NOT NULL, action_summary TEXT NOT NULL);
        CREATE TABLE execass_logical_effects(logical_effect_id TEXT NOT NULL, state TEXT NOT NULL, outcome_json TEXT);

        INSERT INTO execass_delegations VALUES
          ('delegation-1','accepted','quiet text'),
          ('delegation-2','recovering','quiet text'),
          ('delegation-3','completed','ACTIVE RUNNING EXECUTING DO NOT STOP'),
          ('delegation-4','partially_completed','active work'),
          ('delegation-5','failed','active work');
        INSERT INTO execass_continuations VALUES
          ('continuation-1','runnable','quiet text'),
          ('continuation-2','executing','quiet text'),
          ('continuation-3','waiting','quiet text'),
          ('continuation-4','uncertain','quiet text'),
          ('continuation-5','terminal','ACTIVE RUNNING EXECUTING DO NOT STOP'),
          ('continuation-6','superseded','active work');
        INSERT INTO execass_logical_effects VALUES
          ('effect-1','planned','{"text":"quiet"}'),
          ('effect-2','claimed','{"text":"quiet"}'),
          ('effect-3','invoking','{"text":"quiet"}'),
          ('effect-4','outcome_unknown','{"text":"quiet"}'),
          ('effect-5','succeeded','{"text":"ACTIVE RUNNING EXECUTING"}'),
          ('effect-6','failed','{"text":"active work"}'),
          ('effect-7','reconciled_absent','{"text":"active work"}'),
          ('effect-8','reconciled_present','{"text":"active work"}');
        "#,
    )
    .unwrap();

    let status = load_active_work_status(&conn).unwrap();
    assert!(status.active);
    assert_eq!(status.nonterminal_delegation_count, 2);
    assert_eq!(status.nonterminal_continuation_count, 4);
    assert_eq!(status.nonterminal_effect_count, 4);
    assert_eq!(status.active_work_count, 10);
}

#[test]
fn terminal_only_rows_are_not_active_work() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE execass_delegations(delegation_id TEXT NOT NULL, phase TEXT NOT NULL);
        CREATE TABLE execass_continuations(continuation_id TEXT NOT NULL, status TEXT NOT NULL);
        CREATE TABLE execass_logical_effects(logical_effect_id TEXT NOT NULL, state TEXT NOT NULL);
        INSERT INTO execass_delegations VALUES ('d1','completed'),('d2','partially_completed'),('d3','failed');
        INSERT INTO execass_continuations VALUES ('c1','terminal'),('c2','superseded');
        INSERT INTO execass_logical_effects VALUES
          ('e1','succeeded'),('e2','failed'),('e3','reconciled_absent'),('e4','reconciled_present');
        "#,
    )
    .unwrap();

    let status = load_active_work_status(&conn).unwrap();
    assert!(!status.active);
    assert_eq!(status.active_work_count, 0);
}

#[test]
fn binding_digest_changes_when_exact_work_changes_even_if_counts_do_not() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE execass_delegations(delegation_id TEXT NOT NULL, phase TEXT NOT NULL);
        CREATE TABLE execass_continuations(continuation_id TEXT NOT NULL, status TEXT NOT NULL);
        CREATE TABLE execass_logical_effects(logical_effect_id TEXT NOT NULL, state TEXT NOT NULL);
        INSERT INTO execass_delegations VALUES ('delegation-a','accepted');
        "#,
    )
    .unwrap();
    let (before_status, before_digest) = load_active_work_snapshot(&conn).unwrap();
    conn.execute_batch(
        "DELETE FROM execass_delegations; INSERT INTO execass_delegations VALUES ('delegation-b','accepted');",
    )
    .unwrap();
    let (after_status, after_digest) = load_active_work_snapshot(&conn).unwrap();
    assert_eq!(before_status, after_status);
    assert_ne!(before_digest, after_digest);
}
