//! LEON3 Peripheral Implementations

use std::sync::Arc;

use crate::{
    MemoryInterface,
    architecture::leon3::plugnplay::{Device, Record},
};

pub(crate) mod irqmp;

#[derive(Clone)]
pub(crate) enum Peripheral {
    Handled(Arc<dyn ResetablePeripheral>),
    Unhandled(Record),
}

impl std::fmt::Debug for Peripheral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handled(arg0) => f.debug_tuple("Handled").field(&arg0.device_id()).finish(),
            Self::Unhandled(arg0) => f.debug_tuple("Unhandled").field(&arg0.device).finish(),
        }
    }
}

pub(crate) trait Leon3Peripheral: Send + Sync {
    fn device_id(&self) -> Device;
}

pub(crate) trait ResetablePeripheral: Leon3Peripheral {
    fn reset(&self, probe: &mut dyn MemoryInterface) -> Result<(), crate::Error>;
}

impl Leon3Peripheral for Peripheral {
    fn device_id(&self) -> Device {
        match self {
            Peripheral::Handled(p) => p.device_id(),
            Peripheral::Unhandled(record) => record.device,
        }
    }
}
