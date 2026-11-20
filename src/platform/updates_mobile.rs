//! Phones and tablets update through their stores: the updater's states, for the pages that show them, and no updater.

/// Where the updater is, as Settings shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
    },
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    Ready {
        version: String,
    },
    Installing,
    Failed(Failure),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Network,
    Invalid,
    Install,
}

pub struct Updater;
impl Updater {
    pub fn read_only(&self) -> bool {
        false
    }
    pub fn download_page(&self) -> Option<String> {
        None
    }
    pub fn phase(&self) -> Phase {
        Phase::Idle
    }
    pub fn check(&self) -> Phase {
        Phase::Idle
    }
    pub fn download(&self) -> Phase {
        Phase::Idle
    }
    pub fn install(&self) -> Result<(), Failure> {
        Err(Failure::Install)
    }
    pub fn offered_notes(&self) -> Option<String> {
        None
    }
    pub fn install_on_quit(&self) {}
}
pub fn updater() -> Option<&'static Updater> {
    None
}
pub fn preview_phase() -> Option<Phase> {
    None
}
pub fn preview_download(_version: String) {}
