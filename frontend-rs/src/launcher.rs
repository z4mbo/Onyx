#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::collections::{BTreeMap, HashSet};

use crate::model::{AgentSession, MessageKind, MessageRole, workspace_name};

pub const RECENT_SESSION_LIMIT: usize = 12;
/// Transcript hits shown when a query matches message bodies but not the
/// session title, mirroring T3 Code's thread-content search.
const TRANSCRIPT_MATCH_LIMIT: usize = 8;
const TRANSCRIPT_SNIPPET_CONTEXT: usize = 56;

#[derive(Clone, Debug, PartialEq)]
pub enum LauncherTarget {
    OpenSession(String),
    NewSession(Option<String>),
    AddProject,
    OpenSettings,
    OpenVoiceHistory,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LauncherItem {
    pub group: &'static str,
    pub title: String,
    pub detail: String,
    pub target: LauncherTarget,
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn match_rank(needle: &str, field: &str) -> Option<u32> {
    let field = normalize(field);
    if field == needle {
        Some(3)
    } else if field.starts_with(needle) {
        Some(2)
    } else if field.contains(needle) {
        Some(1)
    } else {
        None
    }
}

/// A title match always outranks a detail-only match, and stronger match kinds
/// (exact > prefix > substring) win within a field.
fn score(needle: &str, item: &LauncherItem) -> Option<u32> {
    let title = match_rank(needle, &item.title);
    let detail = match_rank(needle, &item.detail);
    if title.is_none() && detail.is_none() {
        return None;
    }
    Some(title.unwrap_or(0) * 100 + detail.unwrap_or(0))
}

fn filtered(needle: &str, items: Vec<LauncherItem>) -> Vec<LauncherItem> {
    let mut scored: Vec<(u32, LauncherItem)> = items
        .into_iter()
        .filter_map(|item| score(needle, &item).map(|score| (score, item)))
        .collect();
    scored.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    scored.into_iter().map(|(_, item)| item).collect()
}

/// A short window of transcript text around the first occurrence of `needle`
/// (which is already lowercase), with ellipses marking trimmed sides.
fn transcript_snippet(content: &str, needle: &str) -> Option<String> {
    let compact = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let position = compact.to_ascii_lowercase().find(needle)?;
    let start = position.saturating_sub(TRANSCRIPT_SNIPPET_CONTEXT);
    let start = (0..=start)
        .rev()
        .find(|index| compact.is_char_boundary(*index))?;
    let end = (position + needle.len() + TRANSCRIPT_SNIPPET_CONTEXT).min(compact.len());
    let end = (end..=compact.len()).find(|index| compact.is_char_boundary(*index))?;
    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    snippet.push_str(compact[start..end].trim());
    if end < compact.len() {
        snippet.push('…');
    }
    Some(snippet)
}

/// First transcript hit for a session; user messages rank before assistant
/// replies, as in T3 Code's thread search.
fn transcript_match(session: &AgentSession, needle: &str) -> Option<LauncherItem> {
    for role in [MessageRole::User, MessageRole::Assistant] {
        for message in &session.messages {
            if message.role != role || message.kind != MessageKind::Text {
                continue;
            }
            if let Some(snippet) = transcript_snippet(&message.content, needle) {
                let speaker = if role == MessageRole::User {
                    "You"
                } else {
                    "Agent"
                };
                return Some(LauncherItem {
                    group: "Transcript matches",
                    title: session.title.clone(),
                    detail: format!("{speaker}: {snippet}"),
                    target: LauncherTarget::OpenSession(session.id.clone()),
                });
            }
        }
    }
    None
}

fn session_item(session: &AgentSession, group: &'static str) -> LauncherItem {
    let model = session
        .model
        .clone()
        .unwrap_or_else(|| format!("{:?}", session.provider).to_ascii_lowercase());
    LauncherItem {
        group,
        title: session.title.clone(),
        detail: format!("{} · {model}", workspace_name(&session.workspace)),
        target: LauncherTarget::OpenSession(session.id.clone()),
    }
}

pub fn build_items(
    sessions: &[AgentSession],
    draft_workspace: &str,
    query: &str,
) -> Vec<LauncherItem> {
    let actions = vec![
        LauncherItem {
            group: "Actions",
            title: "New session".to_owned(),
            detail: "Start a draft session".to_owned(),
            target: LauncherTarget::NewSession(None),
        },
        LauncherItem {
            group: "Actions",
            title: "Add project".to_owned(),
            detail: "Open a folder as a new project".to_owned(),
            target: LauncherTarget::AddProject,
        },
        LauncherItem {
            group: "Actions",
            title: "Voice history".to_owned(),
            detail: "Dictation and agent transcripts".to_owned(),
            target: LauncherTarget::OpenVoiceHistory,
        },
        LauncherItem {
            group: "Actions",
            title: "Open settings".to_owned(),
            detail: "Providers, voice, and appearance".to_owned(),
            target: LauncherTarget::OpenSettings,
        },
    ];

    let mut recent: Vec<&AgentSession> = sessions.iter().collect();
    recent.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));

