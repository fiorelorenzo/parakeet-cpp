use parakeet_cpp::common_prefix_len;

#[test]
fn prefix_basic() {
    assert_eq!(
        common_prefix_len("ciao mondo", "ciao mondo bello"),
        "ciao mondo".len()
    );
    assert_eq!(common_prefix_len("", "abc"), 0);
    assert_eq!(common_prefix_len("abc", "abc"), "abc".len());
    assert_eq!(common_prefix_len("abc", "abd"), 2);
}

#[test]
fn prefix_is_char_boundary_safe() {
    // "è" is 2 bytes; differing after it must not split a char.
    let a = "perchè";
    let b = "perché";
    let n = common_prefix_len(a, b);
    assert!(a.is_char_boundary(n) && b.is_char_boundary(n));
}
