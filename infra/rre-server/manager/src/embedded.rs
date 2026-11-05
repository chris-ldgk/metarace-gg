include!(concat!(env!("OUT_DIR"), "/payload.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_the_game_payload() {
        assert!(!GAME_ZIP.is_empty());
        assert_eq!(&GAME_ZIP[..2], b"PK");
        assert!(ASSEMBLY_VERSION.starts_with("112."));
    }
}
