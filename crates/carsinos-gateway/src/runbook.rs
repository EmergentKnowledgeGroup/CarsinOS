use super::*;

const RUNBOOK_TEMPLATE_VERSION: &str = "mc-runbook-v1";
const RUNBOOK_LIST_DEFAULT_LIMIT: u32 = 50;
const RUNBOOK_LIST_MAX_LIMIT: u32 = 200;
const RUNBOOK_SESSION_SCAN_LIMIT: u32 = 500;
const RUNBOOK_JOB_RUN_LIMIT: u32 = 20;
const RUNBOOK_APPROVAL_LIMIT: u32 = 10_000;
const RUNBOOK_MESSAGE_LIMIT: u32 = 50;
const RUNBOOK_TOOL_CALL_LIMIT: u32 = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RunbookListCursor {
    filter_key: String,
    updated_at_ms: i64,
    status_rank: u8,
    title: String,
    runbook_id: String,
}

#[derive(Debug, Clone)]
struct RunbookCaches {
    agents_by_id: HashMap<String, AgentRecord>,
    goals_by_id: HashMap<String, GoalRecord>,
    projects_by_id: HashMap<String, ProjectRecord>,
    tasks_by_id: HashMap<String, TaskRecord>,
    task_by_board_card_id: HashMap<String, TaskRecord>,
    task_by_job_id: HashMap<String, TaskRecord>,
    task_by_run_id: HashMap<String, TaskRecord>,
    approvals_by_run_id: HashMap<String, Vec<ApprovalRecord>>,
    now_ms: i64,
}

#[derive(Debug, Clone)]
struct SelectedJobExecution {
    job_run: Option<JobRunRecord>,
    linked_run: Option<RunRecord>,
}

pub(super) async fn list_runbooks(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<ListRunbooksQuery>,
) -> std::result::Result<impl IntoResponse, ApiErrorResponse> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
        "mission_control.runbooks_list",
        "mission_control.runbook",
    )?;

    let limit = query.limit.unwrap_or(RUNBOOK_LIST_DEFAULT_LIMIT);
    let limit = limit.clamp(1, RUNBOOK_LIST_MAX_LIMIT) as usize;
    let filter_key = runbook_filter_key(&query);
    let cursor = decode_list_cursor(query.cursor.as_deref(), &filter_key)?;
    let caches = build_runbook_caches(&state)?;

    let mut items = Vec::new();

    if query.kind.as_deref().is_none() || query.kind.as_deref() == Some("assistant_session_run") {
        for session in state
            .storage
            .list_sessions(RUNBOOK_SESSION_SCAN_LIMIT)
            .map_err(|err| internal_err_with_error("listing sessions for runbooks failed", err))?
        {
            let Some(run) = state
                .storage
                .latest_run_for_session(&session.session_id)
                .map_err(|err| {
                    internal_err_with_error("loading latest run for session failed", err)
                })?
            else {
                continue;
            };
            items.push(build_assistant_run_summary(&session, &run, &caches));
        }
    }

    if query.kind.as_deref().is_none() || query.kind.as_deref() == Some("board_card_run") {
        for board in state
            .storage
            .list_boards()
            .map_err(|err| internal_err_with_error("listing boards for runbooks failed", err))?
        {
            let cards = state
                .storage
                .list_board_cards(&board.board_id)
                .map_err(|err| {
                    internal_err_with_error("listing board cards for runbooks failed", err)
                })?;
            for card in cards {
                items.push(build_board_card_summary(&state, &card, &caches)?);
            }
        }
    }

    if query.kind.as_deref().is_none() || query.kind.as_deref() == Some("scheduled_job_run") {
        for job in state
            .storage
            .list_jobs(500, true)
            .map_err(|err| internal_err_with_error("listing jobs for runbooks failed", err))?
        {
            items.push(build_job_summary(&state, &job, &caches)?);
        }
    }

    if query.kind.as_deref().is_none() || query.kind.as_deref() == Some("strategy_task_execution") {
        for task in caches.tasks_by_id.values() {
            items.push(build_task_summary(&state, task, &caches)?);
        }
    }

    items.retain(|item| matches_runbook_filters(item, &query));
    items.sort_by(compare_runbook_summary_items);

    let counts_by_status = build_status_counts(&items);
    let start_index = match cursor {
        Some(ref cursor_value) => items
            .iter()
            .position(|item| matches_cursor(item, cursor_value))
            .map(|index| index + 1)
            .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "invalid_cursor"))?,
        None => 0,
    };

    let paged_items = items
        .iter()
        .skip(start_index)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_cursor = if start_index + paged_items.len() < items.len() {
        paged_items
            .last()
            .map(|last| encode_list_cursor(last, &filter_key))
    } else {
        None
    };

    Ok(Json(ListRunbooksResponse {
        generated_at_ms: caches.now_ms,
        items: paged_items,
        counts_by_status,
        next_cursor,
    }))
}

pub(super) async fn get_runbook_detail(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path((runbook_kind, anchor_id)): Path<(String, String)>,
) -> std::result::Result<impl IntoResponse, ApiErrorResponse> {
    let auth = require_bearer_auth_with_error(&headers, &state)?;
    require_roles_with_audit(
        &headers,
        &state,
        &auth,
        &[
            ROLE_OPERATOR_ADMIN,
            ROLE_OPERATOR_READONLY,
            ROLE_AUTOMATION_RUNNER,
        ],
        "mission_control.runbook_detail",
        "mission_control.runbook",
    )?;

    let kind = runbook_kind.trim();
    let anchor_id = anchor_id.trim();
    if kind.is_empty() || anchor_id.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid kind or anchor"));
    }

    let caches = build_runbook_caches(&state)?;
    let detail = match kind {
        "assistant_session_run" => build_assistant_run_detail(&state, anchor_id, &caches)?,
        "board_card_run" => build_board_card_detail(&state, anchor_id, &caches)?,
        "scheduled_job_run" => build_job_detail(&state, anchor_id, &caches)?,
        "strategy_task_execution" => build_task_detail(&state, anchor_id, &caches)?,
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "invalid runbook kind")),
    };
    Ok(Json(detail))
}

fn build_runbook_caches(state: &AppState) -> std::result::Result<RunbookCaches, ApiErrorResponse> {
    let now_ms = current_time_ms();
    let goals = collect_goal_records(state)?;
    let projects = collect_project_records(state)?;
    let tasks = collect_task_records(
        state,
        TaskListFilter {
            now_ms,
            ..TaskListFilter::default()
        },
    )?;
    let agents = state
        .storage
        .list_agents()
        .map_err(|err| internal_err_with_error("listing agents for runbooks failed", err))?;
    let approvals = state
        .storage
        .list_approvals(None, RUNBOOK_APPROVAL_LIMIT)
        .map_err(|err| internal_err_with_error("listing approvals for runbooks failed", err))?;

    let mut task_by_board_card_id = HashMap::new();
    let mut task_by_job_id = HashMap::new();
    let mut task_by_run_id = HashMap::new();
    let mut tasks_by_id = HashMap::new();
    for task in tasks {
        if let Some(card_id) = task.linked_board_card_id.as_ref() {
            task_by_board_card_id.insert(card_id.clone(), task.clone());
        }
        if let Some(job_id) = task.linked_job_id.as_ref() {
            task_by_job_id.insert(job_id.clone(), task.clone());
        }
        let runtime = state
            .storage
            .resolve_task_runtime_link(&task)
            .map_err(|err| {
                internal_err_with_error("resolving task runtime for runbooks failed", err)
            })?;
        if let Some(run_id) = runtime.latest_run_id.as_ref() {
            task_by_run_id
                .entry(run_id.clone())
                .or_insert_with(|| task.clone());
        }
        tasks_by_id.insert(task.task_id.clone(), task);
    }

    let mut approvals_by_run_id: HashMap<String, Vec<ApprovalRecord>> = HashMap::new();
    for approval in approvals {
        approvals_by_run_id
            .entry(approval.run_id.clone())
            .or_default()
            .push(approval);
    }
    for items in approvals_by_run_id.values_mut() {
        items.sort_by(|left, right| {
            left.requested_at
                .cmp(&right.requested_at)
                .then_with(|| left.approval_id.cmp(&right.approval_id))
        });
    }

    Ok(RunbookCaches {
        agents_by_id: agents
            .into_iter()
            .map(|agent| (agent.agent_id.clone(), agent))
            .collect(),
        goals_by_id: goals
            .into_iter()
            .map(|goal| (goal.goal_id.clone(), goal))
            .collect(),
        projects_by_id: projects
            .into_iter()
            .map(|project| (project.project_id.clone(), project))
            .collect(),
        tasks_by_id,
        task_by_board_card_id,
        task_by_job_id,
        task_by_run_id,
        approvals_by_run_id,
        now_ms,
    })
}

