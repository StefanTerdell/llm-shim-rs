#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NonStreamingMode {
    Real,
    #[default]
    Virtual,
}
