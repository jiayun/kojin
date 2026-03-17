use std::any::{Any, TypeId};
use std::collections::HashMap;

/// Task execution context providing type-safe access to shared data.
pub struct TaskContext {
    data: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl TaskContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Insert a value into the context.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.data.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Get a reference to a value from the context.
    pub fn data<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    /// Check if the context contains a value of the given type.
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.data.contains_key(&TypeId::of::<T>())
    }
}

impl Default for TaskContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_insert_and_retrieve() {
        let mut ctx = TaskContext::new();
        ctx.insert(42u32);
        ctx.insert("hello".to_string());

        assert_eq!(ctx.data::<u32>(), Some(&42));
        assert_eq!(ctx.data::<String>(), Some(&"hello".to_string()));
        assert_eq!(ctx.data::<bool>(), None);
    }

    #[test]
    fn context_contains() {
        let mut ctx = TaskContext::new();
        ctx.insert(42u32);
        assert!(ctx.contains::<u32>());
        assert!(!ctx.contains::<String>());
    }

    #[test]
    fn context_overwrite() {
        let mut ctx = TaskContext::new();
        ctx.insert(1u32);
        ctx.insert(2u32);
        assert_eq!(ctx.data::<u32>(), Some(&2));
    }
}
