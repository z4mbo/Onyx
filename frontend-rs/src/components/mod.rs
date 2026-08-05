mod composer;
mod home;
mod internal_browser;
mod launcher;
mod orb;
mod overlay;
mod provider_badge;
mod settings;
mod titlebar;
mod transcript;
mod update;
mod user_input;
mod voice_history;
mod workspace;

pub use composer::Composer;
pub use home::HomeView;
pub use internal_browser::InternalBrowser;
pub use launcher::{LauncherDialog, LauncherTarget};
pub use orb::OnyxOrb;
pub use overlay::{AgentOverlay, Hud};
pub use provider_badge::ProviderBadge;
pub use settings::{ColorScheme, SettingsDialog};
pub use titlebar::{Titlebar, TitlebarSession, TitlebarTab};
pub use transcript::Transcript;
pub use update::{UpdateDialog, UpdateStage};
pub use user_input::UserInputCard;
pub use voice_history::VoiceHistoryView;
pub use workspace::{
    BottomTerminalPanel, GitCommitDialog, RightWorkspacePanel, SessionWorkspaceUi,
    TerminalViewport, WorkspaceSurface, WorkspaceSurfaceKind, WorkspaceTerminal,
    WorkspaceTopbarActions,
};
