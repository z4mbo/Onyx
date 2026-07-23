mod account;
mod composer;
mod home;
mod orb;
mod overlay;
mod provider_badge;
mod settings;
mod titlebar;
mod transcript;
mod voice_history;
mod workspace;

pub use account::AccountGate;
pub use composer::Composer;
pub use home::HomeView;
pub use orb::OnyxOrb;
pub use overlay::{AgentOverlay, Hud};
pub use provider_badge::ProviderBadge;
pub use settings::{ColorScheme, SettingsDialog};
pub use titlebar::{Titlebar, TitlebarSession, TitlebarTab};
pub use transcript::Transcript;
pub use voice_history::VoiceHistoryView;
pub use workspace::{
    BottomTerminalPanel, GitCommitDialog, RightWorkspacePanel, SessionWorkspaceUi,
    WorkspaceSurface, WorkspaceSurfaceKind, WorkspaceTerminal, WorkspaceTopbarActions,
};
