#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_version_is_known() {
        // SAFETY: parameterless C function with no side effects.
        let v = unsafe { parakeet_capi_abi_version() };
        assert!(v >= 4, "expected ABI >= 4, got {v}");
    }
}
