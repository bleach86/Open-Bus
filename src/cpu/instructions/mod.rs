pub mod arithmetic {
    mod adc;
    mod cmp;
    mod cpx;
    mod cpy;
    mod sbc;
}

pub mod branches {
    mod bit_branches;
    mod branch;
}

pub mod inc_dec {
    mod dec;
    mod dex;
    mod dey;
    mod inc;
    mod inx;
    mod iny;
}

pub mod jump_call {
    mod jmp;
    mod jsr;
    mod rts;
}

pub mod load_store {
    mod lda;
    mod ldx;
    mod ldy;
    mod sta;
    mod stx;
    mod sty;
    mod stz;
}

pub mod logical {
    mod and;
    mod bit;
    mod eor;
    mod ora;
    mod tsb_trb;
}

pub mod register_tx {
    mod register_transactions;
}

pub mod shifts {
    mod asl;
    mod lsr;
    mod rol;
    mod ror;
}

pub mod stack_ops {
    mod pha;
    mod php;
    mod phx;
    mod phy;
    mod pla;
    mod plp;
    mod plx;
    mod ply;
    mod tsx;
    mod txs;
}

pub mod status_flag_changes {
    mod flags;
}

pub mod system_functions {
    mod brk;
    mod interrupt;
    mod nop;
    mod rti;
    mod wai_stp;
}

/// The opcodes the 6502's makers didn't document, there all the same: the
/// rest of the 256, as the NMOS 6502 runs them.
pub mod undocumented {
    mod dcp;
    mod high_byte_stores;
    mod immediate;
    mod isc;
    mod jam;
    mod lax;
    mod rla;
    mod rra;
    mod sax;
    mod slo;
    mod sre;
}

pub mod op_codes;