fn build_assistant_run_summary(
    session: &SessionRecord,
    run: &RunRecord,
    caches: &RunbookCaches,
) -> RunbookSummaryItemResponse {
    let linked_task = caches.task_by_run_id.get(&run.run_id);
    let unresolved = requested_approvals_for_run(caches, &run.run_id);
    let mut linked_entities = vec![
        session_entity_ref(session),
        run_entity_ref(run, "assistant"),
    ];
    append_task_context_refs(&mut linked_entities, linked_task, caches);
    let owner_agent_id = Some(session.agent_id.clone());
    let owner_agent_label = owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());
    let status = resolve_run_status(run, linked_task, !unresolved.is_empty());
    let current_step_label = Some(match status.as_str() {
        "waiting" => "Await approval".to_string(),
        "failed" => "Run failed".to_string(),
        "completed" => "Run succeeded".to_string(),
        "blocked" => "Blocked by linked task".to_string(),
        _ => "Run executing".to_string(),
    });
    RunbookSummaryItemResponse {
        runbook_id: format!("assistant_session_run:{}", run.run_id),
        runbook_kind: "assistant_session_run".to_string(),
        anchor_kind: "run".to_string(),
        anchor_id: run.run_id.clone(),
        title: assistant_run_title(session),
        status,
        status_reason: unresolved
            .first()
            .map(|approval| approval.request_summary.clone())
            .or_else(|| run.error_text.clone())
            .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
        owner_agent_id,
        owner_agent_label,
        primary_entity_label: assistant_run_title(session),
        updated_at_ms: run_sort_ms(run),
        current_step_label,
        warning_count: 0,
        linked_entities,
        availability: ready_availability(caches.now_ms),
    }
}

fn build_board_card_summary(
    state: &AppState,
    card: &BoardCardRecord,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookSummaryItemResponse, ApiErrorResponse> {
    let selected_run = selected_board_card_run(state, card)?;
    let linked_task = caches.task_by_board_card_id.get(&card.card_id);
    let unresolved = selected_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let mut linked_entities = vec![board_card_entity_ref(card)];
    if let Some(run) = selected_run.as_ref() {
        linked_entities.push(run_entity_ref(run, "boards"));
    }
    append_task_context_refs(&mut linked_entities, linked_task, caches);
    let (status, status_reason, warning_count, availability) = match selected_run.as_ref() {
        Some(run) => (
            resolve_run_status(run, linked_task, !unresolved.is_empty()),
            unresolved
                .first()
                .map(|approval| approval.request_summary.clone())
                .or_else(|| run.error_text.clone())
                .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
            0,
            ready_availability(caches.now_ms),
        ),
        None if card.latest_run_id.is_some() => (
            "limited".to_string(),
            Some("Latest run could not be resolved from the card link.".to_string()),
            1,
            limited_availability(caches.now_ms, vec!["run".to_string()]),
        ),
        None => (
            "pending".to_string(),
            None,
            0,
            ready_availability(caches.now_ms),
        ),
    };
    let owner_agent_id = card
        .owner_agent_id
        .clone()
        .or_else(|| linked_task.and_then(|task| task.owner_agent_id.clone()))
        .or_else(|| {
            selected_run
                .as_ref()
                .and_then(|run| state.storage.get_session(&run.session_id).ok().flatten())
                .map(|session| session.agent_id)
        });
    let owner_agent_label = owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());
    Ok(RunbookSummaryItemResponse {
        runbook_id: format!("board_card_run:{}", card.card_id),
        runbook_kind: "board_card_run".to_string(),
        anchor_kind: "card".to_string(),
        anchor_id: card.card_id.clone(),
        title: card.title.clone(),
        status: status.clone(),
        status_reason,
        owner_agent_id,
        owner_agent_label,
        primary_entity_label: card.title.clone(),
        updated_at_ms: selected_run
            .as_ref()
            .map(run_sort_ms)
            .unwrap_or(card.updated_at),
        current_step_label: Some(summary_step_label(
            &status,
            [
                "Card ready",
                "Run executing",
                "Await approval",
                "Blocked by linked task",
                "Card run completed",
                "Card run failed",
                "Runbook limited",
            ],
        )),
        warning_count,
        linked_entities,
        availability,
    })
}

fn build_job_summary(
    state: &AppState,
    job: &JobRecord,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookSummaryItemResponse, ApiErrorResponse> {
    let selected = selected_job_execution(state, job)?;
    let linked_task = caches.task_by_job_id.get(&job.job_id);
    let unresolved = selected
        .linked_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let status = resolve_job_status(
        job,
        selected.job_run.as_ref(),
        selected.linked_run.as_ref(),
        linked_task,
        !unresolved.is_empty(),
    );
    let mut linked_entities = vec![job_entity_ref(job)];
    if let Some(job_run) = selected.job_run.as_ref() {
        linked_entities.push(job_run_entity_ref(job_run));
    }
    if let Some(run) = selected.linked_run.as_ref() {
        linked_entities.push(run_entity_ref(run, "calendar"));
    }
    append_task_context_refs(&mut linked_entities, linked_task, caches);
    let owner_agent_id = linked_task
        .and_then(|task| task.owner_agent_id.clone())
        .or_else(|| Some(job.agent_id.clone()));
    let owner_agent_label = owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());
    Ok(RunbookSummaryItemResponse {
        runbook_id: format!("scheduled_job_run:{}", job.job_id),
        runbook_kind: "scheduled_job_run".to_string(),
        anchor_kind: "job".to_string(),
        anchor_id: job.job_id.clone(),
        title: job.name.clone(),
        status: status.clone(),
        status_reason: unresolved
            .first()
            .map(|approval| approval.request_summary.clone())
            .or_else(|| {
                selected
                    .linked_run
                    .as_ref()
                    .and_then(|run| run.error_text.clone())
            })
            .or_else(|| {
                selected
                    .job_run
                    .as_ref()
                    .and_then(|run| run.error_text.clone())
            })
            .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
        owner_agent_id,
        owner_agent_label,
        primary_entity_label: job.name.clone(),
        updated_at_ms: selected
            .job_run
            .as_ref()
            .map(job_run_sort_ms)
            .or_else(|| selected.linked_run.as_ref().map(run_sort_ms))
            .unwrap_or(job.updated_at),
        current_step_label: Some(summary_step_label(
            &status,
            [
                "Job enabled",
                "Job processing",
                "Await approval",
                "Blocked by linked task",
                "Job run succeeded",
                "Job run failed",
                "Runbook limited",
            ],
        )),
        warning_count: 0,
        linked_entities,
        availability: ready_availability(caches.now_ms),
    })
}

fn build_task_summary(
    state: &AppState,
    task: &TaskRecord,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookSummaryItemResponse, ApiErrorResponse> {
    let runtime = state
        .storage
        .resolve_task_runtime_link(task)
        .map_err(|err| {
            internal_err_with_error("resolving task runtime for task runbook failed", err)
        })?;
    let latest_run = match runtime.latest_run_id.as_ref() {
        Some(run_id) => state.storage.get_run(run_id).map_err(|err| {
            internal_err_with_error("loading linked run for task runbook failed", err)
        })?,
        None => None,
    };
    let unresolved = latest_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let status = resolve_task_runbook_status(task, latest_run.as_ref(), !unresolved.is_empty());
    let mut linked_entities = vec![task_entity_ref(task)];
    if let Some(card_id) = task.linked_board_card_id.as_ref() {
        if let Some(card) = state.storage.get_board_card(card_id).map_err(|err| {
            internal_err_with_error("loading linked board card for task runbook failed", err)
        })? {
            linked_entities.push(board_card_entity_ref(&card));
        }
    }
    if let Some(job_id) = task.linked_job_id.as_ref() {
        if let Some(job) = state.storage.get_job(job_id).map_err(|err| {
            internal_err_with_error("loading linked job for task runbook failed", err)
        })? {
            linked_entities.push(job_entity_ref(&job));
        }
    }
    if let Some(run) = latest_run.as_ref() {
        linked_entities.push(run_entity_ref(run, "assistant"));
    }
    append_task_context_refs(&mut linked_entities, Some(task), caches);
    let owner_agent_label = task
        .owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());
    Ok(RunbookSummaryItemResponse {
        runbook_id: format!("strategy_task_execution:{}", task.task_id),
        runbook_kind: "strategy_task_execution".to_string(),
        anchor_kind: "task".to_string(),
        anchor_id: task.task_id.clone(),
        title: task.title.clone(),
        status: status.clone(),
        status_reason: task
            .blocked_reason
            .clone()
            .or_else(|| {
                unresolved
                    .first()
                    .map(|approval| approval.request_summary.clone())
            })
            .or_else(|| latest_run.as_ref().and_then(|run| run.error_text.clone())),
        owner_agent_id: task.owner_agent_id.clone(),
        owner_agent_label,
        primary_entity_label: task.title.clone(),
        updated_at_ms: latest_run
            .as_ref()
            .map(run_sort_ms)
            .unwrap_or(task.updated_at),
        current_step_label: Some(summary_step_label(
            &status,
            [
                "Task defined",
                "Execution active",
                "Await approval",
                "Task blocked",
                "Execution completed",
                "Execution failed",
                "Runbook limited",
            ],
        )),
        warning_count: 0,
        linked_entities,
        availability: ready_availability(caches.now_ms),
    })
}