    // A leading `>` restricts the palette to actions, matching the T3 Code
    // command palette convention.
    let trimmed = query.trim_start();
    if let Some(rest) = trimmed.strip_prefix('>') {
        let needle = normalize(rest);
        if needle.is_empty() {
            return actions;
        }
        return filtered(&needle, actions);
    }

    let needle = normalize(query);
    if needle.is_empty() {
        return actions
            .into_iter()
            .chain(
                recent
                    .into_iter()
                    .take(RECENT_SESSION_LIMIT)
                    .map(|session| session_item(session, "Recent sessions")),
            )
            .collect();
    }

    // Key by the full workspace path so distinct projects that share a folder
    // basename (~/work/api vs ~/personal/api) each stay reachable.
    let mut projects = BTreeMap::<String, ()>::new();
    for session in sessions {
        projects.entry(session.workspace.clone()).or_insert(());
    }
    if !draft_workspace.trim().is_empty() {
        projects.entry(draft_workspace.to_owned()).or_insert(());
    }
    let project_items = projects
        .into_keys()
        .map(|path| LauncherItem {
            group: "Projects",
            title: format!("New session in {}", workspace_name(&path)),
            detail: path.clone(),
            target: LauncherTarget::NewSession(Some(path)),
        })
        .collect();
    let session_items = recent
        .iter()
        .map(|session| session_item(session, "Sessions"))
        .collect();

