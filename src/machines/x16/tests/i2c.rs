//! The I2C bus, bit-banged as the KERNAL does it: an address no device is
//! at, NACKed; the RTC's time and RAM, written and read, its address
//! stepping on, its time counted once it's running, or set as an X16's
//! is; the SMC's version, its keys, its mouse's packets, and what it's
//! asked to do.

#[cfg(test)]
mod tests {
    use crate::host::clock::DateTime;
    use crate::machines::x16::i2c::{Asked, I2c, RTC, SMC};

    fn start(i2c: &mut I2c) {
        i2c.lines(true, true);
        i2c.lines(true, false);
        i2c.lines(false, false);
    }

    fn stop(i2c: &mut I2c) {
        i2c.lines(false, false);
        i2c.lines(true, false);
        i2c.lines(true, true);
    }

    /// `byte` sent, a bit as SCL rises: whether it's ACKed.
    fn send(i2c: &mut I2c, byte: u8) -> bool {
        for bit in (0..8).rev() {
            let sda = byte >> bit & 1 != 0;
            i2c.lines(false, sda);
            i2c.lines(true, sda);
            i2c.lines(false, sda);
        }
        i2c.lines(false, true);
        i2c.lines(true, true);
        let ack = !i2c.sda();
        i2c.lines(false, true);
        ack
    }

    /// A byte received, ACKed if `more` are wanted.
    fn receive(i2c: &mut I2c, more: bool) -> u8 {
        let mut byte = 0;
        for _ in 0..8 {
            i2c.lines(true, true);
            byte = byte << 1 | i2c.sda() as u8;
            i2c.lines(false, true);
        }
        i2c.lines(false, !more);
        i2c.lines(true, !more);
        i2c.lines(false, !more);
        i2c.lines(false, true);
        byte
    }

    /// `bytes` written to `device`, its register or command first.
    fn write(i2c: &mut I2c, device: u8, bytes: &[u8]) {
        start(i2c);
        assert!(send(i2c, device << 1), "addressed");
        for &byte in bytes {
            assert!(send(i2c, byte));
        }
        stop(i2c);
    }

    /// `count` bytes read from `device` at `register`.
    fn read(i2c: &mut I2c, device: u8, register: u8, count: usize) -> Vec<u8> {
        start(i2c);
        send(i2c, device << 1);
        send(i2c, register);
        start(i2c); // again, to read
        send(i2c, device << 1 | 1);
        let bytes = (0..count).map(|n| receive(i2c, n + 1 < count)).collect();
        stop(i2c);
        bytes
    }

    #[test]
    fn an_address_no_device_is_at_is_nacked() {
        let mut i2c = I2c::new();
        start(&mut i2c);
        assert!(!send(&mut i2c, 0x50 << 1));
        stop(&mut i2c);
        assert!(i2c.sda(), "let go");
    }

    #[test]
    fn the_rtcs_time_and_ram_are_written_and_read_its_address_stepping() {
        let mut i2c = I2c::new();
        assert_eq!(
            read(&mut i2c, RTC, 0x00, 1),
            [0x00],
            "stopped, as a new one is"
        );
        // 23:59:58, ST set: running, 24 hours.
        write(&mut i2c, RTC, &[0x00, 0x80 | 0x58, 0x59, 0x23]);
        assert_eq!(read(&mut i2c, RTC, 0x00, 3), [0xD8, 0x59, 0x23]);
        assert_eq!(read(&mut i2c, RTC, 0x03, 1)[0] & 0x20, 0x20, "OSCRUN");
        for _ in 0..2 * 8_000_000 {
            i2c.tick();
        }
        assert_eq!(
            read(&mut i2c, RTC, 0x00, 5),
            [0x80, 0x00, 0x00, 0x22, 0x02],
            "midnight, the 2nd, the day after"
        );
        write(&mut i2c, RTC, &[0x5E, 0x12, 0x34, 0x56]);
        assert_eq!(
            read(&mut i2c, RTC, 0x5E, 3),
            [0x12, 0x34, 0x56],
            "round to 0x20, its RAM's first, within its RAM"
        );
        assert_eq!(i2c.rtc.ram[0], 0x56);
    }

    #[test]
    fn the_smc_gives_its_version_and_keys_and_does_as_its_asked() {
        let mut i2c = I2c::new();
        assert_eq!(read(&mut i2c, SMC, 0x30, 1), [47]);
        i2c.smc.key(0x2B);
        i2c.smc.key(0xAB);
        assert_eq!(read(&mut i2c, SMC, 0x07, 1), [0x2B]);
        assert_eq!(read(&mut i2c, SMC, 0x07, 1), [0xAB]);
        assert_eq!(read(&mut i2c, SMC, 0x07, 1), [0x00], "none waiting");
        write(&mut i2c, SMC, &[0x02, 0x00]);
        assert_eq!(i2c.smc.asked, Some(Asked::Reset));
        write(&mut i2c, SMC, &[0x05, 0xFF]);
        assert!(i2c.smc.activity_light);
    }

    #[test]
    fn the_smcs_default_read_is_a_key_then_the_mouse() {
        let mut i2c = I2c::new();
        i2c.smc.key(0x1F);
        // Read with no command written: its default, 0x43.
        start(&mut i2c);
        send(&mut i2c, SMC << 1 | 1);
        let bytes = [receive(&mut i2c, true), receive(&mut i2c, false)];
        stop(&mut i2c);
        assert_eq!(bytes, [0x1F, 0x00], "the key, then no mouse packet");
    }

    #[test]
    fn the_smc_gives_the_mouses_packets_whole_as_its_id_has_them() {
        let mut i2c = I2c::new();
        assert_eq!(read(&mut i2c, SMC, 0x22, 1), [3], "a wheel");
        assert_eq!(read(&mut i2c, SMC, 0x21, 1), [0x00], "no packet");
        // Left held, 5 right, 2 down: Y's sign set.
        assert!(i2c.smc.mouse(0x01, 5, -2));
        let packet = (0..4).map(|_| read(&mut i2c, SMC, 0x21, 1)[0]);
        assert_eq!(packet.collect::<Vec<_>>(), [0x29, 0x05, 0xFE, 0x00]);
        // Without a wheel, 3 bytes; 5 packets take its 15, and a 6th has
        // no room.
        write(&mut i2c, SMC, &[0x20, 0x00]);
        for _ in 0..5 {
            assert!(i2c.smc.mouse(0x02, -256, 255));
        }
        assert!(!i2c.smc.mouse(0x02, 1, 1));
        let packet = (0..3).map(|_| read(&mut i2c, SMC, 0x42, 1)[0]);
        assert_eq!(packet.collect::<Vec<_>>(), [0x1A, 0x00, 0xFF]);
        write(&mut i2c, SMC, &[0x20, 0x03]);
        assert!(i2c.smc.mouse.is_empty(), "its ID set: let go");
    }

    #[test]
    fn the_rtc_set_as_an_x16s_is_running_its_time_given() {
        let mut i2c = I2c::new();
        i2c.rtc.set(DateTime {
            year: 2026,
            month: 11,
            day: 15,
            weekday: 0,
            hour: 23,
            minute: 59,
            second: 58,
        });
        assert_eq!(
            read(&mut i2c, RTC, 0x00, 7),
            [0x80 | 0x58, 0x59, 0x23, 0x20 | 0x08 | 7, 0x15, 0x11, 0x26],
            "ST, OSCRUN and VBATEN; Sunday the 7th day"
        );
        assert!(!i2c.rtc.ram_written);
        write(&mut i2c, RTC, &[0x20, 0x12]);
        assert!(i2c.rtc.ram_written);
    }
}
