use crate::model::{
    AgentSession, ContextUsage, Message, ProviderBrand, RenameSessionInput, SessionStatus,
    UpdateSessionOptionsInput,
};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_SESSION_TITLE_CHARS: usize = 80;

pub fn normalized_session_title(value: &str) -> Result<String, String> {
    let title = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("Choose a name for this session".to_string());
    }
    if title.chars().count() > MAX_SESSION_TITLE_CHARS {
        return Err(format!(
            "Session names can contain at most {MAX_SESSION_TITLE_CHARS} characters"
        ));
    }
    if title.chars().any(char::is_control) {
        return Err("The session name contains unsupported characters".to_string());
    }
    Ok(title)
}

/// Creation accepts an unnamed session: the UI titles it from the first prompt,
/// and a client that sends nothing at all still gets a usable name. Renaming
/// stays strict, because clearing a name there is a mistake, not a default.
pub fn session_title_or_default(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        return Ok("New session".to_string());
    }
    normalized_session_title(value)
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    version: u32,
    sessions: Vec<AgentSession>,
}

fn deserialize_state(value: &str) -> Result<PersistedState, serde_json::Error> {
    let mut value = serde_json::from_str::<serde_json::Value>(value)?;
    if let Some(sessions) = value
        .get_mut("sessions")
        .and_then(|value| value.as_array_mut())
    {
        for session in sessions {
            if session.get("reasoning").and_then(|value| value.as_str()) == Some("ultracode") {
                session["reasoning"] = serde_json::Value::String("xhigh".to_string());
            }
        }
    }
    serde_json::from_value(value)
}

pub struct SessionStore {
    path: PathBuf,
    state: RwLock<PersistedState>,
}

impl SessionStore {
    pub fn load(data_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
        let path = data_dir.join("sessions.json");
        let mut state = match fs::read_to_string(&path) {
            Ok(value) => match deserialize_state(&value) {
                Ok(state) => state,
                Err(error) => {
                    let backup =
                        data_dir.join(format!("sessions.corrupt-{}.json", Utc::now().timestamp()));
                    fs::rename(&path, &backup).map_err(|rename_error| {
                        format!(
                            "Session data is invalid ({error}) and could not be preserved: {rename_error}"
                        )
                    })?;
                    PersistedState::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => PersistedState::default(),
            Err(error) => return Err(error.to_string()),
        };
        let previous_version = state.version;
        state.version = 3;
        for session in &mut state.sessions {
            if previous_version < 2 {
                session.provider_brand = ProviderBrand::for_provider(session.provider);
            }
            if session.status != SessionStatus::Idle {
                session.status = SessionStatus::Idle;
            }
        }
        let store = Self {
            path,
            state: RwLock::new(state),
        };
        store.persist()?;
        Ok(store)
    }

    pub fn list(&self) -> Vec<AgentSession> {
        let mut sessions = self.state.read().sessions.clone();
        sessions.sort_by_key(|session| Reverse(session.updated_at));
        sessions
    }

    pub fn get(&self, id: &str) -> Option<AgentSession> {
        self.state
            .read()
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
    }

    pub fn insert(&self, session: AgentSession) -> Result<AgentSession, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        next.sessions.push(session.clone());
        self.persist_state(&next)?;
        *state = next;
        Ok(session)
    }

    pub fn remove(&self, id: &str) -> Result<bool, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        let before = next.sessions.len();
        next.sessions.retain(|session| session.id != id);
        let removed = before != next.sessions.len();
        if removed {
            self.persist_state(&next)?;
            *state = next;
        }
        Ok(removed)
    }

