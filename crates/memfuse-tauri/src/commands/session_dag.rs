use crate::state::AppState;
use memfuse_core::{MemFuseErrorDto, StorageEngine};
use memfuse_graph::SessionBranchTree;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BranchDto {
    pub branch_id: String,
    pub head_node_id: String,
    pub parent_node_id: Option<String>,
    pub label: String,
    pub is_active: bool,
    pub root_prompt_preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStateNodeDto {
    pub step_id: String,
    pub prompt: String,
    pub response: String,
    pub snapshot_tx_id: Option<u64>,
    pub tool_outputs: Vec<String>,
    pub compacted: bool,
}

impl From<&memfuse_graph::AgentStateNode> for AgentStateNodeDto {
    fn from(node: &memfuse_graph::AgentStateNode) -> Self {
        Self {
            step_id: node.step_id.to_string(),
            prompt: node.prompt.clone(),
            response: node.response.clone(),
            snapshot_tx_id: node.snapshot_tx_id.map(|tx| tx.inner()),
            tool_outputs: node.tool_outputs.clone(),
            compacted: node.compacted,
        }
    }
}

pub async fn get_or_load_session(
    state: &AppState,
    session_id: &str,
) -> Result<Arc<SessionBranchTree>, MemFuseErrorDto> {
    if session_id.trim().is_empty() {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            "Session ID cannot be empty",
        ));
    }

    {
        let guard = state.sessions.read();
        if let Some(tree) = guard.get(session_id) {
            return Ok(Arc::clone(tree));
        }
    }

    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned()
    };

    if let Some(db) = db {
        match SessionBranchTree::load(db.inner_storage().as_ref(), session_id).await {
            Ok(tree) => {
                let tree_arc = Arc::new(tree);
                state
                    .sessions
                    .write()
                    .insert(session_id.to_string(), Arc::clone(&tree_arc));
                Ok(tree_arc)
            }
            Err(_) => Err(MemFuseErrorDto::new(
                "NotFound",
                format!("Session '{session_id}' not found"),
            )),
        }
    } else {
        Err(MemFuseErrorDto::new(
            "NotFound",
            format!("Session '{session_id}' not found"),
        ))
    }
}

pub async fn get_or_create_session(
    state: &AppState,
    session_id: &str,
    initial_prompt: &str,
    initial_response: &str,
) -> Result<Arc<SessionBranchTree>, MemFuseErrorDto> {
    if session_id.trim().is_empty() {
        return Err(MemFuseErrorDto::new(
            "InvalidInput",
            "Session ID cannot be empty",
        ));
    }

    if let Ok(existing) = get_or_load_session(state, session_id).await {
        return Ok(existing);
    }

    let tree = Arc::new(SessionBranchTree::new(
        initial_prompt.to_string(),
        initial_response.to_string(),
    ));

    state
        .sessions
        .write()
        .insert(session_id.to_string(), Arc::clone(&tree));

    save_session_if_db_open(state, session_id, &tree).await?;

    Ok(tree)
}