fn build_assistant_run_detail(
    state: &AppState,
    run_id: &str,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookDetailResponse, ApiErrorResponse> {
    let run = state
        .storage
        .get_run(run_id)
        .map_err(|err| internal_err_with_error("loading assistant run failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "anchor_missing"))?;
    let session = state
        .storage
        .get_session(&run.session_id)
        .map_err(|err| internal_err_with_error("loading assistant session failed", err))?
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "inconsistent_runbook_links"))?;
    let messages = state
        .storage
        .list_messages(&session.session_id, RUNBOOK_MESSAGE_LIMIT)
        .map_err(|err| internal_err_with_error("listing assistant messages failed", err))?;
    let tool_calls = state
        .storage
        .list_tool_calls(&run.run_id, RUNBOOK_TOOL_CALL_LIMIT)
        .map_err(|err| internal_err_with_error("listing assistant tool calls failed", err))?;
    let approvals = approvals_for_run(caches, &run.run_id);
    let unresolved = approvals
        .iter()
        .filter(|approval| approval.status == "requested")
        .cloned()
        .collect::<Vec<_>>();
    let linked_task = caches.task_by_run_id.get(&run.run_id);
    let status = resolve_run_status(&run, linked_task, !unresolved.is_empty());
    let mut linked_entities = vec![
        session_entity_ref(&session),
        run_entity_ref(&run, "assistant"),
    ];
    append_task_context_refs(&mut linked_entities, linked_task, caches);
    let actions = vec![
        open_entity_action("open-session", "Open session", session_entity_ref(&session)),
        open_entity_action("open-run", "Open run", run_entity_ref(&run, "assistant")),
    ];
    let mut source_facts = vec![
        source_fact(
            "run",
            "run",
            Some(run_entity_ref(&run, "assistant")),
            Some(run_sort_ms(&run)),
            false,
        ),
        source_fact(
            "session",
            "session",
            Some(session_entity_ref(&session)),
            Some(session.updated_at),
            false,
        ),
    ];
    let mut history = Vec::new();
    if let Some(user_message) = messages.iter().rev().find(|message| message.role == "user") {
        history.push(history_item(
            "user-input-ready",
            "message",
            "User input ready",
            Some(truncate_detail(&user_message.content_text)),
            user_message.created_at,
            Some("user_input_ready"),
            vec![message_entity_ref(user_message)],
        ));
        source_facts.push(source_fact(
            "user-message",
            "message",
            Some(message_entity_ref(user_message)),
            Some(user_message.created_at),
            false,
        ));
    }
    history.push(history_item(
        "run-created",
        "run",
        "Run created",
        None,
        run.created_at,
        Some("run_created"),
        vec![run_entity_ref(&run, "assistant")],
    ));
    for approval in &approvals {
        history.push(history_item(
            &format!("approval-{}", approval.approval_id),
            "approval",
            approval_history_label(approval),
            Some(approval.request_summary.clone()),
            approval.requested_at,
            Some("approval_wait"),
            vec![approval_entity_ref(approval)],
        ));
        source_facts.push(source_fact(
            &format!("approval-{}", approval.approval_id),
            "approval",
            Some(approval_entity_ref(approval)),
            Some(approval.requested_at),
            false,
        ));
    }
    for tool_call in &tool_calls {
        history.push(history_item(
            &format!("tool-call-{}", tool_call.tool_call_id),
            "tool_call",
            format!("Tool {}", tool_call.tool_name),
            tool_call.error_text.clone(),
            tool_call.started_at.unwrap_or(run.created_at),
            Some("tool_activity"),
            vec![tool_call_entity_ref(tool_call)],
        ));
    }
    if run.status == "succeeded" {
        history.push(history_item(
            "run-succeeded",
            "run",
            "Run succeeded",
            None,
            run.ended_at.unwrap_or(run.created_at),
            Some("run_succeeded"),
            vec![run_entity_ref(&run, "assistant")],
        ));
    } else if run.status == "failed" {
        history.push(history_item(
            "run-failed",
            "run",
            "Run failed",
            run.error_text.clone(),
            run.ended_at.unwrap_or(run.created_at),
            Some("run_failed"),
            vec![run_entity_ref(&run, "assistant")],
        ));
    }
    history.sort_by(|left, right| {
        left.occurred_at_ms
            .cmp(&right.occurred_at_ms)
            .then_with(|| left.history_id.cmp(&right.history_id))
    });

    let tool_step_state = if tool_calls.iter().any(|call| call.status == "running") {
        "active"
    } else if tool_calls.is_empty() {
        "skipped"
    } else {
        "completed"
    };
    let memory_step_state = if run.usage_json.is_some() {
        "completed"
    } else {
        "skipped"
    };
    let approval_waiting_since = unresolved.first().map(|approval| approval.requested_at);
    let steps = vec![
        step(
            ("session_selected", "Session selected", "session"),
            "completed",
            None,
            (Some(session.created_at), Some(session.updated_at), None),
            vec![session_entity_ref(&session)],
            vec![],
            0,
        ),
        step(
            ("user_input_ready", "User input ready", "message"),
            if messages.iter().any(|message| message.role == "user") {
                "completed"
            } else {
                "limited"
            },
            if messages.iter().any(|message| message.role == "user") {
                None
            } else {
                Some("No durable user message found for this run.".to_string())
            },
            (
                messages
                    .iter()
                    .rev()
                    .find(|message| message.role == "user")
                    .map(|message| message.created_at),
                messages
                    .iter()
                    .rev()
                    .find(|message| message.role == "user")
                    .map(|message| message.created_at),
                None,
            ),
            vec![],
            vec![],
            1,
        ),
        step(
            ("run_created", "Run created", "run"),
            "completed",
            None,
            (Some(run.created_at), Some(run.created_at), None),
            vec![run_entity_ref(&run, "assistant")],
            vec!["open-run".to_string()],
            2,
        ),
        step(
            ("run_executing", "Run executing", "run"),
            if !unresolved.is_empty() {
                "completed"
            } else if matches!(
                run.status.as_str(),
                "queued" | "running" | "pending_approval"
            ) {
                "active"
            } else if run.status == "failed" || run.status == "succeeded" {
                "completed"
            } else {
                "limited"
            },
            None,
            (
                run.started_at.or(Some(run.created_at)),
                if matches!(run.status.as_str(), "failed" | "succeeded") {
                    run.ended_at
                } else {
                    None
                },
                None,
            ),
            vec![run_entity_ref(&run, "assistant")],
            vec!["open-run".to_string()],
            3,
        ),
        step(
            ("approval_wait", "Await approval", "approval"),
            if unresolved.is_empty() {
                "skipped"
            } else {
                "waiting"
            },
            unresolved
                .first()
                .map(|approval| approval.request_summary.clone()),
            (
                approval_waiting_since,
                unresolved.first().and_then(|approval| approval.decided_at),
                approval_waiting_since,
            ),
            unresolved.iter().map(approval_entity_ref).collect(),
            vec![],
            4,
        ),
        step(
            ("tool_activity", "Tool activity", "tool_call"),
            tool_step_state,
            None,
            (
                tool_calls.first().and_then(|call| call.started_at),
                tool_calls.iter().rev().find_map(|call| call.ended_at),
                None,
            ),
            tool_calls.iter().map(tool_call_entity_ref).collect(),
            vec![],
            5,
        ),
        step(
            ("memory_context", "Memory context", "memory"),
            memory_step_state,
            None,
            (run.started_at.or(Some(run.created_at)), run.ended_at, None),
            vec![],
            vec![],
            6,
        ),
        step(
            ("run_succeeded", "Run succeeded", "terminal"),
            if run.status == "succeeded" {
                "completed"
            } else if run.status == "failed" {
                "skipped"
            } else {
                "idle"
            },
            None,
            (run.ended_at, run.ended_at, None),
            vec![run_entity_ref(&run, "assistant")],
            vec!["open-run".to_string()],
            7,
        ),
        step(
            ("run_failed", "Run failed", "terminal"),
            if run.status == "failed" {
                "failed"
            } else if run.status == "succeeded" {
                "skipped"
            } else {
                "idle"
            },
            run.error_text.clone(),
            (run.ended_at, run.ended_at, None),
            vec![run_entity_ref(&run, "assistant")],
            vec!["open-run".to_string()],
            8,
        ),
    ];

    Ok(finalize_detail(
        "assistant_session_run",
        &run.run_id,
        assistant_run_title(&session),
        RunbookDetailParts {
            status,
            status_reason: unresolved
                .first()
                .map(|approval| approval.request_summary.clone())
                .or_else(|| run.error_text.clone())
                .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
            selected_execution_ref: Some(RunbookExecutionRefResponse {
                entity_kind: "run".to_string(),
                entity_id: run.run_id.clone(),
                created_at_ms: run.created_at,
                started_at_ms: run.started_at,
                waiting_since_ms: approval_waiting_since,
                finished_at_ms: run.ended_at,
            }),
            linked_entities,
            steps,
            history,
            actions,
            source_facts,
            availability: ready_availability(caches.now_ms),
            warnings: Vec::new(),
            owner_agent_id: Some(session.agent_id.clone()),
            owner_agent_label: caches
                .agents_by_id
                .get(&session.agent_id)
                .map(|agent| agent.name.clone()),
        },
    ))
}

fn build_board_card_detail(
    state: &AppState,
    card_id: &str,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookDetailResponse, ApiErrorResponse> {
    let card = state
        .storage
        .get_board_card(card_id)
        .map_err(|err| internal_err_with_error("loading board card for runbook failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "anchor_missing"))?;
    let selected_run = selected_board_card_run(state, &card)?;
    let linked_task = caches.task_by_board_card_id.get(&card.card_id);
    let unresolved = selected_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let status = match selected_run.as_ref() {
        Some(run) => resolve_run_status(run, linked_task, !unresolved.is_empty()),
        None if card.latest_run_id.is_some() => "limited".to_string(),
        None => "pending".to_string(),
    };
    let mut linked_entities = vec![board_card_entity_ref(&card)];
    let mut source_facts = vec![source_fact(
        "card",
        "board_card",
        Some(board_card_entity_ref(&card)),
        Some(card.updated_at),
        false,
    )];
    let mut warnings = Vec::new();
    let mut actions = vec![open_entity_action(
        "open-card",
        "Open board card",
        board_card_entity_ref(&card),
    )];
    let mut history = vec![history_item(
        "card-created",
        "board_card",
        "Card ready",
        None,
        card.created_at,
        Some("card_ready"),
        vec![board_card_entity_ref(&card)],
    )];
    let mut selected_execution_ref = None;
    let mut session_linked_state = if card.linked_session_id.is_some() {
        "completed"
    } else {
        "idle"
    };
    let mut run_created_state = "idle";
    let mut run_executing_state = "idle";
    let mut approval_wait_state = "skipped";
    let mut terminal_success_state = "idle";
    let mut terminal_failure_state = "idle";
    let mut run_reason = None;

    if let Some(run) = selected_run.as_ref() {
        selected_execution_ref = Some(RunbookExecutionRefResponse {
            entity_kind: "run".to_string(),
            entity_id: run.run_id.clone(),
            created_at_ms: run.created_at,
            started_at_ms: run.started_at,
            waiting_since_ms: unresolved.first().map(|approval| approval.requested_at),
            finished_at_ms: run.ended_at,
        });
        linked_entities.push(run_entity_ref(run, "boards"));
        source_facts.push(source_fact(
            "run",
            "run",
            Some(run_entity_ref(run, "boards")),
            Some(run_sort_ms(run)),
            false,
        ));
        actions.push(open_entity_action(
            "open-run",
            "Open run",
            run_entity_ref(run, "boards"),
        ));
        history.push(history_item(
            "run-created",
            "run",
            "Run created",
            None,
            run.created_at,
            Some("run_created"),
            vec![run_entity_ref(run, "boards")],
        ));
        run_created_state = "completed";
        run_reason = unresolved
            .first()
            .map(|approval| approval.request_summary.clone())
            .or_else(|| run.error_text.clone());
        if !unresolved.is_empty() {
            approval_wait_state = "waiting";
            run_executing_state = "completed";
        } else if matches!(
            run.status.as_str(),
            "queued" | "running" | "pending_approval"
        ) {
            run_executing_state = "active";
        } else if run.status == "succeeded" {
            run_executing_state = "completed";
            terminal_success_state = "completed";
        } else if run.status == "failed" {
            run_executing_state = "completed";
            terminal_failure_state = "failed";
        }
        for approval in unresolved {
            history.push(history_item(
                &format!("approval-{}", approval.approval_id),
                "approval",
                "Approval requested",
                Some(approval.request_summary.clone()),
                approval.requested_at,
                Some("approval_wait"),
                vec![approval_entity_ref(&approval)],
            ));
        }
    } else if card.latest_run_id.is_some() {
        warnings.push(warning(
            "missing-run",
            "missing_run",
            "Latest run could not be resolved from the board card link.",
        ));
        session_linked_state = if card.linked_session_id.is_some() {
            "completed"
        } else {
            "limited"
        };
        run_created_state = "limited";
        run_executing_state = "limited";
    }

    append_task_context_refs(&mut linked_entities, linked_task, caches);
    let owner_agent_id = card
        .owner_agent_id
        .clone()
        .or_else(|| linked_task.and_then(|task| task.owner_agent_id.clone()));
    let owner_agent_label = owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());

    let steps = vec![
        step(
            ("card_ready", "Card ready", "board_card"),
            "completed",
            None,
            (Some(card.created_at), Some(card.updated_at), None),
            vec![board_card_entity_ref(&card)],
            vec!["open-card".to_string()],
            0,
        ),
        step(
            ("session_linked", "Session linked", "session"),
            session_linked_state,
            if card.linked_session_id.is_some() {
                None
            } else {
                Some("No linked session on this card yet.".to_string())
            },
            (
                card.updated_at.checked_sub(1),
                card.updated_at.checked_sub(1),
                None,
            ),
            card.linked_session_id
                .as_ref()
                .map(|session_id| session_link_entity_ref(session_id))
                .into_iter()
                .collect(),
            vec![],
            1,
        ),
        step(
            ("run_created", "Run created", "run"),
            run_created_state,
            run_reason.clone(),
            (
                selected_execution_ref
                    .as_ref()
                    .map(|item| item.created_at_ms),
                selected_execution_ref
                    .as_ref()
                    .map(|item| item.created_at_ms),
                None,
            ),
            selected_run
                .iter()
                .map(|run| run_entity_ref(run, "boards"))
                .collect(),
            vec!["open-run".to_string()],
            2,
        ),
        step(
            ("run_executing", "Run executing", "run"),
            run_executing_state,
            run_reason.clone(),
            (
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.started_at_ms)
                    .or(selected_execution_ref
                        .as_ref()
                        .map(|item| item.created_at_ms)),
                if terminal_success_state == "completed" || terminal_failure_state == "failed" {
                    selected_execution_ref
                        .as_ref()
                        .and_then(|item| item.finished_at_ms)
                } else {
                    None
                },
                None,
            ),
            selected_run
                .iter()
                .map(|run| run_entity_ref(run, "boards"))
                .collect(),
            vec!["open-run".to_string()],
            3,
        ),
        step(
            ("approval_wait", "Await approval", "approval"),
            approval_wait_state,
            run_reason.clone(),
            (
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.waiting_since_ms),
                None,
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.waiting_since_ms),
            ),
            requested_approvals_for_run(
                caches,
                selected_run
                    .as_ref()
                    .map(|run| run.run_id.as_str())
                    .unwrap_or(""),
            )
            .iter()
            .map(approval_entity_ref)
            .collect(),
            vec![],
            4,
        ),
        step(
            ("card_run_completed", "Card run completed", "terminal"),
            terminal_success_state,
            None,
            (
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.finished_at_ms),
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.finished_at_ms),
                None,
            ),
            selected_run
                .iter()
                .map(|run| run_entity_ref(run, "boards"))
                .collect(),
            vec!["open-run".to_string()],
            5,
        ),
        step(
            ("card_run_failed", "Card run failed", "terminal"),
            terminal_failure_state,
            run_reason.clone(),
            (
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.finished_at_ms),
                selected_execution_ref
                    .as_ref()
                    .and_then(|item| item.finished_at_ms),
                None,
            ),
            selected_run
                .iter()
                .map(|run| run_entity_ref(run, "boards"))
                .collect(),
            vec!["open-run".to_string()],
            6,
        ),
    ];

    let availability = if warnings.is_empty() {
        ready_availability(caches.now_ms)
    } else {
        limited_availability(caches.now_ms, vec!["run".to_string()])
    };

    Ok(finalize_detail(
        "board_card_run",
        &card.card_id,
        card.title.clone(),
        RunbookDetailParts {
            status,
            status_reason: run_reason
                .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
            selected_execution_ref,
            linked_entities,
            steps,
            history,
            actions,
            source_facts,
            availability,
            warnings,
            owner_agent_id,
            owner_agent_label,
        },
    ))
}

