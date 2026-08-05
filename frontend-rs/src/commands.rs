//! Slash commands offered in the composer.
//!
//! Onyx drives every CLI headlessly (`--print` with a stream transport), so the
//! interactive REPL commands a terminal offers are not available to forward:
//! sending `/model` down a stream-json pipe makes the model read it as prose.
//! Each command therefore names an action Onyx performs itself, and only the
//! handful that provider CLIs genuinely accept as *prompt* text are forwarded.

use crate::model::ProviderId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandAction {
    /// Opens the existing file picker.
    AttachFiles,
    /// Replaces the palette with a list of choices.
    PickModel,
    PickReasoning,
    PickAccess,
    PickSpeed,
    /// Flips plan/build in place.
    ToggleMode,
    /// Handled by the application shell.
    Rename,
    NewSession,
    OpenSettings,
    OpenProject,
    ShowUsage,
    ShowContext,
    /// Sent to the provider as prompt text, which is how these are invoked in a
    /// non-interactive session too.
    Prompt(&'static str),
}

impl CommandAction {
    /// True when the palette stays open to show a second level of choices.
    // Only called from the wasm-gated components tree, so the native test
    // build would otherwise flag it as dead code.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub const fn opens_values(self) -> bool {
        matches!(
            self,
            Self::PickModel | Self::PickReasoning | Self::PickAccess | Self::PickSpeed
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandEntry {
    /// Written the way it is typed, including the leading slash.
    pub name: &'static str,
    pub summary: &'static str,
    pub action: CommandAction,
}

const fn entry(name: &'static str, summary: &'static str, action: CommandAction) -> CommandEntry {
    CommandEntry {
        name,
        summary,
        action,
    }
}

/// Commands Onyx implements for every provider, because Onyx owns the state
/// they act on.
const SHARED: &[CommandEntry] = &[
    entry(
        "/add",
        "Attach files to the prompt",
        CommandAction::AttachFiles,
    ),
    entry(
        "/model",
        "Switch the model for this session",
        CommandAction::PickModel,
    ),
    entry(
        "/effort",
        "Set reasoning effort",
        CommandAction::PickReasoning,
    ),
    entry(
        "/speed",
        "Switch the service tier",
        CommandAction::PickSpeed,
    ),
    entry(
        "/permissions",
        "Change what runs without asking",
        CommandAction::PickAccess,
    ),
    entry(
        "/plan",
        "Toggle plan and build mode",
        CommandAction::ToggleMode,
    ),
    entry(
        "/context",
        "Show context used for this session",
        CommandAction::ShowContext,
    ),
    entry(
        "/usage",
        "Show the provider usage window",
        CommandAction::ShowUsage,
    ),
    entry("/rename", "Rename this session", CommandAction::Rename),
    entry("/new", "Start a new session", CommandAction::NewSession),
    entry(
        "/project",
        "Open the project in an editor",
        CommandAction::OpenProject,
    ),
    entry("/config", "Open Onyx settings", CommandAction::OpenSettings),
];

/// Commands forwarded verbatim, one list per CLI. These are prompt-level in the
/// provider's own protocol, so a headless session honours them.
const CLAUDE_FORWARDED: &[CommandEntry] = &[
    entry(
        "/init",
        "Write a CLAUDE.md for this project",
        CommandAction::Prompt("/init"),
    ),
    entry(
        "/compact",
        "Summarize the conversation so far",
        CommandAction::Prompt("/compact"),
    ),
    entry(
        "/review",
        "Review the pending changes",
        CommandAction::Prompt("/review"),
    ),
];

const CODEX_FORWARDED: &[CommandEntry] = &[
    entry(
        "/init",
        "Write an AGENTS.md for this project",
        CommandAction::Prompt("/init"),
    ),
    entry(
        "/compact",
        "Summarize the conversation so far",
        CommandAction::Prompt("/compact"),
    ),
    entry(
        "/review",
        "Review the pending changes",
        CommandAction::Prompt("/review"),
    ),
];

const GEMINI_FORWARDED: &[CommandEntry] = &[
    entry(
        "/init",
        "Write a GEMINI.md for this project",
        CommandAction::Prompt("/init"),
    ),
    entry(
        "/compress",
        "Summarize the conversation so far",
        CommandAction::Prompt("/compress"),
    ),
];

const KIMI_FORWARDED: &[CommandEntry] = &[entry(
    "/compact",
    "Summarize the conversation so far",
    CommandAction::Prompt("/compact"),
)];

pub fn commands_for(provider: ProviderId) -> Vec<CommandEntry> {
    let forwarded = match provider {
        ProviderId::Claude => CLAUDE_FORWARDED,
        ProviderId::Codex => CODEX_FORWARDED,
        ProviderId::Gemini => GEMINI_FORWARDED,
        ProviderId::Kimi => KIMI_FORWARDED,
        // OpenCode's server API has no forwarded slash commands; OpenRouter is
        // a direct API loop with no CLI behind it.
        ProviderId::Opencode | ProviderId::Openrouter => &[],
    };
    SHARED.iter().chain(forwarded).copied().collect()
}

/// Matches the text after the leading slash against command names. An empty
/// query lists everything, which is what the `+` menu shows.
pub fn filter(commands: &[CommandEntry], query: &str) -> Vec<CommandEntry> {
    let needle = query.trim_start_matches('/').trim().to_ascii_lowercase();
    if needle.is_empty() {
        return commands.to_vec();
    }
    let mut prefix = Vec::new();
    let mut contains = Vec::new();
    for command in commands {
        let name = command.name.trim_start_matches('/').to_ascii_lowercase();
        if name.starts_with(&needle) {
            prefix.push(*command);
        } else if name.contains(&needle) || command.summary.to_ascii_lowercase().contains(&needle) {
            contains.push(*command);
        }
    }
    prefix.extend(contains);
    prefix
}

/// True while the prompt is still a bare command token, which is when the
/// palette should be following along.
pub fn is_command_query(value: &str) -> bool {
    value.starts_with('/') && !value.trim_start_matches('/').contains(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::{CommandAction, ProviderId, commands_for, filter, is_command_query};

    #[test]
    fn every_provider_gets_the_shared_commands() {
        for provider in ProviderId::ALL {
            let commands = commands_for(provider);
            assert!(commands.iter().any(|command| command.name == "/model"));
            assert!(commands.iter().any(|command| command.name == "/rename"));
        }
    }

    #[test]
    fn only_cli_providers_forward_prompt_commands() {
        let claude = commands_for(ProviderId::Claude);
        assert!(
            claude
                .iter()
                .any(|command| command.action == CommandAction::Prompt("/init"))
        );
        let openrouter = commands_for(ProviderId::Openrouter);
        assert!(
            !openrouter
                .iter()
                .any(|command| matches!(command.action, CommandAction::Prompt(_)))
        );
    }

    #[test]
    fn names_are_unique_per_provider() {
        for provider in ProviderId::ALL {
            let commands = commands_for(provider);
            let mut names = commands
                .iter()
                .map(|command| command.name)
                .collect::<Vec<_>>();
            names.sort_unstable();
            let count = names.len();
            names.dedup();
            assert_eq!(names.len(), count, "duplicate command for {provider:?}");
        }
    }

    #[test]
    fn filtering_prefers_name_prefixes() {
        let commands = commands_for(ProviderId::Claude);
        let matches = filter(&commands, "/mod");
        assert_eq!(matches.first().map(|command| command.name), Some("/model"));

        // A summary word still finds the command when the name does not match.
        let matches = filter(&commands, "/reasoning");
        assert!(matches.iter().any(|command| command.name == "/effort"));

        assert_eq!(filter(&commands, "/").len(), commands.len());
        assert!(filter(&commands, "/zzzz").is_empty());
    }

    #[test]
    fn the_palette_closes_once_the_prompt_is_prose() {
        assert!(is_command_query("/"));
        assert!(is_command_query("/mod"));
        assert!(!is_command_query("/model please"));
        assert!(!is_command_query("fix the parser"));
        assert!(!is_command_query(""));
    }
}