pub async fn save_session_if_db_open(
    state: &AppState,
    session_id: &str,
    tree: &SessionBranchTree,
) -> Result<(), MemFuseErrorDto> {
    let db = {
        let db_guard = state.db.read();
        db_guard.as_ref().cloned()
    };

    if let Some(db) = db {
        let tx = db.allocate_tx().map_err(|e| MemFuseErrorDto::from(&e))?;
        tree.save(db.inner_storage().as_ref(), session_id, tx)
            .await
            .map_err(|e| MemFuseErrorDto::from(&e))?;
        db.inner_storage()
            .commit(tx)
            .await
            .map_err(|e| MemFuseErrorDto::from(&e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn list_branches(
    state: State<'_, AppState>,
    session_id: String,
) -> Result<Vec<BranchDto>, MemFuseErrorDto> {
    let tree = get_or_load_session(&state, &session_id).await?;

    let nodes_guard = tree.lock_nodes();
    let nodes = nodes_guard.nodes();
    let edges = nodes_guard.edges();
    let active_head = *nodes_guard.active_head();

    let parent_ids: std::collections::HashSet<u64> = edges.iter().map(|e| e.parent).collect();

    let mut branch_heads: Vec<u64> = nodes
        .keys()
        .copied()
        .filter(|id| !parent_ids.contains(id))
        .collect();

    if !branch_heads.contains(&active_head) && nodes.contains_key(&active_head) {
        branch_heads.push(active_head);
    }

    branch_heads.sort();

    let mut branches = Vec::new();
    for head_id in branch_heads {
        let incoming_edge = edges.iter().rev().find(|e| e.child == head_id);

        let (parent_node_id, label) = match incoming_edge {
            Some(edge) => (Some(edge.parent.to_string()), edge.label.clone()),
            None => (None, "main".to_string()),
        };

        let node_prompt = nodes
            .get(&head_id)
            .map(|n| n.prompt.chars().take(60).collect::<String>())
            .unwrap_or_default();

        branches.push(BranchDto {
            branch_id: head_id.to_string(),
            head_node_id: head_id.to_string(),
            parent_node_id,
            label,
            is_active: head_id == active_head,
            root_prompt_preview: node_prompt,
        });
    }

    Ok(branches)
}

#[tauri::command]
pub async fn create_branch(
    state: State<'_, AppState>,
    session_id: String,
    from_node_id: String,
    label: Option<String>,
) -> Result<BranchDto, MemFuseErrorDto> {
    let from_id = from_node_id.parse::<u64>().map_err(|_| {
        MemFuseErrorDto::new(
            "InvalidInput",
            format!("Invalid from_node_id: '{from_node_id}'"),
        )
    })?;

    let tree = get_or_load_session(&state, &session_id).await?;

    let branch_label = label
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("branch");

    let prompt = format!("Branch ab Schritt {from_id}");
    let response = "Branch erstellt. Setze die Konversation fort...".to_string();

    let new_node_id = tree
        .branch_from(
            from_id,
            prompt.clone(),
            response,
            None,
            Vec::new(),
            branch_label,
        )
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    tree.set_active_head(new_node_id)
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    save_session_if_db_open(&state, &session_id, &tree).await?;

    Ok(BranchDto {
        branch_id: new_node_id.to_string(),
        head_node_id: new_node_id.to_string(),
        parent_node_id: Some(from_id.to_string()),
        label: branch_label.to_string(),
        is_active: true,
        root_prompt_preview: prompt,
    })
}

#[tauri::command]
pub async fn switch_branch(
    state: State<'_, AppState>,
    session_id: String,
    branch_id: String,
) -> Result<(), MemFuseErrorDto> {
    let node_id = branch_id.parse::<u64>().map_err(|_| {
        MemFuseErrorDto::new("InvalidInput", format!("Invalid branch_id: '{branch_id}'"))
    })?;

    let tree = get_or_load_session(&state, &session_id).await?;

    tree.set_active_head(node_id)
        .map_err(|e| MemFuseErrorDto::from(&e))?;

    save_session_if_db_open(&state, &session_id, &tree).await?;

    Ok(())
}

#[tauri::command]
pub async fn get_branch_history(
    state: State<'_, AppState>,
    session_id: String,
    branch_id: String,
) -> Result<Vec<AgentStateNodeDto>, MemFuseErrorDto> {
    let target_node_id = branch_id.parse::<u64>().map_err(|_| {
        MemFuseErrorDto::new("InvalidInput", format!("Invalid branch_id: '{branch_id}'"))
    })?;

    let tree = get_or_load_session(&state, &session_id).await?;

    let nodes_guard = tree.lock_nodes();
    let nodes = nodes_guard.nodes();
    let edges = nodes_guard.edges();

    if !nodes.contains_key(&target_node_id) {
        return Err(MemFuseErrorDto::new(
            "NotFound",
            format!("Node '{branch_id}' nicht in Session '{session_id}' gefunden"),
        ));
    }

    let mut path = Vec::new();
    let mut current = target_node_id;

    loop {
        if let Some(node) = nodes.get(&current) {
            path.push(AgentStateNodeDto::from(node));
        }
        if let Some(edge) = edges.iter().rev().find(|e| e.child == current) {
            current = edge.parent;
        } else {
            break;
        }
    }

    path.reverse();
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_branches_non_existent_session_fails(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();
        let state_ref: State<'_, AppState> = unsafe { std::mem::transmute(&state) };

        let res = list_branches(state_ref, "non_existent_sess".to_string()).await;
        assert!(res.is_err());
        let err = res.err().ok_or("Expected error")?;
        assert_eq!(err.kind, "NotFound");
        assert!(err.message.contains("non_existent_sess"));
        Ok(())
    }

    #[tokio::test]
    async fn test_create_and_list_branches_success() -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();

        let session = get_or_create_session(&state, "sess1", "Root Prompt", "Root Answer").await?;
        assert_eq!(session.node_count(), 1);

        let state_ref1: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let branches = list_branches(state_ref1, "sess1".to_string()).await?;
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].branch_id, "0");
        assert_eq!(branches[0].head_node_id, "0");
        assert_eq!(branches[0].parent_node_id, None);
        assert_eq!(branches[0].label, "main");
        assert!(branches[0].is_active);

        let state_ref2: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let new_branch1 = create_branch(
            state_ref2,
            "sess1".to_string(),
            "0".to_string(),
            Some("explore_side_topic_1".to_string()),
        )
        .await?;

        assert_eq!(new_branch1.branch_id, "1");
        assert_eq!(new_branch1.head_node_id, "1");
        assert_eq!(new_branch1.parent_node_id, Some("0".to_string()));
        assert_eq!(new_branch1.label, "explore_side_topic_1");
        assert!(new_branch1.is_active);

        let state_ref3: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let _new_branch2 = create_branch(
            state_ref3,
            "sess1".to_string(),
            "0".to_string(),
            Some("explore_side_topic_2".to_string()),
        )
        .await?;

        let state_ref4: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let branches_after = list_branches(state_ref4, "sess1".to_string()).await?;
        assert_eq!(branches_after.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_switch_branch_and_history() -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();

        let tree = get_or_create_session(&state, "sess2", "Root Q", "Root A").await?;
        let step1 = tree.append_step(
            "Step 1 Q".to_string(),
            "Step 1 A".to_string(),
            None,
            Vec::new(),
            "main",
        )?;
        let step2 = tree.append_step(
            "Step 2 Q".to_string(),
            "Step 2 A".to_string(),
            None,
            Vec::new(),
            "main",
        )?;
        assert_eq!(step1, 1);
        assert_eq!(step2, 2);

        let state_ref1: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let branch = create_branch(
            state_ref1,
            "sess2".to_string(),
            "1".to_string(),
            Some("alt_branch".to_string()),
        )
        .await?;
        assert_eq!(branch.branch_id, "3");

        let state_ref2: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let history_branch3 =
            get_branch_history(state_ref2, "sess2".to_string(), "3".to_string()).await?;
        assert_eq!(history_branch3.len(), 3);
        assert_eq!(history_branch3[0].step_id, "0");
        assert_eq!(history_branch3[1].step_id, "1");
        assert_eq!(history_branch3[2].step_id, "3");

        let state_ref3: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        switch_branch(state_ref3, "sess2".to_string(), "2".to_string()).await?;

        let state_ref4: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let history_branch2 =
            get_branch_history(state_ref4, "sess2".to_string(), "2".to_string()).await?;
        assert_eq!(history_branch2.len(), 3);
        assert_eq!(history_branch2[0].step_id, "0");
        assert_eq!(history_branch2[1].step_id, "1");
        assert_eq!(history_branch2[2].step_id, "2");

        Ok(())
    }

    #[tokio::test]
    async fn test_invalid_node_id_or_branch_id_error_handling(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let state = AppState::new();

        let _ = get_or_create_session(&state, "sess3", "Root", "Ans").await?;

        let state_ref1: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let err1 = create_branch(
            state_ref1,
            "sess3".to_string(),
            "invalid_node".to_string(),
            None,
        )
        .await
        .err()
        .ok_or("Expected error")?;
        assert_eq!(err1.kind, "InvalidInput");

        let state_ref2: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let err2 = switch_branch(state_ref2, "sess3".to_string(), "not_a_number".to_string())
            .await
            .err()
            .ok_or("Expected error")?;
        assert_eq!(err2.kind, "InvalidInput");

        let state_ref3: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let err3 = switch_branch(state_ref3, "sess3".to_string(), "999".to_string())
            .await
            .err()
            .ok_or("Expected error")?;
        assert_eq!(err3.kind, "InvalidInput");

        let state_ref4: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let err4 = get_branch_history(state_ref4, "sess3".to_string(), "999".to_string())
            .await
            .err()
            .ok_or("Expected error")?;
        assert_eq!(err4.kind, "NotFound");

        Ok(())
    }

    #[tokio::test]
    async fn test_persistence_survives_db_reopen() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().to_path_buf();

        let db = memfuse_db::MemFuse::open_with_config(
            &path,
            memfuse_db::MemFuseConfig {
                dimension: 4,
                ..Default::default()
            },
        )
        .await?;

        let state = AppState::new();
        *state.db.write() = Some(Arc::new(db));

        let _ = get_or_create_session(&state, "persisted_sess", "Root Q", "Root A").await?;
        let state_ref1: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let _ = create_branch(
            state_ref1,
            "persisted_sess".to_string(),
            "0".to_string(),
            Some("b1".to_string()),
        )
        .await?;
        let state_ref2: State<'_, AppState> = unsafe { std::mem::transmute(&state) };
        let _ = create_branch(
            state_ref2,
            "persisted_sess".to_string(),
            "0".to_string(),
            Some("b2".to_string()),
        )
        .await?;

        // Simulate app restart with new AppState pointing to same DB path
        let db2 = memfuse_db::MemFuse::open_with_config(
            &path,
            memfuse_db::MemFuseConfig {
                dimension: 4,
                ..Default::default()
            },
        )
        .await?;

        let new_state = AppState::new();
        *new_state.db.write() = Some(Arc::new(db2));
        let new_state_ref1: State<'_, AppState> = unsafe { std::mem::transmute(&new_state) };

        let branches = list_branches(new_state_ref1, "persisted_sess".to_string()).await?;
        assert_eq!(branches.len(), 2);

        let new_state_ref2: State<'_, AppState> = unsafe { std::mem::transmute(&new_state) };
        let history = get_branch_history(
            new_state_ref2,
            "persisted_sess".to_string(),
            "1".to_string(),
        )
        .await?;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].step_id, "0");
        assert_eq!(history[1].step_id, "1");

        Ok(())
    }
}
