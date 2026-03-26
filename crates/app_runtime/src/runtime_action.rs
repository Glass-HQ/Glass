#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeAction {
    Run,
    Build,
}

impl RuntimeAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Run => "Run",
            Self::Build => "Build",
        }
    }
}
