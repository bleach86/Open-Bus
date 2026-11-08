//! What a machine's RAM holds at power-on, as it's said.

#[cfg(test)]
mod tests {
    use crate::host::machine::PowerOnRam;

    #[test]
    fn its_ram_is_as_its_said_to_be() {
        assert_eq!(PowerOnRam::default(), PowerOnRam::Ones);
        for (name, ram) in [
            ("ones", PowerOnRam::Ones),
            ("zeros", PowerOnRam::Zeros),
            ("random", PowerOnRam::Random),
        ] {
            assert_eq!(PowerOnRam::parse(name), Some(ram));
        }
        assert_eq!(PowerOnRam::parse("ff"), None);

        let mut ram = [0x5A; 0x800];
        PowerOnRam::Ones.fill(&mut ram);
        assert!(ram.iter().all(|&byte| byte == 0xFF));
        PowerOnRam::Zeros.fill(&mut ram);
        assert!(ram.iter().all(|&byte| byte == 0));
    }

    #[test]
    fn random_ram_is_different_each_time() {
        let (mut first, mut second) = ([0; 0x800], [0; 0x800]);
        PowerOnRam::Random.fill(&mut first);
        PowerOnRam::Random.fill(&mut second);
        assert_ne!(first, second);
        // Every value about as often as any other: none missing, of 64K. (Of
        // 2K, one would be about one time in eleven.)
        let mut lots = vec![0; 0x10000];
        PowerOnRam::Random.fill(&mut lots);
        let mut seen = [false; 256];
        for byte in lots {
            seen[byte as usize] = true;
        }
        assert!(seen.iter().all(|&seen| seen), "a value never seen");
    }
}