fn build_job_detail(
    state: &AppState,
    job_id: &str,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookDetailResponse, ApiErrorResponse> {
    let job = state
        .storage
        .get_job(job_id)
        .map_err(|err| internal_err_with_error("loading job for runbook failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "anchor_missing"))?;
    let selected = selected_job_execution(state, &job)?;
    let linked_task = caches.task_by_job_id.get(&job.job_id);
    let unresolved = selected
        .linked_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let status = resolve_job_status(
        &job,
        selected.job_run.as_ref(),
        selected.linked_run.as_ref(),
        linked_task,
        !unresolved.is_empty(),
    );
    let mut linked_entities = vec![job_entity_ref(&job)];
    let mut actions = vec![
        open_entity_action("open-job", "Open job", job_entity_ref(&job)),
        RunbookActionResponse {
            action_id: "run-job-now".to_string(),
            action_kind: "run_job_now".to_string(),
            label: "Run job now".to_string(),
            availability: "enabled".to_string(),
            disabled_reason: None,
            target_entity_ref: Some(job_entity_ref(&job)),
        },
        RunbookActionResponse {
            action_id: "toggle-job-enabled".to_string(),
            action_kind: "toggle_job_enabled".to_string(),
            label: if job.enabled {
                "Disable job".to_string()
            } else {
                "Enable job".to_string()
            },
            availability: "enabled".to_string(),
            disabled_reason: None,
            target_entity_ref: Some(job_entity_ref(&job)),
        },
    ];
    let mut source_facts = vec![source_fact(
        "job",
        "job",
        Some(job_entity_ref(&job)),
        Some(job.updated_at),
        false,
    )];
    let mut history = vec![history_item(
        "job-created",
        "job",
        "Job ready",
        None,
        job.created_at,
        Some("job_enabled"),
        vec![job_entity_ref(&job)],
    )];

    let selected_execution_ref =
        selected
            .job_run
            .as_ref()
            .map(|job_run| RunbookExecutionRefResponse {
                entity_kind: "job_run".to_string(),
                entity_id: job_run.job_run_id.clone(),
                created_at_ms: job_run.created_at,
                started_at_ms: job_run.started_at,
                waiting_since_ms: selected.linked_run.as_ref().and_then(|run| {
                    requested_approvals_for_run(caches, &run.run_id)
                        .first()
                        .map(|approval| approval.requested_at)
                }),
                finished_at_ms: job_run.ended_at,
            });
    if let Some(job_run) = selected.job_run.as_ref() {
        linked_entities.push(job_run_entity_ref(job_run));
        source_facts.push(source_fact(
            "job-run",
            "job_run",
            Some(job_run_entity_ref(job_run)),
            Some(job_run_sort_ms(job_run)),
            false,
        ));
        history.push(history_item(
            "job-run-started",
            "job_run",
            "Job run started",
            None,
            job_run.started_at.unwrap_or(job_run.created_at),
            Some("job_run_started"),
            vec![job_run_entity_ref(job_run)],
        ));
    }
    if let Some(run) = selected.linked_run.as_ref() {
        linked_entities.push(run_entity_ref(run, "calendar"));
        actions.push(open_entity_action(
            "open-linked-run",
            "Open linked run",
            run_entity_ref(run, "calendar"),
        ));
        source_facts.push(source_fact(
            "linked-run",
            "run",
            Some(run_entity_ref(run, "calendar")),
            Some(run_sort_ms(run)),
            false,
        ));
        for approval in &unresolved {
            history.push(history_item(
                &format!("approval-{}", approval.approval_id),
                "approval",
                "Approval requested",
                Some(approval.request_summary.clone()),
                approval.requested_at,
                Some("approval_wait"),
                vec![approval_entity_ref(approval)],
            ));
        }
    }
    append_task_context_refs(&mut linked_entities, linked_task, caches);

    let owner_agent_id = linked_task
        .and_then(|task| task.owner_agent_id.clone())
        .or_else(|| Some(job.agent_id.clone()));
    let owner_agent_label = owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());

    let steps = vec![
        step(
            ("job_enabled", "Job enabled", "job"),
            if job.enabled { "completed" } else { "idle" },
            if job.enabled {
                None
            } else {
                Some("Job is disabled.".to_string())
            },
            (Some(job.created_at), Some(job.updated_at), None),
            vec![job_entity_ref(&job)],
            vec!["open-job".to_string(), "toggle-job-enabled".to_string()],
            0,
        ),
        step(
            ("job_due_or_triggered", "Job due or triggered", "job"),
            if selected.job_run.is_some() {
                "completed"
            } else {
                "idle"
            },
            None,
            (
                selected.job_run.as_ref().map(|run| run.created_at),
                selected.job_run.as_ref().map(|run| run.created_at),
                None,
            ),
            selected.job_run.iter().map(job_run_entity_ref).collect(),
            vec!["run-job-now".to_string()],
            1,
        ),
        step(
            ("job_run_started", "Job run started", "job_run"),
            if selected.job_run.is_some() {
                "completed"
            } else {
                "idle"
            },
            None,
            (
                selected
                    .job_run
                    .as_ref()
                    .and_then(|run| run.started_at)
                    .or(selected.job_run.as_ref().map(|run| run.created_at)),
                selected
                    .job_run
                    .as_ref()
                    .and_then(|run| run.started_at)
                    .or(selected.job_run.as_ref().map(|run| run.created_at)),
                None,
            ),
            selected.job_run.iter().map(job_run_entity_ref).collect(),
            vec!["run-job-now".to_string()],
            2,
        ),
        step(
            ("job_processing", "Job processing", "job_run"),
            match status.as_str() {
                "waiting" => "completed",
                "active" => "active",
                "completed" | "failed" => "completed",
                "limited" => "limited",
                _ => "idle",
            },
            selected
                .linked_run
                .as_ref()
                .and_then(|run| run.error_text.clone())
                .or_else(|| {
                    selected
                        .job_run
                        .as_ref()
                        .and_then(|run| run.error_text.clone())
                }),
            (
                selected
                    .job_run
                    .as_ref()
                    .and_then(|run| run.started_at)
                    .or(selected.job_run.as_ref().map(|run| run.created_at)),
                if matches!(status.as_str(), "completed" | "failed") {
                    selected.job_run.as_ref().and_then(|run| run.ended_at)
                } else {
                    None
                },
                None,
            ),
            linked_entities.clone(),
            vec!["open-job".to_string()],
            3,
        ),
        step(
            ("approval_wait", "Await approval", "approval"),
            if unresolved.is_empty() {
                "skipped"
            } else {
                "waiting"
            },
            unresolved
                .first()
                .map(|approval| approval.request_summary.clone()),
            (
                unresolved.first().map(|approval| approval.requested_at),
                None,
                unresolved.first().map(|approval| approval.requested_at),
            ),
            unresolved.iter().map(approval_entity_ref).collect(),
            vec![],
            4,
        ),
        step(
            ("job_run_succeeded", "Job run succeeded", "terminal"),
            if status == "completed" {
                "completed"
            } else {
                "idle"
            },
            None,
            (
                selected.job_run.as_ref().and_then(|run| run.ended_at),
                selected.job_run.as_ref().and_then(|run| run.ended_at),
                None,
            ),
            selected.job_run.iter().map(job_run_entity_ref).collect(),
            vec!["open-job".to_string()],
            5,
        ),
        step(
            ("job_run_failed", "Job run failed", "terminal"),
            if status == "failed" { "failed" } else { "idle" },
            selected
                .linked_run
                .as_ref()
                .and_then(|run| run.error_text.clone())
                .or_else(|| {
                    selected
                        .job_run
                        .as_ref()
                        .and_then(|run| run.error_text.clone())
                }),
            (
                selected.job_run.as_ref().and_then(|run| run.ended_at),
                selected.job_run.as_ref().and_then(|run| run.ended_at),
                None,
            ),
            selected.job_run.iter().map(job_run_entity_ref).collect(),
            vec!["open-job".to_string()],
            6,
        ),
    ];

    Ok(finalize_detail(
        "scheduled_job_run",
        &job.job_id,
        job.name.clone(),
        RunbookDetailParts {
            status,
            status_reason: unresolved
                .first()
                .map(|approval| approval.request_summary.clone())
                .or_else(|| {
                    selected
                        .linked_run
                        .as_ref()
                        .and_then(|run| run.error_text.clone())
                })
                .or_else(|| {
                    selected
                        .job_run
                        .as_ref()
                        .and_then(|run| run.error_text.clone())
                })
                .or_else(|| linked_task.and_then(|task| task.blocked_reason.clone())),
            selected_execution_ref,
            linked_entities,
            steps,
            history,
            actions,
            source_facts,
            availability: ready_availability(caches.now_ms),
            warnings: Vec::new(),
            owner_agent_id,
            owner_agent_label,
        },
    ))
}

fn build_task_detail(
    state: &AppState,
    task_id: &str,
    caches: &RunbookCaches,
) -> std::result::Result<RunbookDetailResponse, ApiErrorResponse> {
    let task = state
        .storage
        .get_task(task_id)
        .map_err(|err| internal_err_with_error("loading task for runbook failed", err))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "anchor_missing"))?;
    let runtime = state
        .storage
        .resolve_task_runtime_link(&task)
        .map_err(|err| internal_err_with_error("resolving task runtime link failed", err))?;
    let latest_run = match runtime.latest_run_id.as_ref() {
        Some(run_id) => state
            .storage
            .get_run(run_id)
            .map_err(|err| internal_err_with_error("loading task linked run failed", err))?,
        None => None,
    };
    let unresolved = latest_run
        .as_ref()
        .map(|run| requested_approvals_for_run(caches, &run.run_id))
        .unwrap_or_default();
    let status = resolve_task_runbook_status(&task, latest_run.as_ref(), !unresolved.is_empty());
    let mut linked_entities = vec![task_entity_ref(&task)];
    let mut actions = vec![open_entity_action(
        "open-task",
        "Open task",
        task_entity_ref(&task),
    )];
    let mut source_facts = vec![source_fact(
        "task",
        "task",
        Some(task_entity_ref(&task)),
        Some(task.updated_at),
        false,
    )];
    let mut history = vec![history_item(
        "task-defined",
        "task",
        "Task defined",
        None,
        task.created_at,
        Some("task_defined"),
        vec![task_entity_ref(&task)],
    )];
    if let Some(card_id) = task.linked_board_card_id.as_ref() {
        if let Some(card) = state
            .storage
            .get_board_card(card_id)
            .map_err(|err| internal_err_with_error("loading linked board card failed", err))?
        {
            linked_entities.push(board_card_entity_ref(&card));
            actions.push(open_entity_action(
                "open-linked-card",
                "Open board card",
                board_card_entity_ref(&card),
            ));
            history.push(history_item(
                "task-linked-card",
                "link",
                "Board card linked",
                Some(card.title.clone()),
                card.updated_at,
                Some("execution_linked"),
                vec![board_card_entity_ref(&card)],
            ));
        }
    }
    if let Some(job_id) = task.linked_job_id.as_ref() {
        if let Some(job) = state
            .storage
            .get_job(job_id)
            .map_err(|err| internal_err_with_error("loading linked job failed", err))?
        {
            linked_entities.push(job_entity_ref(&job));
            actions.push(open_entity_action(
                "open-linked-job",
                "Open job",
                job_entity_ref(&job),
            ));
            history.push(history_item(
                "task-linked-job",
                "link",
                "Job linked",
                Some(job.name.clone()),
                job.updated_at,
                Some("execution_linked"),
                vec![job_entity_ref(&job)],
            ));
        }
    }
    if let Some(run) = latest_run.as_ref() {
        linked_entities.push(run_entity_ref(run, "assistant"));
        actions.push(open_entity_action(
            "open-linked-run",
            "Open run",
            run_entity_ref(run, "assistant"),
        ));
        source_facts.push(source_fact(
            "run",
            "run",
            Some(run_entity_ref(run, "assistant")),
            Some(run_sort_ms(run)),
            false,
        ));
        history.push(history_item(
            "task-run",
            "run",
            "Execution selected",
            None,
            run.created_at,
            Some("execution_active"),
            vec![run_entity_ref(run, "assistant")],
        ));
        for approval in &unresolved {
            history.push(history_item(
                &format!("approval-{}", approval.approval_id),
                "approval",
                "Approval requested",
                Some(approval.request_summary.clone()),
                approval.requested_at,
                Some("approval_wait"),
                vec![approval_entity_ref(approval)],
            ));
        }
    }
    append_task_context_refs(&mut linked_entities, Some(&task), caches);
    let owner_agent_label = task
        .owner_agent_id
        .as_ref()
        .and_then(|agent_id| caches.agents_by_id.get(agent_id))
        .map(|agent| agent.name.clone());
    let steps = vec![
        step(
            ("task_defined", "Task defined", "task"),
            "completed",
            None,
            (Some(task.created_at), Some(task.updated_at), None),
            vec![task_entity_ref(&task)],
            vec!["open-task".to_string()],
            0,
        ),
        step(
            ("execution_linked", "Execution linked", "link"),
            if task.linked_board_card_id.is_some() || task.linked_job_id.is_some() {
                "completed"
            } else {
                "idle"
            },
            None,
            (Some(task.updated_at), Some(task.updated_at), None),
            linked_entities.clone(),
            vec![],
            1,
        ),
        step(
            ("execution_active", "Execution active", "run"),
            match status.as_str() {
                "waiting" => "completed",
                "active" => "active",
                "completed" | "failed" => "completed",
                "blocked" => "idle",
                _ => "idle",
            },
            latest_run.as_ref().and_then(|run| run.error_text.clone()),
            (
                latest_run
                    .as_ref()
                    .and_then(|run| run.started_at)
                    .or(latest_run.as_ref().map(|run| run.created_at)),
                if matches!(status.as_str(), "completed" | "failed") {
                    latest_run.as_ref().and_then(|run| run.ended_at)
                } else {
                    None
                },
                None,
            ),
            latest_run
                .iter()
                .map(|run| run_entity_ref(run, "assistant"))
                .collect(),
            vec!["open-linked-run".to_string()],
            2,
        ),
        step(
            ("approval_wait", "Await approval", "approval"),
            if unresolved.is_empty() {
                "skipped"
            } else {
                "waiting"
            },
            unresolved
                .first()
                .map(|approval| approval.request_summary.clone()),
            (
                unresolved.first().map(|approval| approval.requested_at),
                None,
                unresolved.first().map(|approval| approval.requested_at),
            ),
            unresolved.iter().map(approval_entity_ref).collect(),
            vec![],
            3,
        ),
        step(
            ("blocked", "Task blocked", "task"),
            if task.status == "blocked" {
                "blocked"
            } else {
                "skipped"
            },
            task.blocked_reason.clone(),
            (Some(task.updated_at), None, Some(task.updated_at)),
            vec![task_entity_ref(&task)],
            vec!["open-task".to_string()],
            4,
        ),
        step(
            ("execution_completed", "Execution completed", "terminal"),
            if status == "completed" {
                "completed"
            } else {
                "idle"
            },
            None,
            (
                latest_run.as_ref().and_then(|run| run.ended_at),
                latest_run.as_ref().and_then(|run| run.ended_at),
                None,
            ),
            latest_run
                .iter()
                .map(|run| run_entity_ref(run, "assistant"))
                .collect(),
            vec!["open-linked-run".to_string()],
            5,
        ),
        step(
            ("execution_failed", "Execution failed", "terminal"),
            if status == "failed" { "failed" } else { "idle" },
            latest_run.as_ref().and_then(|run| run.error_text.clone()),
            (
                latest_run.as_ref().and_then(|run| run.ended_at),
                latest_run.as_ref().and_then(|run| run.ended_at),
                None,
            ),
            latest_run
                .iter()
                .map(|run| run_entity_ref(run, "assistant"))
                .collect(),
            vec!["open-linked-run".to_string()],
            6,
        ),
    ];
    Ok(finalize_detail(
        "strategy_task_execution",
        &task.task_id,
        task.title.clone(),
        RunbookDetailParts {
            status,
            status_reason: task
                .blocked_reason
                .clone()
                .or_else(|| {
                    unresolved
                        .first()
                        .map(|approval| approval.request_summary.clone())
                })
                .or_else(|| latest_run.as_ref().and_then(|run| run.error_text.clone())),
            selected_execution_ref: latest_run.as_ref().map(|run| RunbookExecutionRefResponse {
                entity_kind: "run".to_string(),
                entity_id: run.run_id.clone(),
                created_at_ms: run.created_at,
                started_at_ms: run.started_at,
                waiting_since_ms: unresolved.first().map(|approval| approval.requested_at),
                finished_at_ms: run.ended_at,
            }),
            linked_entities,
            steps,
            history,
            actions,
            source_facts,
            availability: ready_availability(caches.now_ms),
            warnings: Vec::new(),
            owner_agent_id: task.owner_agent_id.clone(),
            owner_agent_label,
        },
    ))
}

