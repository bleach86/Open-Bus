pub mod arithmetic {
    mod adc;
    mod cmp;
    mod cpx;
    mod cpy;
    mod sbc;
}

pub mod branches {
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
}

pub mod logical {
    mod and;
    mod bit;
    mod eor;
    mod ora;
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
    mod pla;
    mod plp;
    mod tsx;
    mod txs;
}

pub mod status_flag_changes {
    mod status_flag_changes;
}

pub mod system_functions {
    mod brk;
    mod nop;
    mod rti;
}

pub mod op_codes;
