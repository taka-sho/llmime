/// Field classification result for IME behaviour control.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldClass {
    /// Password or credit-card field — suppress IME input.
    Sensitive,
    /// Normal text field — IME input allowed.
    NonSensitive,
    /// Could not determine field type (COM error or no scopes reported).
    Unknown,
}

pub struct FieldDetector;

impl FieldDetector {
    /// Classify a slice of raw INPUT_SCOPE integers (platform-independent).
    ///
    /// IS_PASSWORD(76) and IS_CREDITCARDNUMBER(79) → `Sensitive`.
    /// Any other non-empty scope list → `NonSensitive`.
    /// Empty slice → `Unknown`.
    pub fn classify_scopes(scopes: &[i32]) -> FieldClass {
        const IS_PASSWORD: i32 = 76;
        const IS_CREDITCARDNUMBER: i32 = 79;

        if scopes.is_empty() {
            return FieldClass::Unknown;
        }
        for &s in scopes {
            if s == IS_PASSWORD || s == IS_CREDITCARDNUMBER {
                return FieldClass::Sensitive;
            }
        }
        FieldClass::NonSensitive
    }

    /// Classify a TSF input context by querying its `ITfInputScope` for sensitive field types.
    ///
    /// Returns `Unknown` on any COM error or when no scopes are provided.
    #[cfg(target_os = "windows")]
    pub fn classify_input_scope(
        input_scope: &windows::Win32::UI::TextServices::ITfInputScope,
    ) -> FieldClass {
        use windows::Win32::UI::TextServices::INPUT_SCOPE;

        let mut raw_ptr: *mut INPUT_SCOPE = std::ptr::null_mut();
        let mut count: u32 = 0;

        // SAFETY: raw_ptr and count are valid output parameters; memory freed below.
        let result = unsafe { input_scope.GetInputScopes(&mut raw_ptr, &mut count) };

        if result.is_err() || raw_ptr.is_null() || count == 0 {
            return FieldClass::Unknown;
        }

        // SAFETY: Windows allocated `count` INPUT_SCOPE elements starting at raw_ptr.
        let scopes: Vec<i32> = unsafe {
            std::slice::from_raw_parts(raw_ptr, count as usize)
                .iter()
                .map(|s| s.0)
                .collect()
        };

        // SAFETY: Windows allocated this memory; CoTaskMemFree is the correct deallocator.
        unsafe {
            windows::Win32::System::Com::CoTaskMemFree(Some(raw_ptr.cast()));
        }

        Self::classify_scopes(&scopes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_field() {
        assert_eq!(
            FieldDetector::classify_scopes(&[76]),
            FieldClass::Sensitive
        );
    }

    #[test]
    fn creditcard_field() {
        assert_eq!(
            FieldDetector::classify_scopes(&[79]),
            FieldClass::Sensitive
        );
    }

    #[test]
    fn default_field() {
        // IS_DEFAULT = 0
        assert_eq!(
            FieldDetector::classify_scopes(&[0]),
            FieldClass::NonSensitive
        );
    }

    #[test]
    fn url_field() {
        // IS_URL = 1
        assert_eq!(
            FieldDetector::classify_scopes(&[1]),
            FieldClass::NonSensitive
        );
    }

    #[test]
    fn sensitive_wins_over_mixed() {
        assert_eq!(
            FieldDetector::classify_scopes(&[0, 76]),
            FieldClass::Sensitive
        );
    }

    #[test]
    fn empty_scopes_unknown() {
        assert_eq!(FieldDetector::classify_scopes(&[]), FieldClass::Unknown);
    }
}
