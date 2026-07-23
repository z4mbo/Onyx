use crate::model::{
    AgentSession, ContextUsage, Message, ProviderBrand, SessionStatus, UpdateSessionOptionsInput,
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

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedState {
    version: u32,
    sessions: Vec<AgentSession>,
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
            Ok(value) => match serde_json::from_str::<PersistedState>(&value) {
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
        state.version = 2;
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
        let connection_changed = session.provider != input.provider || session.model != input.model;
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
        if session.messages.is_empty() {
            session.title = title_from(&message.content);
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

fn title_from(content: &str) -> String {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = compact.chars().take(54).collect::<String>();
    if compact.chars().count() > 54 {
        format!("{title}…")
    } else if title.is_empty() {
        "New session".to_string()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::{PersistedState, SessionStore, title_from};
    use crate::model::{
        AccessMode, AgentSession, InteractionMode, Message, MessageKind, MessageRole,
        ProviderBrand, ProviderId, SessionStatus, SpeedMode,
    };
    use chrono::Utc;
    use parking_lot::RwLock;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn title_is_compact_and_bounded() {
        assert_eq!(title_from("  hello\n   world  "), "hello world");
        assert!(title_from(&"x".repeat(80)).ends_with('…'));
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
}
