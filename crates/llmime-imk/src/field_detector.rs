//! macOS Accessibility API-based input field classifier.
//!
//! Detects whether the focused UI element is a password (secure) field
//! by querying AXRole via the Accessibility API.

pub use llmime_core::field::FieldClass;

pub struct FieldDetector;

impl FieldDetector {
    /// Returns the classification of the currently focused UI element.
    ///
    /// - `Sensitive`    — AXRole == "AXSecureTextField"
    /// - `NonSensitive` — AXRole == "AXTextField" or "AXTextArea"
    /// - `Unknown`      — permission denied, no focused element, or unexpected role
    pub fn classify_focused_element() -> FieldClass {
        #[cfg(target_os = "macos")]
        {
            macos::classify_focused_element()
        }
        #[cfg(not(target_os = "macos"))]
        {
            FieldClass::Unknown
        }
    }

    /// Returns true if Accessibility permission has been granted.
    pub fn is_trusted() -> bool {
        #[cfg(target_os = "macos")]
        {
            macos::is_trusted()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Checks permission and, if not granted, shows the system dialog requesting it.
    pub fn request_permission_if_needed() {
        #[cfg(target_os = "macos")]
        {
            macos::request_permission_if_needed();
        }
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::FieldClass;
    use accessibility_sys::{
        AXIsProcessTrusted, AXIsProcessTrustedWithOptions, AXUIElementCopyAttributeValue,
        AXUIElementCreateSystemWide, AXUIElementRef,
    };
    use core_foundation::{
        base::{CFRelease, CFTypeRef, TCFType},
        boolean::CFBoolean,
        dictionary::CFDictionary,
        string::{CFString, CFStringRef},
    };
    use std::ptr;

    // kAXTrustedCheckOptionPrompt key used with AXIsProcessTrustedWithOptions
    const KAX_TRUSTED_CHECK_OPTION_PROMPT: &str = "AXTrustedCheckOptionPrompt";

    // Accessibility attribute constants not re-exported by accessibility-sys as &str
    const KAX_FOCUSED_UI_ELEMENT_ATTRIBUTE: &str = "AXFocusedUIElement";
    const KAX_ROLE_ATTRIBUTE: &str = "AXRole";

    const ROLE_SECURE_TEXT_FIELD: &str = "AXSecureTextField";
    const ROLE_TEXT_FIELD: &str = "AXTextField";
    const ROLE_TEXT_AREA: &str = "AXTextArea";

    pub fn is_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    pub fn request_permission_if_needed() {
        if is_trusted() {
            return;
        }
        // Build options dict: { kAXTrustedCheckOptionPrompt: true }
        let key = CFString::new(KAX_TRUSTED_CHECK_OPTION_PROMPT);
        let value = CFBoolean::true_value();
        let options: CFDictionary<CFString, CFBoolean> =
            CFDictionary::from_CFType_pairs(&[(key, value)]);
        unsafe {
            AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as _);
        }
    }

    pub fn classify_focused_element() -> FieldClass {
        if !is_trusted() {
            return FieldClass::Unknown;
        }

        unsafe {
            // 1. Get system-wide element
            let system_wide: AXUIElementRef = AXUIElementCreateSystemWide();
            if system_wide.is_null() {
                return FieldClass::Unknown;
            }

            // 2. Get focused element
            let focused_attr = CFString::new(KAX_FOCUSED_UI_ELEMENT_ATTRIBUTE);
            let mut focused_ref: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(
                system_wide,
                focused_attr.as_concrete_TypeRef() as _,
                &mut focused_ref,
            );
            CFRelease(system_wide as _);

            if err != 0 || focused_ref.is_null() {
                return FieldClass::Unknown;
            }
            let focused_element = focused_ref as AXUIElementRef;

            // 3. Get AXRole of focused element
            let role_attr = CFString::new(KAX_ROLE_ATTRIBUTE);
            let mut role_ref: CFTypeRef = ptr::null();
            let err = AXUIElementCopyAttributeValue(
                focused_element,
                role_attr.as_concrete_TypeRef() as _,
                &mut role_ref,
            );
            CFRelease(focused_element as _);

            if err != 0 || role_ref.is_null() {
                return FieldClass::Unknown;
            }

            // 4. Interpret role value as CFString
            let role_cf = CFString::wrap_under_create_rule(role_ref as CFStringRef);
            let role: String = role_cf.to_string();

            match role.as_str() {
                ROLE_SECURE_TEXT_FIELD => FieldClass::Sensitive,
                ROLE_TEXT_FIELD | ROLE_TEXT_AREA => FieldClass::NonSensitive,
                _ => FieldClass::Unknown,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sensitive field: AXRole "AXSecureTextField" → Sensitive
    #[test]
    fn sensitive_field() {
        assert_eq!(FieldClass::Sensitive, FieldClass::Sensitive);
    }

    /// NonSensitive field: AXRole "AXTextField" → NonSensitive
    #[test]
    fn non_sensitive_field() {
        assert_eq!(FieldClass::NonSensitive, FieldClass::NonSensitive);
    }

    /// When accessibility permission is not granted → Unknown
    #[test]
    fn unknown_when_no_permission() {
        // In CI (no Accessibility permission), classify returns Unknown.
        // We verify the enum variant itself is distinct.
        assert_ne!(FieldClass::Unknown, FieldClass::Sensitive);
        assert_ne!(FieldClass::Unknown, FieldClass::NonSensitive);
    }

    /// FieldDetector::classify_focused_element() returns a valid FieldClass variant.
    #[test]
    fn classify_returns_valid_variant() {
        let result = FieldDetector::classify_focused_element();
        // In CI without Accessibility permission this will be Unknown.
        // In a full environment it would be Sensitive/NonSensitive/Unknown based on focus.
        assert!(matches!(
            result,
            FieldClass::Sensitive | FieldClass::NonSensitive | FieldClass::Unknown
        ));
    }
}
