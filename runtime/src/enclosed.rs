pub fn test(s: &str) {
    let i = 0..;
    for _ in i {
        lazy_regex::regex_is_match!("foo", s);
    }
}