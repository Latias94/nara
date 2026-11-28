/// Stable type tags stored in the NPAK v1 table of contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u64)]
pub enum NpakEntryKind {
    Scene = 1,
    Image = 2,
    Shader = 3,
    Material = 4,
    Raw = 5,
}