    pub fn update_options(&self, input: UpdateSessionOptionsInput) -> Result<AgentSession, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        let session = next
            .sessions
            .iter_mut()
            .find(|session| session.id == input.session_id)
            .ok_or_else(|| "Session not found".to_string())?;
        if matches!(
            session.status,
            SessionStatus::Running | SessionStatus::WaitingApproval
        ) {
            return Err("Stop the running turn before changing its runtime options".into());
        }
        if session.provider != input.provider && !session.messages.is_empty() {
            return Err(
                "A session keeps its original CLI runtime. Create a new session to switch provider."
                    .into(),
            );
        }
        let connection_changed = session.provider != input.provider;
        session.provider = input.provider;
        session.provider_brand = input.provider_brand;
        session.model = input.model;
        session.reasoning = input.reasoning;
        session.speed_mode = input.speed_mode;
        session.interaction_mode = input.interaction_mode;
        session.access_mode = input.access_mode;
        session.updated_at = Utc::now();
        if connection_changed {
            session.provider_session_id = None;
            session.context_usage = None;
        }
        let result = session.clone();
        self.persist_state(&next)?;
        *state = next;
        Ok(result)
    }

    pub fn rename(&self, input: RenameSessionInput) -> Result<AgentSession, String> {
        let title = normalized_session_title(&input.title)?;
        let mut state = self.state.write();
        let mut next = state.clone();
        let session = next
            .sessions
            .iter_mut()
            .find(|session| session.id == input.session_id)
            .ok_or_else(|| "Session not found".to_string())?;
        session.title = title;
        session.updated_at = Utc::now();
        let result = session.clone();
        self.persist_state(&next)?;
        *state = next;
        Ok(result)
    }

    pub fn begin_turn(&self, id: &str, message: Message) -> Result<AgentSession, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        let session = next
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .ok_or_else(|| "Session not found".to_string())?;
        if session.status != SessionStatus::Idle && session.status != SessionStatus::Failed {
            return Err("This session already has a running turn".to_string());
        }
        session.messages.push(message);
        session.status = SessionStatus::Running;
        session.updated_at = Utc::now();
        let result = session.clone();
        self.persist_state(&next)?;
        *state = next;
        Ok(result)
    }

    /// Records a steering message the user sent while a turn was running.
    /// The session stays in its running state; only the transcript grows.
    pub fn append_user_message(&self, id: &str, message: Message) -> Result<AgentSession, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        let session = next
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .ok_or_else(|| "Session not found".to_string())?;
        if session.status != SessionStatus::Running {
            return Err("This session has no running turn to steer".to_string());
        }
        session.messages.push(message);
        session.updated_at = Utc::now();
        let result = session.clone();
        self.persist_state(&next)?;
        *state = next;
        Ok(result)
    }

    pub fn finish_turn(
        &self,
        id: &str,
        activities: Vec<Message>,
        message: Message,
        provider_session_id: Option<String>,
        context_usage: Option<ContextUsage>,
        failed: bool,
    ) -> Result<AgentSession, String> {
        let mut state = self.state.write();
        let mut next = state.clone();
        let session = next
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .ok_or_else(|| "Session not found".to_string())?;
        session.messages.extend(activities);
        if !message.content.trim().is_empty() {
            session.messages.push(message);
        }
        // Activities reach the store only when the turn ends, while a steering
        // message is stored the moment it is sent. Ordering by creation time
        // keeps a steered prompt where it appeared while the turn streamed,
        // instead of jumping above the activity that preceded it. Messages are
        // already created in order, so this is a no-op for every other turn.
        session.messages.sort_by_key(|message| message.created_at);
        if let Some(provider_session_id) = provider_session_id {
            session.provider_session_id = Some(provider_session_id);
        }
        if let Some(context_usage) = context_usage {
            session.context_usage = Some(context_usage);
        }
        session.status = if failed {
            SessionStatus::Failed
        } else {
            SessionStatus::Idle
        };
        session.updated_at = Utc::now();
        let result = session.clone();
        // A completed provider turn must never remain "running" in memory just
        // because the final disk flush failed. Keep the recoverable in-memory
        // snapshot so the UI can stop its progress state and retry persistence
        // on the next mutation or launch.
        let persistence = self.persist_state(&next);
        *state = next;
        persistence?;
        Ok(result)
    }

    fn persist(&self) -> Result<(), String> {
        self.persist_state(&self.state.read())
    }

    fn persist_state(&self, state: &PersistedState) -> Result<(), String> {
        let value = serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        let mut file = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&value).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        #[cfg(windows)]
        if self.path.exists() {
            fs::remove_file(&self.path).map_err(|error| error.to_string())?;
        }
        fs::rename(&temporary, &self.path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PersistedState, SessionStore, deserialize_state, normalized_session_title,
        session_title_or_default,
    };
    use crate::model::{
        AccessMode, AgentSession, InteractionMode, Message, MessageKind, MessageRole,
        ProviderBrand, ProviderId, ReasoningEffort, RenameSessionInput, SessionStatus, SpeedMode,
        UpdateSessionOptionsInput,
    };
    use chrono::Utc;
    use parking_lot::RwLock;
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    fn test_session(id: String) -> AgentSession {
        let now = Utc::now();
        AgentSession {
            id,
            title: "Original session".to_string(),
            provider: ProviderId::Codex,
            provider_brand: ProviderBrand::Openai,
            model: Some("gpt-5".to_string()),
            reasoning: None,
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
            workspace: PathBuf::from("."),
            provider_session_id: Some("provider-continuation".to_string()),
            status: SessionStatus::Idle,
            messages: Vec::new(),
            context_usage: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn legacy_non_native_ultracode_value_migrates_to_xhigh() {
        let mut value = serde_json::to_value(PersistedState {
            version: 2,
            sessions: vec![test_session("legacy".to_string())],
        })
        .expect("state should serialize");
        value["sessions"][0]["reasoning"] = serde_json::Value::String("ultracode".to_string());

        let state = deserialize_state(&value.to_string()).expect("legacy state should migrate");
        assert_eq!(state.sessions[0].reasoning, Some(ReasoningEffort::Xhigh));
    }

    #[test]
    fn session_title_is_required_compact_and_bounded() {
        assert_eq!(
            normalized_session_title("  hello\n   world  ").unwrap(),
            "hello world"
        );
        assert!(normalized_session_title("   ").is_err());
        assert!(normalized_session_title(&"x".repeat(81)).is_err());
    }

    #[test]
    fn creating_a_session_without_a_name_falls_back_to_a_default() {
        assert_eq!(session_title_or_default("   ").unwrap(), "New session");
        assert_eq!(
            session_title_or_default(" Fix  the parser ").unwrap(),
            "Fix the parser",
        );
        assert!(session_title_or_default(&"x".repeat(81)).is_err());
    }

    #[test]
    fn completed_turn_exits_running_state_when_disk_flush_fails() {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let unavailable_parent =
            std::env::temp_dir().join(format!("onyx-missing-{}", Uuid::new_v4()));
        let store = SessionStore {
            path: unavailable_parent.join("sessions.json"),
            state: RwLock::new(PersistedState {
                version: 2,
                sessions: vec![AgentSession {
                    id: id.clone(),
                    title: "Test".to_string(),
                    provider: ProviderId::Codex,
                    provider_brand: ProviderBrand::Openai,
                    model: None,
                    reasoning: None,
                    speed_mode: SpeedMode::Standard,
                    interaction_mode: InteractionMode::Build,
                    access_mode: AccessMode::ApprovalRequired,
                    workspace: PathBuf::from("."),
                    provider_session_id: None,
                    status: SessionStatus::Running,
                    messages: Vec::new(),
                    context_usage: None,
                    created_at: now,
                    updated_at: now,
                }],
            }),
        };

        let result = store.finish_turn(
            &id,
            Vec::new(),
            Message::new(MessageRole::Assistant, MessageKind::Text, "done"),
            None,
            None,
            false,
        );

        assert!(result.is_err());
        let recovered = store.get(&id).expect("session should remain in memory");
        assert_eq!(recovered.status, SessionStatus::Idle);
        assert_eq!(
            recovered
                .messages
                .last()
                .map(|message| message.content.as_str()),
            Some("done")
        );
    }

    #[test]
    fn removing_a_session_is_persisted_across_reload() {
        let data_dir = std::env::temp_dir().join(format!("onyx-session-remove-{}", Uuid::new_v4()));
        let id = Uuid::new_v4().to_string();
        let store = SessionStore::load(&data_dir).expect("store should load");
        store
            .insert(test_session(id.clone()))
            .expect("session should persist");

        assert!(store.remove(&id).expect("session should be removed"));
        assert!(store.get(&id).is_none());
        drop(store);

        let restored = SessionStore::load(&data_dir).expect("store should reload");
        assert!(restored.get(&id).is_none());
        fs::remove_dir_all(data_dir).expect("temporary store should be removable");
    }

    #[test]
    fn a_steered_prompt_keeps_its_place_when_the_turn_finishes() {
        let data_dir = std::env::temp_dir().join(format!("onyx-session-steer-{}", Uuid::new_v4()));
        let id = Uuid::new_v4().to_string();
        let store = SessionStore::load(&data_dir).expect("store should load");
        let mut session = test_session(id.clone());
        session.messages.clear();
        store.insert(session).expect("session should persist");

        let started = Utc::now();
        let mut prompt = Message::new(MessageRole::User, MessageKind::Text, "build it");
        prompt.created_at = started;
        store.begin_turn(&id, prompt).expect("turn should start");

        // One activity happens, then the user steers, then another activity.
        let mut first = Message::new(MessageRole::Tool, MessageKind::Tool, "Running Bash");
        first.created_at = started + chrono::Duration::seconds(1);
        let mut steer = Message::new(MessageRole::User, MessageKind::Text, "also add tests");
        steer.created_at = started + chrono::Duration::seconds(2);
        let mut second = Message::new(MessageRole::Tool, MessageKind::Tool, "Running Edit");
        second.created_at = started + chrono::Duration::seconds(3);
        let mut reply = Message::new(MessageRole::Assistant, MessageKind::Text, "done");
        reply.created_at = started + chrono::Duration::seconds(4);

        store
            .append_user_message(&id, steer)
            .expect("steering should be recorded");
        let finished = store
            .finish_turn(&id, vec![first, second], reply, None, None, false)
            .expect("turn should finish");

        let contents = finished
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            contents,
            vec![
                "build it",
                "Running Bash",
                "also add tests",
                "Running Edit",
                "done"
            ],
        );
        drop(store);
        fs::remove_dir_all(data_dir).expect("temporary store should be removable");
    }

    #[test]
    fn rename_is_normalized_and_persisted() {
        let data_dir = std::env::temp_dir().join(format!("onyx-session-rename-{}", Uuid::new_v4()));
        let id = Uuid::new_v4().to_string();
        let store = SessionStore::load(&data_dir).expect("store should load");
        store
            .insert(test_session(id.clone()))
            .expect("session should persist");

        let renamed = store
            .rename(RenameSessionInput {
                session_id: id.clone(),
                title: "  Renamed\n session ".to_string(),
            })
            .expect("session should rename");
        assert_eq!(renamed.title, "Renamed session");
        drop(store);

        let restored = SessionStore::load(&data_dir).expect("store should reload");
        assert_eq!(
            restored.get(&id).map(|session| session.title),
            Some("Renamed session".to_string())
        );
        fs::remove_dir_all(data_dir).expect("temporary store should be removable");
    }

    #[test]
    fn changing_a_model_keeps_the_cli_continuation() {
        let data_dir = std::env::temp_dir().join(format!("onyx-session-model-{}", Uuid::new_v4()));
        let id = Uuid::new_v4().to_string();
        let store = SessionStore::load(&data_dir).expect("store should load");
        store
            .insert(test_session(id.clone()))
            .expect("session should persist");

        let updated = store
            .update_options(UpdateSessionOptionsInput {
                session_id: id,
                provider: ProviderId::Codex,
                provider_brand: ProviderBrand::Openai,
                model: Some("gpt-5.1".to_string()),
                reasoning: None,
                speed_mode: SpeedMode::Standard,
                interaction_mode: InteractionMode::Build,
                access_mode: AccessMode::ApprovalRequired,
            })
            .expect("model should update");
        assert_eq!(
            updated.provider_session_id.as_deref(),
            Some("provider-continuation")
        );
        fs::remove_dir_all(data_dir).expect("temporary store should be removable");
    }
}
