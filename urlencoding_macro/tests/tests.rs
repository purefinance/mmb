/// Took from crate `urlencoding`
#[cfg(test)]
mod tests_urlencoding {
    use urlencoding_macro::encode;

    #[test]
    fn it_encodes_successfully() {
        let expected = "this%20that";
        assert_eq!(expected, encode!("this that"));
    }

    #[test]
    fn it_encodes_successfully_emoji() {
        let expected = "%F0%9F%91%BE%20Exterminate%21";
        assert_eq!(expected, encode!("👾 Exterminate!"));
    }

    #[test]
    fn misc() {
        assert_eq!("pureascii", encode!("pureascii"));
        assert_eq!("", encode!(""));
        assert_eq!("%26a%25b%21c.d%3Fe", encode!("&a%b!c.d?e"));
        assert_eq!("%00", encode!("\0"));
        assert_eq!("%00x", encode!("\0x"));
        assert_eq!("x%00", encode!("x\0"));
        assert_eq!("x%00x", encode!("x\0x"));
        assert_eq!("aa%00%00bb", encode!("aa\0\0bb"));
    }

    #[test]
    fn whatwg_examples() {
        assert_eq!(encode!("≡"), "%E2%89%A1");
        assert_eq!(encode!("‽"), "%E2%80%BD");
        assert_eq!(encode!("Say what‽"), "Say%20what%E2%80%BD");
    }
}