    let mut items = filtered(&needle, actions);
    items.extend(filtered(&needle, project_items));
    let matched_sessions = filtered(&needle, session_items);
    // Sessions whose title/detail did not match may still match on transcript
    // content; those surface with a snippet instead of disappearing.
    let matched_ids = matched_sessions
        .iter()
        .filter_map(|item| match &item.target {
            LauncherTarget::OpenSession(id) => Some(id.as_str()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let transcript_items = recent
        .iter()
        .filter(|session| !matched_ids.contains(session.id.as_str()))
        .filter_map(|session| transcript_match(session, &needle))
        .take(TRANSCRIPT_MATCH_LIMIT)
        .collect::<Vec<_>>();
    items.extend(matched_sessions);
    items.extend(transcript_items);
    items
}

#[cfg(test)]
mod tests {
    use super::{LauncherTarget, RECENT_SESSION_LIMIT, build_items};
    use crate::model::{
        AccessMode, AgentSession, InteractionMode, Message, MessageKind, MessageRole,
        ProviderBrand, ProviderId, ReasoningEffort, SessionStatus, SpeedMode,
    };

    fn session(id: &str, title: &str, workspace: &str, updated_at: &str) -> AgentSession {
        AgentSession {
            id: id.to_owned(),
            title: title.to_owned(),
            provider: ProviderId::Claude,
            provider_brand: ProviderBrand::Anthropic,
            model: None,
            reasoning: Some(ReasoningEffort::Medium),
            speed_mode: SpeedMode::Standard,
            interaction_mode: InteractionMode::Build,
            access_mode: AccessMode::ApprovalRequired,
            workspace: workspace.to_owned(),
            provider_session_id: None,
            status: SessionStatus::Idle,
            messages: Vec::new(),
            context_usage: None,
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: updated_at.to_owned(),
        }
    }

    #[test]
    fn empty_query_lists_actions_then_most_recent_sessions() {
        let sessions = vec![
            session("a", "Older", "/tmp/onyx", "2026-01-01T00:00:00Z"),
            session("b", "Newer", "/tmp/onyx", "2026-02-01T00:00:00Z"),
        ];
        let items = build_items(&sessions, "", "");
        assert_eq!(items[0].group, "Actions");
        let recent: Vec<&str> = items
            .iter()
            .filter(|item| item.group == "Recent sessions")
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(recent, ["Newer", "Older"]);
    }

    #[test]
    fn recent_sessions_are_capped() {
        let sessions: Vec<AgentSession> = (0..RECENT_SESSION_LIMIT + 5)
            .map(|index| {
                session(
                    &format!("id-{index}"),
                    "Session",
                    "/tmp/onyx",
                    "2026-01-01T00:00:00Z",
                )
            })
            .collect();
        let items = build_items(&sessions, "", "");
        let recent = items
            .iter()
            .filter(|item| item.group == "Recent sessions")
            .count();
        assert_eq!(recent, RECENT_SESSION_LIMIT);
    }

    #[test]
    fn queries_rank_title_prefixes_above_substrings() {
        let sessions = vec![
            session("a", "Fix the parser", "/tmp/onyx", "2026-01-01T00:00:00Z"),
            session("b", "Parser rewrite", "/tmp/onyx", "2026-01-02T00:00:00Z"),
        ];
        let items = build_items(&sessions, "", "parser");
        let titles: Vec<&str> = items
            .iter()
            .filter(|item| item.group == "Sessions")
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(titles, ["Parser rewrite", "Fix the parser"]);
    }

    #[test]
    fn queries_surface_projects_and_drop_unmatched_actions() {
        let sessions = vec![session(
            "a",
            "Session",
            "/Users/dev/orbit",
            "2026-01-01T00:00:00Z",
        )];
        let items = build_items(&sessions, "", "orbit");
        assert!(items.iter().any(|item| {
            item.group == "Projects"
                && item.target == LauncherTarget::NewSession(Some("/Users/dev/orbit".to_owned()))
        }));
        assert!(items.iter().all(|item| item.group != "Actions"));
    }

    #[test]
    fn unmatched_query_returns_nothing() {
        let sessions = vec![session("a", "Session", "/tmp/onyx", "2026-01-01T00:00:00Z")];
        assert!(build_items(&sessions, "", "zzz-no-match").is_empty());
    }

    fn with_messages(mut base: AgentSession, messages: Vec<(MessageRole, &str)>) -> AgentSession {
        base.messages = messages
            .into_iter()
            .enumerate()
            .map(|(index, (role, content))| Message {
                id: format!("m{index}"),
                role,
                kind: MessageKind::Text,
                content: content.to_owned(),
                created_at: "2026-01-01T00:00:00Z".to_owned(),
            })
            .collect();
        base
    }

    #[test]
    fn transcript_content_matches_surface_with_snippets() {
        let sessions = vec![with_messages(
            session("a", "Refactor cache", "/tmp/onyx", "2026-01-01T00:00:00Z"),
            vec![
                (MessageRole::User, "please tune the eviction thresholds"),
                (MessageRole::Assistant, "Done, thresholds updated."),
            ],
        )];
        let items = build_items(&sessions, "", "eviction");
        let hit = items
            .iter()
            .find(|item| item.group == "Transcript matches")
            .expect("transcript match should surface");
        assert_eq!(hit.title, "Refactor cache");
        assert!(hit.detail.starts_with("You: "));
        assert!(hit.detail.contains("eviction"));
        assert_eq!(hit.target, LauncherTarget::OpenSession("a".to_owned()));
    }

    #[test]
    fn title_matched_sessions_do_not_duplicate_into_transcripts() {
        let sessions = vec![with_messages(
            session("a", "Eviction work", "/tmp/onyx", "2026-01-01T00:00:00Z"),
            vec![(MessageRole::User, "about eviction again")],
        )];
        let items = build_items(&sessions, "", "eviction");
        assert!(items.iter().any(|item| item.group == "Sessions"));
        assert!(items.iter().all(|item| item.group != "Transcript matches"));
    }

    #[test]
    fn transcript_snippets_trim_long_messages() {
        let long = format!("{} needle-word {}", "left ".repeat(50), "right ".repeat(50));
        let sessions = vec![with_messages(
            session("a", "Long", "/tmp/onyx", "2026-01-01T00:00:00Z"),
            vec![(MessageRole::Assistant, long.as_str())],
        )];
        let items = build_items(&sessions, "", "needle-word");
        let hit = items
            .iter()
            .find(|item| item.group == "Transcript matches")
            .expect("transcript match should surface");
        assert!(hit.detail.starts_with("Agent: …"));
        assert!(hit.detail.ends_with('…'));
        assert!(hit.detail.len() < 200);
    }

    #[test]
    fn angle_prefix_restricts_to_actions() {
        let sessions = vec![session(
            "a",
            "Settings work",
            "/tmp/onyx",
            "2026-01-01T00:00:00Z",
        )];
        let items = build_items(&sessions, "", ">");
        assert!(!items.is_empty());
        assert!(items.iter().all(|item| item.group == "Actions"));

        let items = build_items(&sessions, "", "> settings");
        assert!(items.iter().all(|item| item.group == "Actions"));
        assert!(items.iter().any(|item| item.title == "Open settings"));
    }

    #[test]
    fn same_named_projects_stay_distinct() {
        let sessions = vec![
            session("a", "One", "/Users/dev/work/api", "2026-01-01T00:00:00Z"),
            session(
                "b",
                "Two",
                "/Users/dev/personal/api",
                "2026-01-01T00:00:00Z",
            ),
        ];
        let items = build_items(&sessions, "", "api");
        let paths: Vec<_> = items
            .iter()
            .filter(|item| item.group == "Projects")
            .map(|item| item.detail.as_str())
            .collect();
        assert!(paths.contains(&"/Users/dev/work/api"));
        assert!(paths.contains(&"/Users/dev/personal/api"));
    }

    #[test]
    fn draft_workspace_without_sessions_is_searchable() {
        let items = build_items(&[], "/Users/dev/fresh", "fresh");
        assert!(items.iter().any(|item| {
            item.group == "Projects"
                && item.target == LauncherTarget::NewSession(Some("/Users/dev/fresh".to_owned()))
        }));
    }
}
