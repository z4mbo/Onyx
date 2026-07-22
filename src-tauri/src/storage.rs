use crate::model::{AgentSession, Message, SessionStatus};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
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
        state.version = 1;
        for session in &mut state.sessions {
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
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
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

    pub fn finish_turn(
        &self,
        id: &str,
        activities: Vec<Message>,
        message: Message,
        provider_session_id: Option<String>,
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
        session.status = if failed {
            SessionStatus::Failed
        } else {
            SessionStatus::Idle
        };
        session.updated_at = Utc::now();
        let result = session.clone();
        self.persist_state(&next)?;
        *state = next;
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
    use super::title_from;

    #[test]
    fn title_is_compact_and_bounded() {
        assert_eq!(title_from("  hello\n   world  "), "hello world");
        assert!(title_from(&"x".repeat(80)).ends_with('…'));
    }
}
