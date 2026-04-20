//! FieldClassifier trait and FieldClass enum — shared across OS backends.

#[derive(Debug, Clone, PartialEq)]
pub enum FieldClass {
    Sensitive,
    NonSensitive,
    Unknown,
}

pub trait FieldClassifier: Send + Sync {
    fn classify(&self) -> FieldClass;
}
