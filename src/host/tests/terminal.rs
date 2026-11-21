//! The host's terminal, for a machine's terminal port: a telnet client's
//! end of a line one key, not two; Ctrl-C taking the keys typed before it,
//! as a break, so a machine that looks for it in the key waiting sees it;
//! and the BASIC machine stopping a program that prints, a key typed ahead
//! of the Ctrl-C.

#[cfg(test)]
mod tests {
    use crate::cpu::bus::Bus;
    use crate::host::terminal::{Terminal, typed};
    use crate::machines::basic::Memory;

    #[test]
    fn a_return_and_what_a_telnet_client_sends_after_it_are_one_key() {
        assert!(typed(0, b'\r'));
        assert!(!typed(b'\r', b'\n'), "CR LF");
        assert!(!typed(b'\r', 0), "CR NUL");
        assert!(typed(b'\n', b'\n'), "two line feeds: two returns");
        assert!(typed(b'\r', b'\r'), "two returns");
        assert!(typed(b'A', 0), "a NUL typed on its own");
    }

    #[test]
    fn ctrl_c_takes_the_keys_before_it_as_a_break() {
        let terminal = Terminal::default();
        assert!(!terminal.take_break());
        terminal.type_key(b'X');
        terminal.type_break();
        terminal.type_key(b'Y');
        assert!(terminal.take_break());
        assert_eq!(terminal.key(), Some(b'Y'), "X and Ctrl-C gone, Y after");
        assert!(!terminal.take_break(), "once");
    }

    #[test]
    fn the_basic_machine_sees_ctrl_c_behind_a_key_left_waiting() {
        let mut memory = Memory {
            terminal: Some(Terminal::default()),
            ..Memory::default()
        };
        let terminal = memory.terminal.as_ref().unwrap();
        terminal.type_key(b'\n');
        assert_eq!(memory.read(0xFFF9), b'\n', "a key BASIC leaves waiting");
        memory.terminal.as_ref().unwrap().type_break();
        assert_eq!(
            memory.read(0xFFF9),
            0x03,
            "Ctrl-C there, for BASIC to stop on"
        );
    }
}