struct RunbookDetailParts {
    status: String,
    status_reason: Option<String>,
    selected_execution_ref: Option<RunbookExecutionRefResponse>,
    linked_entities: Vec<RunbookEntityRefResponse>,
    steps: Vec<RunbookStepResponse>,
    history: Vec<RunbookHistoryItemResponse>,
    actions: Vec<RunbookActionResponse>,
    source_facts: Vec<RunbookSourceFactResponse>,
    availability: RunbookDataAvailabilityResponse,
    warnings: Vec<RunbookWarningResponse>,
    owner_agent_id: Option<String>,
    owner_agent_label: Option<String>,
}

fn finalize_detail(
    runbook_kind: &str,
    anchor_id: &str,
    title: String,
    parts: RunbookDetailParts,
) -> RunbookDetailResponse {
    let active_step_id = resolve_active_step_id(&parts.steps, &parts.status);
    let next_step_ids =
        resolve_next_step_ids(runbook_kind, active_step_id.as_deref(), &parts.status);
    let RunbookDetailParts {
        status,
        status_reason,
        selected_execution_ref,
        linked_entities,
        steps,
        history,
        actions,
        source_facts,
        availability,
        warnings,
        owner_agent_id,
        owner_agent_label,
    } = parts;
    RunbookDetailResponse {
        runbook_id: format!("{runbook_kind}:{anchor_id}"),
        runbook_kind: runbook_kind.to_string(),
        template_id: runbook_kind.to_string(),
        template_version: RUNBOOK_TEMPLATE_VERSION.to_string(),
        anchor_kind: anchor_kind_for_runbook(runbook_kind).to_string(),
        anchor_id: anchor_id.to_string(),
        title,
        status,
        status_reason,
        generated_at_ms: current_time_ms(),
        selected_execution_ref,
        active_step_id,
        next_step_ids,
        linked_entities,
        steps,
        history,
        actions,
        source_facts,
        availability,
        warnings,
        owner_agent_id,
        owner_agent_label,
    }
}

