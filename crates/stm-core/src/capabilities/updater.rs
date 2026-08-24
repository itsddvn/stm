use crate::lifecycle::LifecycleService;

pub struct UpdaterService {
    lifecycle: LifecycleService,
}

impl UpdaterService {
    pub fn new(lifecycle: LifecycleService) -> Self {
        Self { lifecycle }
    }

    pub fn coordinator(&self) -> &LifecycleService {
        &self.lifecycle
    }
}
