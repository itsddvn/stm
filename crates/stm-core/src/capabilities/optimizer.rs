use crate::lifecycle::LifecycleService;

/// Reserved capability. This delivery has no apply UX.
pub struct OptimizerService {
    _lifecycle: LifecycleService,
}

impl OptimizerService {
    pub fn new(lifecycle: LifecycleService) -> Self {
        Self {
            _lifecycle: lifecycle,
        }
    }
}