fn compare_runbook_summary_items(
    left: &RunbookSummaryItemResponse,
    right: &RunbookSummaryItemResponse,
) -> std::cmp::Ordering {
    right
        .updated_at_ms
        .cmp(&left.updated_at_ms)
        .then_with(|| status_rank(&left.status).cmp(&status_rank(&right.status)))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.runbook_id.cmp(&right.runbook_id))
}

fn build_status_counts(items: &[RunbookSummaryItemResponse]) -> RunbookStatusCountsResponse {
    let mut counts = RunbookStatusCountsResponse {
        pending: 0,
        active: 0,
        waiting: 0,
        blocked: 0,
        failed: 0,
        completed: 0,
        limited: 0,
    };
    for item in items {
        match item.status.as_str() {
            "pending" => counts.pending += 1,
            "active" => counts.active += 1,
            "waiting" => counts.waiting += 1,
            "blocked" => counts.blocked += 1,
            "failed" => counts.failed += 1,
            "completed" => counts.completed += 1,
            "limited" => counts.limited += 1,
            _ => {}
        }
    }
    counts
}

fn matches_runbook_filters(item: &RunbookSummaryItemResponse, query: &ListRunbooksQuery) -> bool {
    if let Some(kind) = query.kind.as_deref() {
        if item.runbook_kind != kind {
            return false;
        }
    }
    if let Some(status) = query.status.as_deref() {
        if item.status != status {
            return false;
        }
    }
    if let Some(owner_agent_id) = query.owner_agent_id.as_deref() {
        if item.owner_agent_id.as_deref() != Some(owner_agent_id) {
            return false;
        }
    }
    if let Some(linked_task_id) = query.linked_task_id.as_deref() {
        if !item
            .linked_entities
            .iter()
            .any(|entity| entity.entity_kind == "task" && entity.entity_id == linked_task_id)
        {
            return false;
        }
    }
    if let Some(linked_project_id) = query.linked_project_id.as_deref() {
        if !item
            .linked_entities
            .iter()
            .any(|entity| entity.entity_kind == "project" && entity.entity_id == linked_project_id)
        {
            return false;
        }
    }
    if let Some(linked_goal_id) = query.linked_goal_id.as_deref() {
        if !item
            .linked_entities
            .iter()
            .any(|entity| entity.entity_kind == "goal" && entity.entity_id == linked_goal_id)
        {
            return false;
        }
    }
    if let Some(text) = query.query.as_deref() {
        let query_text = text.trim().to_ascii_lowercase();
        if !query_text.is_empty() {
            let haystack = format!(
                "{} {} {} {} {}",
                item.title,
                item.primary_entity_label,
                item.status_reason.clone().unwrap_or_default(),
                item.owner_agent_label.clone().unwrap_or_default(),
                item.current_step_label.clone().unwrap_or_default()
            )
            .to_ascii_lowercase();
            if !haystack.contains(&query_text) {
                return false;
            }
        }
    }
    true
}

