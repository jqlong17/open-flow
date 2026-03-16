/// Convert between simplified and traditional Chinese using macOS native APIs.
/// Falls back to no-op on non-macOS platforms.

/// Apply Chinese character conversion based on config value.
/// - "s2t": Simplified -> Traditional
/// - "t2s": Traditional -> Simplified
/// - "" or "none": no conversion
pub fn convert_chinese(text: &str, mode: &str) -> String {
    match mode {
        "s2t" => transform_string(text, false),
        "t2s" => transform_string(text, true),
        _ => text.to_string(),
    }
}

#[cfg(target_os = "macos")]
fn transform_string(text: &str, reverse: bool) -> String {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;

    // CFStringTransform with "Simplified-Traditional"
    // reverse=false: Simplified -> Traditional
    // reverse=true:  Traditional -> Simplified
    extern "C" {
        fn CFStringCreateMutableCopy(
            alloc: *const std::ffi::c_void,
            max_length: i64,
            the_string: core_foundation::string::CFStringRef,
        ) -> *mut std::ffi::c_void;

        fn CFStringTransform(
            string: *mut std::ffi::c_void,
            range: *mut std::ffi::c_void,
            transform: core_foundation::string::CFStringRef,
            reverse: bool,
        ) -> bool;
    }

    let cf_text = CFString::new(text);
    let transform = CFString::new("Simplified-Traditional");

    unsafe {
        let mutable = CFStringCreateMutableCopy(std::ptr::null(), 0, cf_text.as_concrete_TypeRef());
        if mutable.is_null() {
            return text.to_string();
        }

        let ok = CFStringTransform(
            mutable,
            std::ptr::null_mut(),
            transform.as_concrete_TypeRef(),
            reverse,
        );

        if ok {
            // Read back the transformed string
            let cf_result =
                CFString::wrap_under_create_rule(mutable as core_foundation::string::CFStringRef);
            cf_result.to_string()
        } else {
            // Transform failed, return original
            core_foundation::base::CFRelease(mutable);
            text.to_string()
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn transform_string(text: &str, _reverse: bool) -> String {
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_conversion() {
        assert_eq!(convert_chinese("hello", ""), "hello");
        assert_eq!(convert_chinese("hello", "none"), "hello");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_s2t() {
        let result = convert_chinese("简体中文测试", "s2t");
        assert!(
            result.contains('簡') || result.contains('體'),
            "Expected traditional chars, got: {}",
            result
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_t2s() {
        let result = convert_chinese("簡體中文測試", "t2s");
        assert!(
            result.contains('简') || result.contains('体'),
            "Expected simplified chars, got: {}",
            result
        );
    }
}
