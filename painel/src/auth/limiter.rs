/// Limitador de tentativas de login (implementado na Task 2).
#[derive(Debug, Default)]
pub struct LoginLimiter {}

impl LoginLimiter {
    pub fn new() -> Self {
        Self {}
    }
}
