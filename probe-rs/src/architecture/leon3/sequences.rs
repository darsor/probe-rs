use std::{fmt::Debug, sync::Arc};

/// A interface to operate debug sequences for Leon3 targets.
///
/// Should be implemented on a custom handle for chips that require special sequence code.
pub trait Leon3DebugSequence: Send + Sync + Debug {
    // TODO(darsor)
}

/// The default sequences that is used for Leon3 chips that do not specify a specific sequence.
#[derive(Debug)]
pub struct DefaultLeon3Sequence(pub(crate) ());

impl DefaultLeon3Sequence {
    /// Creates a new default Leon3 debug sequence.
    pub fn create() -> Arc<dyn Leon3DebugSequence> {
        Arc::new(Self(()))
    }
}

impl Leon3DebugSequence for DefaultLeon3Sequence {}
