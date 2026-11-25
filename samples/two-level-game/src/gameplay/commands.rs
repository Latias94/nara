use crate::command::{GameCommand, SemanticCommand, ServerCommand};

pub(crate) const TRAVEL: ServerCommand = ServerCommand::Gameplay(SemanticCommand::Travel);
pub(crate) const INSTANCE: ServerCommand = ServerCommand::Gameplay(SemanticCommand::Instance);
pub(crate) const MARKER: ServerCommand = ServerCommand::Gameplay(SemanticCommand::Marker);
pub(crate) const SAVE: ServerCommand = ServerCommand::Save;
pub(crate) const RESTORE: ServerCommand = ServerCommand::Restore;
pub(crate) const RETRY: ServerCommand = ServerCommand::Gameplay(SemanticCommand::Retry);
pub(crate) const SIDE_ROOM: ServerCommand = ServerCommand::SideRoom;
pub(crate) const LOOKOUT: ServerCommand = ServerCommand::Lookout;

pub(crate) const fn action(command: ServerCommand) -> GameCommand {
    GameCommand::Player(command)
}

pub(crate) const fn movement(direction: usize, pressed: bool) -> GameCommand {
    action(ServerCommand::Gameplay(SemanticCommand::Move {
        direction,
        pressed,
    }))
}