fn resolve_run_status(
    run: &RunRecord,
    linked_task: Option<&TaskRecord>,
    has_waiting_approval: bool,
) -> String {
    if linked_task.map(|task| task.status.as_str()) == Some("blocked") {
        return "blocked".to_string();
    }
    if has_waiting_approval {
        return "waiting".to_string();
    }
    match run.status.as_str() {
        "failed" => "failed".to_string(),
        "succeeded" => "completed".to_string(),
        "queued" | "running" | "pending_approval" => "active".to_string(),
        _ => "limited".to_string(),
    }
}

fn resolve_job_status(
    _job: &JobRecord,
    job_run: Option<&JobRunRecord>,
    linked_run: Option<&RunRecord>,
    linked_task: Option<&TaskRecord>,
    has_waiting_approval: bool,
) -> String {
    if linked_task.map(|task| task.status.as_str()) == Some("blocked") {
        return "blocked".to_string();
    }
    if has_waiting_approval {
        return "waiting".to_string();
    }
    if let Some(run) = linked_run {
        return resolve_run_status(run, linked_task, false);
    }
    if let Some(job_run) = job_run {
        return match job_run.status.as_str() {
            "failed" => "failed".to_string(),
            "succeeded" => "completed".to_string(),
            "running" => "active".to_string(),
            _ => "limited".to_string(),
        };
    }
    "pending".to_string()
}

fn resolve_task_runbook_status(
    task: &TaskRecord,
    latest_run: Option<&RunRecord>,
    has_waiting_approval: bool,
) -> String {
    if task.status == "blocked" {
        return "blocked".to_string();
    }
    if has_waiting_approval {
        return "waiting".to_string();
    }
    match latest_run {
        Some(run) => match run.status.as_str() {
            "failed" => "failed".to_string(),
            "succeeded" => "completed".to_string(),
            "queued" | "running" | "pending_approval" => "active".to_string(),
            _ => "limited".to_string(),
        },
        None => "pending".to_string(),
    }
}

fn selected_board_card_run(
    state: &AppState,
    card: &BoardCardRecord,
) -> std::result::Result<Option<RunRecord>, ApiErrorResponse> {
    if let Some(run_id) = card.latest_run_id.as_ref() {
        return state
            .storage
            .get_run(run_id)
            .map_err(|err| internal_err_with_error("loading board card latest run failed", err));
    }
    match card.linked_session_id.as_ref() {
        Some(session_id) => state
            .storage
            .latest_run_for_session(session_id)
            .map_err(|err| internal_err_with_error("loading board card session run failed", err)),
        None => Ok(None),
    }
}

fn selected_job_execution(
    state: &AppState,
    job: &JobRecord,
) -> std::result::Result<SelectedJobExecution, ApiErrorResponse> {
    let runs = state
        .storage
        .list_job_runs(&job.job_id, RUNBOOK_JOB_RUN_LIMIT)
        .map_err(|err| internal_err_with_error("listing job runs for runbook failed", err))?;
    let selected_job_run = runs
        .iter()
        .find(|run| run.status == "running")
        .cloned()
        .or_else(|| runs.first().cloned());
    let linked_run = parse_job_session_id(job)
        .map(|session_id| {
            state
                .storage
                .latest_run_for_session(&session_id)
                .map_err(|err| {
                    internal_err_with_error("loading linked session run for job failed", err)
                })
        })
        .transpose()?
        .flatten();
    Ok(SelectedJobExecution {
        job_run: selected_job_run,
        linked_run,
    })
}

fn parse_job_session_id(job: &JobRecord) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).ok()?;
    payload
        .get("session_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn requested_approvals_for_run(caches: &RunbookCaches, run_id: &str) -> Vec<ApprovalRecord> {
    approvals_for_run(caches, run_id)
        .into_iter()
        .filter(|approval| approval.status == "requested")
        .collect()
}

fn approvals_for_run(caches: &RunbookCaches, run_id: &str) -> Vec<ApprovalRecord> {
    caches
        .approvals_by_run_id
        .get(run_id)
        .cloned()
        .unwrap_or_default()
}

fn append_task_context_refs(
    linked_entities: &mut Vec<RunbookEntityRefResponse>,
    linked_task: Option<&TaskRecord>,
    caches: &RunbookCaches,
) {
    let Some(task) = linked_task else {
        return;
    };
    linked_entities.push(task_entity_ref(task));
    if let Some(project) = caches.projects_by_id.get(&task.project_id) {
        linked_entities.push(project_entity_ref(project));
        if let Some(goal) = caches.goals_by_id.get(&project.goal_id) {
            linked_entities.push(goal_entity_ref(goal));
        }
    }
}

fn resolve_active_step_id(steps: &[RunbookStepResponse], status: &str) -> Option<String> {
    let preferred = match status {
        "failed" => "failed",
        "completed" => "completed",
        "waiting" => "waiting",
        "blocked" => "blocked",
        "active" => "active",
        "pending" => "idle",
        "limited" => "limited",
        _ => "idle",
    };
    pick_step_by_state(steps, preferred).or_else(|| {
        [
            "active",
            "waiting",
            "blocked",
            "idle",
            "limited",
            "completed",
            "failed",
        ]
        .iter()
        .find_map(|state| pick_step_by_state(steps, state))
    })
}

fn pick_step_by_state(steps: &[RunbookStepResponse], state: &str) -> Option<String> {
    steps
        .iter()
        .filter(|step| step.state == state)
        .max_by(|left, right| {
            left.template_index
                .cmp(&right.template_index)
                .then_with(|| step_sort_ms(left).cmp(&step_sort_ms(right)))
                .then_with(|| left.step_id.cmp(&right.step_id))
        })
        .map(|step| step.step_id.clone())
}

fn resolve_next_step_ids(
    runbook_kind: &str,
    active_step_id: Option<&str>,
    status: &str,
) -> Vec<String> {
    if matches!(status, "completed" | "failed") {
        return Vec::new();
    }
    let Some(step_id) = active_step_id else {
        return Vec::new();
    };
    let next = match (runbook_kind, step_id) {
        ("assistant_session_run", "session_selected") => vec!["user_input_ready"],
        ("assistant_session_run", "user_input_ready") => vec!["run_created"],
        ("assistant_session_run", "run_created") => vec!["run_executing"],
        ("assistant_session_run", "run_executing") => vec![
            "approval_wait",
            "tool_activity",
            "run_succeeded",
            "run_failed",
        ],
        ("assistant_session_run", "approval_wait") => vec!["run_executing"],
        ("assistant_session_run", "tool_activity") => vec!["run_succeeded", "run_failed"],
        ("board_card_run", "card_ready") => vec!["session_linked"],
        ("board_card_run", "session_linked") => vec!["run_created"],
        ("board_card_run", "run_created") => vec!["run_executing"],
        ("board_card_run", "run_executing") => {
            vec!["approval_wait", "card_run_completed", "card_run_failed"]
        }
        ("board_card_run", "approval_wait") => vec!["run_executing"],
        ("scheduled_job_run", "job_enabled") => vec!["job_due_or_triggered"],
        ("scheduled_job_run", "job_due_or_triggered") => vec!["job_run_started"],
        ("scheduled_job_run", "job_run_started") => vec!["job_processing"],
        ("scheduled_job_run", "job_processing") => {
            vec!["approval_wait", "job_run_succeeded", "job_run_failed"]
        }
        ("scheduled_job_run", "approval_wait") => vec!["job_processing"],
        ("strategy_task_execution", "task_defined") => vec!["execution_linked"],
        ("strategy_task_execution", "execution_linked") => vec!["execution_active"],
        ("strategy_task_execution", "execution_active") => vec![
            "approval_wait",
            "blocked",
            "execution_completed",
            "execution_failed",
        ],
        ("strategy_task_execution", "approval_wait") => vec!["execution_active"],
        _ => Vec::new(),
    };
    next.into_iter().map(|item| item.to_string()).collect()
}

fn step_sort_ms(step: &RunbookStepResponse) -> i64 {
    step.finished_at_ms
        .or(step.waiting_since_ms)
        .or(step.started_at_ms)
        .unwrap_or_default()
}

fn ready_availability(now_ms: i64) -> RunbookDataAvailabilityResponse {
    RunbookDataAvailabilityResponse {
        is_limited: false,
        is_stale: false,
        last_refresh_at_ms: now_ms,
        missing_source_kinds: Vec::new(),
        stale_reason: None,
    }
}

fn limited_availability(
    now_ms: i64,
    missing_source_kinds: Vec<String>,
) -> RunbookDataAvailabilityResponse {
    RunbookDataAvailabilityResponse {
        is_limited: true,
        is_stale: false,
        last_refresh_at_ms: now_ms,
        missing_source_kinds,
        stale_reason: None,
    }
}

fn summary_step_label(status: &str, labels: [&str; 7]) -> String {
    match status {
        "pending" => labels[0].to_string(),
        "active" => labels[1].to_string(),
        "waiting" => labels[2].to_string(),
        "blocked" => labels[3].to_string(),
        "completed" => labels[4].to_string(),
        "failed" => labels[5].to_string(),
        "limited" => labels[6].to_string(),
        _ => labels[0].to_string(),
    }
}

