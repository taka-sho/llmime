//! FocusWatcher: propagates field classification on focus change.
//!
//! Designed to reflect focus changes within 100ms (F-064).

use std::sync::{Arc, Mutex};

use super::classifier::{FieldClass, FieldClassifier};

pub struct FocusWatcher {
    classifier: Arc<dyn FieldClassifier>,
    current: Mutex<FieldClass>,
}

impl FocusWatcher {
    pub fn new(classifier: Arc<dyn FieldClassifier>) -> Arc<Self> {
        let initial = classifier.classify();
        Arc::new(Self {
            classifier,
            current: Mutex::new(initial),
        })
    }

    /// Call when the focused input field changes.
    /// Re-classifies via the injected classifier and updates internal state.
    /// Returns the new classification.
    pub fn on_focus_change(&self) -> FieldClass {
        let class = self.classifier.classify();
        *self.current.lock().expect("FocusWatcher mutex poisoned") = class.clone();
        class
    }

    pub fn current(&self) -> FieldClass {
        self.current
            .lock()
            .expect("FocusWatcher mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;

    struct StubClassifier {
        sensitive: AtomicBool,
    }

    impl StubClassifier {
        fn new(sensitive: bool) -> Arc<Self> {
            Arc::new(Self {
                sensitive: AtomicBool::new(sensitive),
            })
        }

        fn set_sensitive(&self, val: bool) {
            self.sensitive.store(val, Ordering::SeqCst);
        }
    }

    impl FieldClassifier for StubClassifier {
        fn classify(&self) -> FieldClass {
            if self.sensitive.load(Ordering::SeqCst) {
                FieldClass::Sensitive
            } else {
                FieldClass::NonSensitive
            }
        }
    }

    #[test]
    fn trait_contract_sensitive() {
        let stub = StubClassifier::new(true);
        assert_eq!(stub.classify(), FieldClass::Sensitive);
    }

    #[test]
    fn trait_contract_non_sensitive() {
        let stub = StubClassifier::new(false);
        assert_eq!(stub.classify(), FieldClass::NonSensitive);
    }

    #[test]
    fn focus_watcher_propagates() {
        let stub = StubClassifier::new(false);
        let watcher = FocusWatcher::new(stub.clone());
        assert_eq!(watcher.current(), FieldClass::NonSensitive);

        stub.set_sensitive(true);
        let result = watcher.on_focus_change();
        assert_eq!(result, FieldClass::Sensitive);
        assert_eq!(watcher.current(), FieldClass::Sensitive);
    }

    #[test]
    fn focus_watcher_timing() {
        let stub = StubClassifier::new(false);
        let watcher = FocusWatcher::new(stub.clone());
        stub.set_sensitive(true);

        let start = Instant::now();
        let result = watcher.on_focus_change();
        let elapsed = start.elapsed();

        assert_eq!(result, FieldClass::Sensitive);
        // on_focus_change must complete well within 100ms (F-064)
        assert!(
            elapsed.as_millis() < 100,
            "on_focus_change took {}ms, expected <100ms",
            elapsed.as_millis()
        );
    }
}
