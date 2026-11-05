//! A web page as a display, on any machine: here the NES, with a window
//! that's only memory. Its picture as a canvas has it, its sound taken,
//! what's pressed given; and the same machine going on from a state.

#[cfg(test)]
mod tests {
    use crate::cpu::CPU;
    use crate::host::machine::Machine;
    use crate::host::page::{Page, Shared};
    use crate::host::run::Host;
    use crate::machines::nes::Nes;
    use crate::machines::nes::cartridge::Cartridge;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn host() -> (Host<Nes>, Rc<RefCell<Shared>>) {
        let mut image = b"NES\x1A\x01\x01".to_vec();
        image.extend([0; 10]);
        image.extend([0xEA; 0x4000 + 0x2000]);
        let mut nes = Nes::new(Cartridge::new(&image).unwrap());
        nes.name = "Test".to_string();
        let mut cpu = CPU::with_bus(nes);
        cpu.reset();
        let page = Rc::new(RefCell::new(Shared::default()));
        let state = std::env::temp_dir().join("cpu_emu_no_page_state");
        (Host::new(cpu, Box::new(Page(page.clone())), state), page)
    }

    #[test]
    fn its_picture_is_a_canvass_and_its_sound_is_kept() {
        let (mut host, page) = host();
        host.frame();
        let page = page.borrow();
        assert_eq!((page.width, page.height), (256, 240));
        assert_eq!(page.pixels.len(), 256 * 240 * 4);
        assert!(page.pixels.chunks(4).all(|dot| dot[3] == 0xFF), "a dot not opaque");
        assert!(!page.samples.is_empty(), "no sound kept");
        assert_eq!(page.title, "NES   Test");
    }

    #[test]
    fn a_dots_bytes_are_red_green_and_blue_in_that_order() {
        use crate::host::display::Display;
        let page = Rc::new(RefCell::new(Shared::default()));
        Page(page.clone()).show(&[0x123456], 1, 1);
        assert_eq!(page.borrow().pixels, [0x12, 0x34, 0x56, 0xFF]);
    }

    #[test]
    fn what_the_page_is_told_reaches_the_machine() {
        let (mut host, page) = host();
        page.borrow_mut().gamepad.start = true;
        host.frame();
        assert!(host.cpu.memory.gamepad.start, "the gamepad not given");
        page.borrow_mut().reset = true;
        host.frame();
        let pc = host.cpu.pc;
        host.frame();
        assert_eq!(host.cpu.pc, pc, "the CPU ran, Reset held");
        assert_eq!(host.cpu.memory.title(), "NES   Test");
    }
}