fn assistant_run_title(session: &SessionRecord) -> String {
    session
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| format!("Assistant run {}", session.session_key))
}

fn open_entity_action(
    action_id: &str,
    label: &str,
    target_entity_ref: RunbookEntityRefResponse,
) -> RunbookActionResponse {
    RunbookActionResponse {
        action_id: action_id.to_string(),
        action_kind: "open_entity".to_string(),
        label: label.to_string(),
        availability: "enabled".to_string(),
        disabled_reason: None,
        target_entity_ref: Some(target_entity_ref),
    }
}

fn source_fact(
    fact_id: &str,
    fact_kind: &str,
    entity_ref: Option<RunbookEntityRefResponse>,
    occurred_at_ms: Option<i64>,
    partial: bool,
) -> RunbookSourceFactResponse {
    RunbookSourceFactResponse {
        fact_id: fact_id.to_string(),
        fact_kind: fact_kind.to_string(),
        entity_ref,
        occurred_at_ms,
        partial,
    }
}

fn history_item(
    history_id: &str,
    event_kind: &str,
    label: impl Into<String>,
    detail: Option<String>,
    occurred_at_ms: i64,
    step_id: Option<&str>,
    entity_refs: Vec<RunbookEntityRefResponse>,
) -> RunbookHistoryItemResponse {
    RunbookHistoryItemResponse {
        history_id: history_id.to_string(),
        event_kind: event_kind.to_string(),
        label: label.into(),
        detail,
        occurred_at_ms,
        step_id: step_id.map(|value| value.to_string()),
        entity_refs,
    }
}

fn warning(warning_id: &str, warning_kind: &str, message: &str) -> RunbookWarningResponse {
    RunbookWarningResponse {
        warning_id: warning_id.to_string(),
        warning_kind: warning_kind.to_string(),
        message: message.to_string(),
    }
}

fn step(
    descriptor: (&str, &str, &str),
    state: &str,
    state_reason: Option<String>,
    timing: (Option<i64>, Option<i64>, Option<i64>),
    linked_entity_refs: Vec<RunbookEntityRefResponse>,
    action_refs: Vec<String>,
    template_index: u32,
) -> RunbookStepResponse {
    let (step_id, label, kind) = descriptor;
    let (started_at_ms, finished_at_ms, waiting_since_ms) = timing;
    RunbookStepResponse {
        step_id: step_id.to_string(),
        label: label.to_string(),
        kind: kind.to_string(),
        state: state.to_string(),
        state_reason,
        started_at_ms,
        finished_at_ms,
        waiting_since_ms,
        linked_entity_refs,
        action_refs,
        template_index,
    }
}

fn truncate_detail(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() > 160 {
        format!("{}...", &trimmed[..160])
    } else {
        trimmed.to_string()
    }
}

fn approval_history_label(approval: &ApprovalRecord) -> String {
    match approval.status.as_str() {
        "requested" => "Approval requested".to_string(),
        "approved" => "Approval approved".to_string(),
        "rejected" => "Approval rejected".to_string(),
        other => format!("Approval {other}"),
    }
}

fn runbook_filter_key(query: &ListRunbooksQuery) -> String {
    [
        query.kind.as_deref().unwrap_or(""),
        query.status.as_deref().unwrap_or(""),
        query.owner_agent_id.as_deref().unwrap_or(""),
        query.query.as_deref().unwrap_or(""),
        query.linked_task_id.as_deref().unwrap_or(""),
        query.linked_project_id.as_deref().unwrap_or(""),
        query.linked_goal_id.as_deref().unwrap_or(""),
    ]
    .join("|")
}

fn decode_list_cursor(
    cursor: Option<&str>,
    filter_key: &str,
) -> std::result::Result<Option<RunbookListCursor>, ApiErrorResponse> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let payload = URL_SAFE_NO_PAD
        .decode(cursor.as_bytes())
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid_cursor"))?;
    let decoded: RunbookListCursor = serde_json::from_slice(&payload)
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "invalid_cursor"))?;
    if decoded.filter_key != filter_key {
        return Err(api_error(StatusCode::BAD_REQUEST, "invalid_cursor"));
    }
    Ok(Some(decoded))
}

fn encode_list_cursor(item: &RunbookSummaryItemResponse, filter_key: &str) -> String {
    let payload = RunbookListCursor {
        filter_key: filter_key.to_string(),
        updated_at_ms: item.updated_at_ms,
        status_rank: status_rank(&item.status),
        title: item.title.clone(),
        runbook_id: item.runbook_id.clone(),
    };
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap_or_default())
}

fn matches_cursor(item: &RunbookSummaryItemResponse, cursor: &RunbookListCursor) -> bool {
    item.updated_at_ms == cursor.updated_at_ms
        && status_rank(&item.status) == cursor.status_rank
        && item.title == cursor.title
        && item.runbook_id == cursor.runbook_id
}

fn status_rank(status: &str) -> u8 {
    match status {
        "failed" => 0,
        "waiting" => 1,
        "blocked" => 2,
        "active" => 3,
        "completed" => 4,
        "pending" => 5,
        "limited" => 6,
        _ => 7,
    }
}

fn run_sort_ms(run: &RunRecord) -> i64 {
    run.ended_at.or(run.started_at).unwrap_or(run.created_at)
}

fn job_run_sort_ms(run: &JobRunRecord) -> i64 {
    run.ended_at.or(run.started_at).unwrap_or(run.created_at)
}

fn anchor_kind_for_runbook(kind: &str) -> &str {
    match kind {
        "assistant_session_run" => "run",
        "board_card_run" => "card",
        "scheduled_job_run" => "job",
        "strategy_task_execution" => "task",
        _ => "unknown",
    }
}

fn deep_link(
    tab: &str,
    target_kind: &str,
    target_id: Option<String>,
    context: Option<&str>,
) -> RunbookDeepLinkTargetResponse {
    RunbookDeepLinkTargetResponse {
        tab: tab.to_string(),
        target_kind: target_kind.to_string(),
        target_id,
        context: context.map(|value| value.to_string()),
    }
}

fn session_entity_ref(session: &SessionRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "session".to_string(),
        entity_id: session.session_id.clone(),
        display_label: session
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| session.session_key.clone()),
        deep_link: deep_link(
            "assistant",
            "session",
            Some(session.session_id.clone()),
            None,
        ),
    }
}

fn session_link_entity_ref(session_id: &str) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "session".to_string(),
        entity_id: session_id.to_string(),
        display_label: session_id.to_string(),
        deep_link: deep_link("assistant", "session", Some(session_id.to_string()), None),
    }
}

fn run_entity_ref(run: &RunRecord, tab: &str) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "run".to_string(),
        entity_id: run.run_id.clone(),
        display_label: format!("Run {}", run.run_id),
        deep_link: deep_link(tab, "run", Some(run.run_id.clone()), None),
    }
}

fn approval_entity_ref(approval: &ApprovalRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "approval".to_string(),
        entity_id: approval.approval_id.clone(),
        display_label: approval.request_summary.clone(),
        deep_link: deep_link(
            "focus",
            "approval",
            Some(approval.approval_id.clone()),
            None,
        ),
    }
}

fn tool_call_entity_ref(tool_call: &ToolCallRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "tool_call".to_string(),
        entity_id: tool_call.tool_call_id.clone(),
        display_label: tool_call.tool_name.clone(),
        deep_link: deep_link(
            "assistant",
            "run",
            Some(tool_call.run_id.clone()),
            Some("tool_call"),
        ),
    }
}

fn message_entity_ref(message: &MessageRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "message".to_string(),
        entity_id: message.message_id.clone(),
        display_label: truncate_detail(&message.content_text),
        deep_link: deep_link(
            "assistant",
            "session",
            Some(message.session_id.clone()),
            Some("message"),
        ),
    }
}

fn board_card_entity_ref(card: &BoardCardRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "card".to_string(),
        entity_id: card.card_id.clone(),
        display_label: card.title.clone(),
        deep_link: deep_link("boards", "card", Some(card.card_id.clone()), None),
    }
}

fn job_entity_ref(job: &JobRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "job".to_string(),
        entity_id: job.job_id.clone(),
        display_label: job.name.clone(),
        deep_link: deep_link("calendar", "job", Some(job.job_id.clone()), None),
    }
}

fn job_run_entity_ref(run: &JobRunRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "job_run".to_string(),
        entity_id: run.job_run_id.clone(),
        display_label: format!("Job run {}", run.job_run_id),
        deep_link: deep_link("calendar", "job_run", Some(run.job_run_id.clone()), None),
    }
}

fn task_entity_ref(task: &TaskRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "task".to_string(),
        entity_id: task.task_id.clone(),
        display_label: task.title.clone(),
        deep_link: deep_link("strategy", "task", Some(task.task_id.clone()), None),
    }
}

fn project_entity_ref(project: &ProjectRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "project".to_string(),
        entity_id: project.project_id.clone(),
        display_label: project.name.clone(),
        deep_link: deep_link(
            "strategy",
            "project",
            Some(project.project_id.clone()),
            None,
        ),
    }
}

fn goal_entity_ref(goal: &GoalRecord) -> RunbookEntityRefResponse {
    RunbookEntityRefResponse {
        entity_kind: "goal".to_string(),
        entity_id: goal.goal_id.clone(),
        display_label: goal.title.clone(),
        deep_link: deep_link("strategy", "goal", Some(goal.goal_id.clone()), None),
    }
}
